//! A small immediate-mode UI on top of the canvas.
//!
//! Every frame the app draws its whole window through a [`Frame`]. Each
//! interactive widget records a [`Hit`] (where it is and which [`Msg`] it
//! sends) and, while a screen reader is listening, an accessibility node.
//! Input is resolved against the previous frame's hits, so drawing and
//! behaviour can't drift apart. Nothing is redrawn unless something changed.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use accesskit::Role;
use tiny_skia::{Path, Transform};

use crate::app::Msg;
use crate::edit::TextEdit;
use crate::gfx::{Canvas, Color, Rect};
pub use crate::text::Style;
use crate::text::{Line, Text};
use crate::theme::Theme;

pub type Id = u64;

/// A stable widget id from anything hashable.
pub fn id(h: impl Hash) -> Id {
    let mut s = DefaultHasher::new();
    h.hash(&mut s);
    s.finish() | 1 // never 0 (the window)
}

pub const BODY: Style = Style::new(15.0, 400.0);
pub const STRONG: Style = Style::new(15.0, 600.0);
pub const SMALL: Style = Style::new(13.0, 400.0);
pub const CAPTION: Style = Style::new(12.0, 500.0);
pub const KEY: Style = Style::new(19.0, 400.0);
pub const TITLE: Style = Style::new(17.0, 600.0);
pub const RADIUS: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sense {
    Click,
    /// Pointer drags are routed to the app (graph panning, sliders).
    Drag,
    /// A text field: clicks place the caret.
    Text,
    /// A scroll container (wheel only).
    Scroll,
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub id: Id,
    pub rect: Rect,
    pub sense: Sense,
    pub msg: Option<Msg>,
    pub focusable: bool,
    /// Keypad keys leave Enter to the calculator ("=") when focused.
    pub keypad: bool,
}

#[derive(Default, Debug)]
pub struct Input {
    pub pointer: Option<(f32, f32)>,
    pub hover: Option<Id>,
    pub pressed: Option<Id>,
    pub focus: Option<Id>,
    /// Draw focus rings (keyboard navigation in use).
    pub focus_visible: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Scroll {
    pub offset: f32,
    pub content: f32,
    pub view: f32,
}

impl Scroll {
    pub fn max(&self) -> f32 {
        (self.content - self.view).max(0.0)
    }
}

/// One accessibility node (only collected while assistive tech listens).
#[derive(Clone, Debug)]
pub struct Node {
    pub id: Id,
    pub parent: Id,
    pub role: Role,
    pub label: String,
    pub value: Option<String>,
    pub rect: Rect,
    pub toggled: Option<bool>,
    pub selected: Option<bool>,
    pub disabled: bool,
    pub live: bool,
    pub clickable: bool,
    pub focusable: bool,
}

#[derive(Default)]
pub struct Icons {
    cache: HashMap<&'static str, Option<Path>>,
}

impl Icons {
    fn get(&mut self, d: &'static str) -> Option<&Path> {
        self.cache
            .entry(d)
            .or_insert_with(|| crate::svgpath::parse(d))
            .as_ref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

/// How a keypad key looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyLook {
    Number,
    Function,
    Operator,
    Equals,
    /// A toggle (2nd, hyp…), lit when on.
    Toggle(bool),
}

pub struct Frame<'a, 'p> {
    pub cv: Canvas<'p>,
    pub text: &'a mut Text,
    pub icons: &'a mut Icons,
    pub t: Theme,
    pub input: &'a Input,
    pub scrolls: &'a mut HashMap<Id, Scroll>,
    pub hits: Vec<Hit>,
    pub nodes: Option<Vec<Node>>,
    parents: Vec<Id>,
    clips: Vec<Option<Rect>>,
}

impl<'a, 'p> Frame<'a, 'p> {
    pub fn new(
        cv: Canvas<'p>,
        text: &'a mut Text,
        icons: &'a mut Icons,
        t: Theme,
        input: &'a Input,
        scrolls: &'a mut HashMap<Id, Scroll>,
        a11y: bool,
    ) -> Self {
        Frame {
            cv,
            text,
            icons,
            t,
            input,
            scrolls,
            hits: Vec::new(),
            nodes: a11y.then(Vec::new),
            parents: Vec::new(),
            clips: Vec::new(),
        }
    }

    pub fn width(&self) -> f32 {
        self.cv.width()
    }

    pub fn height(&self) -> f32 {
        self.cv.height()
    }

    // ------------------------------------------------------------ state

    pub fn hovered(&self, id: Id) -> bool {
        self.input.hover == Some(id)
    }

    pub fn pressed(&self, id: Id) -> bool {
        self.input.pressed == Some(id) && self.input.hover == Some(id)
    }

    pub fn focused(&self, id: Id) -> bool {
        self.input.focus == Some(id)
    }

    fn ring(&self, id: Id) -> bool {
        self.input.focus_visible && self.focused(id)
    }

    // ------------------------------------------------------------ plumbing

    /// Record an interactive region (clipped to the current clip).
    pub fn hit(&mut self, id: Id, rect: Rect, sense: Sense, msg: Option<Msg>, focusable: bool) {
        let rect = match self.cv.clip() {
            Some(c) => match rect.intersect(&c) {
                Some(r) => r,
                None => return,
            },
            None => rect,
        };
        self.hits.push(Hit {
            id,
            rect,
            sense,
            msg,
            focusable,
            keypad: false,
        });
    }

    /// Record an accessibility node under the current group.
    pub fn node(&mut self, id: Id, role: Role, label: &str, rect: Rect) -> Option<&mut Node> {
        let parent = self.parents.last().copied().unwrap_or(0);
        let nodes = self.nodes.as_mut()?;
        nodes.push(Node {
            id,
            parent,
            role,
            label: label.to_string(),
            value: None,
            rect,
            toggled: None,
            selected: None,
            disabled: false,
            live: false,
            clickable: false,
            focusable: false,
        });
        nodes.last_mut()
    }

    pub fn group(&mut self, id: Id, role: Role, label: &str, rect: Rect) {
        self.node(id, role, label, rect);
        self.parents.push(id);
    }

    pub fn end_group(&mut self) {
        self.parents.pop();
    }

    pub fn push_clip(&mut self, r: Rect) {
        let prev = self.cv.clip();
        let r = match prev {
            Some(p) => p.intersect(&r).unwrap_or(Rect::new(r.x, r.y, 0.0, 0.0)),
            None => r,
        };
        self.clips.push(prev);
        self.cv.set_clip(Some(r));
    }

    pub fn pop_clip(&mut self) {
        let prev = self.clips.pop().flatten();
        self.cv.set_clip(prev);
    }

    // ------------------------------------------------------------ text & icons

    pub fn layout(&mut self, s: &str, st: Style) -> Line {
        self.text.layout(s, st)
    }

    /// Draw a laid-out line inside `r`, vertically centred on capitals.
    pub fn draw_line(&mut self, line: &Line, r: Rect, align: Align, color: Color) {
        let x = match align {
            Align::Start => r.x,
            Align::Center => r.cx() - line.width / 2.0,
            Align::End => r.right() - line.width,
        };
        let y = r.cy() + line.cap / 2.0;
        self.text.draw(&mut self.cv, line, x, y, color);
    }

    pub fn label(&mut self, r: Rect, s: &str, st: Style, color: Color, align: Align) -> f32 {
        let line = self.text.layout(s, st);
        self.draw_line(&line, r, align, color);
        line.width
    }

    /// A label shrunk (down to `min` px) to fit `r`'s width.
    pub fn label_fit(&mut self, r: Rect, s: &str, st: Style, min: f32, color: Color, align: Align) {
        let mut st = st;
        let mut line = self.text.layout(s, st);
        if line.width > r.w && line.width > 0.0 {
            st.size = (st.size * r.w / line.width).max(min);
            line = self.text.layout(s, st);
        }
        self.draw_line(&line, r, align, color);
    }

    /// Word-wrap `text` to `w`; returns the lines.
    pub fn wrap(&mut self, text: &str, w: f32, st: Style) -> Vec<String> {
        let mut out = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split(' ') {
                let cand = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{line} {word}")
                };
                if !line.is_empty() && self.text.width(&cand, st) > w {
                    out.push(std::mem::take(&mut line));
                    line = word.to_string();
                } else {
                    line = cand;
                }
            }
            out.push(line);
        }
        out
    }

    /// Draw wrapped text from (x, y); returns the height used.
    pub fn paragraph(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        text: &str,
        st: Style,
        color: Color,
    ) -> f32 {
        let lh = (st.size * 1.4).round();
        let lines = self.wrap(text, w, st);
        let clip = self.cv.clip();
        for (i, l) in lines.iter().enumerate() {
            let ly = y + i as f32 * lh;
            if clip.is_some_and(|c| ly > c.bottom() || ly + lh < c.y) {
                continue;
            }
            self.label(Rect::new(x, ly, w, lh), l, st, color, Align::Start);
        }
        lines.len() as f32 * lh
    }

    /// Stroke one of the 24×24 line icons centred in `r` at `size` px.
    pub fn icon(&mut self, r: Rect, d: &'static str, size: f32, color: Color) {
        let k = size / 24.0;
        let t =
            Transform::from_scale(k, k).post_translate(r.cx() - size / 2.0, r.cy() - size / 2.0);
        if let Some(p) = self.icons.get(d).cloned() {
            self.cv.stroke_path_t(&p, color, 1.8 * k, None, t);
        }
    }

    fn focus_ring(&mut self, r: Rect, radius: f32) {
        let c = self.t.accent_text;
        self.cv.rounded_border(r.inset(-2.0), radius + 2.0, c, 2.0);
    }

    // ------------------------------------------------------------ widgets

    /// A keypad key.
    #[allow(clippy::too_many_arguments)]
    pub fn key(
        &mut self,
        id: Id,
        r: Rect,
        label: &str,
        icon: Option<&'static str>,
        look: KeyLook,
        name: &str,
        msg: Msg,
        enabled: bool,
    ) {
        let t = self.t;
        let (bg, fg) = match look {
            KeyLook::Number => (t.surface, t.fg),
            KeyLook::Function => (t.surface2, t.fg),
            KeyLook::Operator => (t.surface2, t.accent_text),
            KeyLook::Equals => (t.accent, t.on_accent),
            KeyLook::Toggle(true) => (t.accent, t.on_accent),
            KeyLook::Toggle(false) => (t.surface2, t.fg),
        };
        self.cv.rounded(r, RADIUS, bg);
        if enabled && self.pressed(id) {
            self.cv.rounded(r, RADIUS, t.press);
        } else if enabled && self.hovered(id) {
            self.cv.rounded(r, RADIUS, t.hover);
        }
        let fg = if enabled { fg } else { fg.alpha(0.35) };
        match icon {
            Some(d) => self.icon(r, d, 20.0, fg),
            None => {
                let st = if matches!(look, KeyLook::Number) {
                    Style::new(KEY.size + 2.0, 500.0)
                } else {
                    KEY
                };
                let line = self.text.layout_markup(label, st);
                let line = if line.width > r.w - 8.0 {
                    let st = Style {
                        size: st.size * (r.w - 8.0) / line.width,
                        ..st
                    };
                    self.text.layout_markup(label, st)
                } else {
                    line
                };
                self.draw_line(&line, r, Align::Center, fg);
            }
        }
        if self.ring(id) {
            self.focus_ring(r, RADIUS);
        }
        if enabled {
            self.hit(id, r, Sense::Click, Some(msg), true);
            if let Some(h) = self.hits.last_mut().filter(|h| h.id == id) {
                h.keypad = true;
            }
        }
        let toggled = match look {
            KeyLook::Toggle(on) => Some(on),
            _ => None,
        };
        if let Some(n) = self.node(id, Role::Button, name, r) {
            n.toggled = toggled;
            n.disabled = !enabled;
            n.clickable = enabled;
            n.focusable = enabled;
        }
    }

    /// A text button. `on`: Some(..) makes it a toggle / tab.
    #[allow(clippy::too_many_arguments)]
    pub fn button(
        &mut self,
        id: Id,
        r: Rect,
        label: &str,
        st: Style,
        msg: Msg,
        enabled: bool,
        on: Option<bool>,
        fill: bool,
    ) {
        let t = self.t;
        let lit = on == Some(true);
        if lit {
            self.cv.rounded(r, RADIUS, t.accent);
        } else if fill {
            self.cv.rounded(r, RADIUS, t.surface2);
        }
        if enabled && self.pressed(id) {
            self.cv.rounded(r, RADIUS, t.press);
        } else if enabled && self.hovered(id) {
            self.cv.rounded(r, RADIUS, t.hover);
        }
        let fg = match (enabled, lit) {
            (false, _) => t.fg_faint,
            (true, true) => t.on_accent,
            (true, false) => t.fg,
        };
        let inner = r.inset_xy(6.0, 0.0);
        self.label_fit(inner, label, st, 10.0, fg, Align::Center);
        if self.ring(id) {
            self.focus_ring(r, RADIUS);
        }
        if enabled {
            self.hit(id, r, Sense::Click, Some(msg), true);
        }
        let plain = crate::app::plain(label);
        if let Some(n) = self.node(id, Role::Button, &plain, r) {
            n.toggled = on;
            n.disabled = !enabled;
            n.clickable = enabled;
            n.focusable = enabled;
        }
    }

    /// A round icon button with an accessible name (and tooltip-like label
    /// for assistive tech). `on` makes it a toggle.
    #[allow(clippy::too_many_arguments)]
    pub fn icon_button(
        &mut self,
        id: Id,
        r: Rect,
        d: &'static str,
        name: &str,
        msg: Msg,
        enabled: bool,
        on: Option<bool>,
    ) {
        let t = self.t;
        let rad = r.w.min(r.h) / 2.0;
        if on == Some(true) {
            self.cv.rounded(r, rad, t.accent.alpha(0.18));
        }
        if enabled && self.pressed(id) {
            self.cv.rounded(r, rad, t.press);
        } else if enabled && self.hovered(id) {
            self.cv.rounded(r, rad, t.hover);
        }
        let c = match (enabled, on) {
            (false, _) => t.fg_faint,
            (true, Some(true)) => t.accent_text,
            _ => t.fg,
        };
        self.icon(r, d, 18.0, c);
        if self.ring(id) {
            self.focus_ring(r, rad);
        }
        if enabled {
            self.hit(id, r, Sense::Click, Some(msg), true);
        }
        if let Some(n) = self.node(id, Role::Button, name, r) {
            n.toggled = on;
            n.disabled = !enabled;
            n.clickable = enabled;
            n.focusable = enabled;
        }
    }

    /// A selectable row (lists, menus). Draws its background; the caller
    /// draws the content.
    pub fn row(&mut self, id: Id, r: Rect, msg: Msg, selected: bool, name: &str) {
        let t = self.t;
        if selected {
            self.cv.rounded(r, 6.0, t.accent.alpha(0.16));
        }
        if self.pressed(id) {
            self.cv.rounded(r, 6.0, t.press);
        } else if self.hovered(id) {
            self.cv.rounded(r, 6.0, t.hover);
        }
        if self.ring(id) {
            self.focus_ring(r, 6.0);
        }
        self.hit(id, r, Sense::Click, Some(msg), true);
        if let Some(n) = self.node(id, Role::ListBoxOption, name, r) {
            n.selected = Some(selected);
            n.clickable = true;
            n.focusable = true;
        }
    }

    /// A single-line text field.
    pub fn text_field(
        &mut self,
        id: Id,
        r: Rect,
        e: &TextEdit,
        placeholder: &str,
        error: bool,
        name: &str,
    ) {
        let t = self.t;
        let focused = self.focused(id);
        self.cv.rounded(r, 6.0, t.surface);
        let border = if error {
            t.danger
        } else if focused {
            t.accent_text
        } else {
            t.border
        };
        self.cv
            .rounded_border(r, 6.0, border, if focused || error { 2.0 } else { 1.0 });
        let inner = r.inset_xy(10.0, 0.0);
        self.push_clip(inner);
        let shown = if e.preedit.is_empty() {
            e.text.clone()
        } else {
            let mut s = e.text.clone();
            s.insert_str(e.cursor, &e.preedit);
            s
        };
        let line = self.text.layout(&shown, BODY);
        // Keep the caret in view.
        let caret = line.caret_x(e.cursor + e.preedit.len());
        let scroll = (caret - inner.w + 2.0).max(0.0);
        let x0 = inner.x - scroll;
        let base = inner.cy() + line.cap / 2.0;
        if focused && e.has_selection() {
            let (a, b) = e.selection();
            let (xa, xb) = (line.caret_x(a), line.caret_x(b));
            self.cv.fill_rect(
                Rect::new(x0 + xa, inner.cy() - 10.0, xb - xa, 20.0),
                t.accent.alpha(0.30),
            );
        }
        if shown.is_empty() {
            self.label(inner, placeholder, BODY, t.fg_faint, Align::Start);
        } else {
            self.text.draw(&mut self.cv, &line, x0, base, t.fg);
        }
        if !e.preedit.is_empty() {
            let (a, b) = (
                line.caret_x(e.cursor),
                line.caret_x(e.cursor + e.preedit.len()),
            );
            self.cv
                .fill_rect(Rect::new(x0 + a, base + 3.0, b - a, 1.0), t.fg);
        }
        if focused {
            self.cv.fill_rect(
                Rect::new((x0 + caret).round(), inner.cy() - 10.0, 1.5, 20.0),
                t.fg,
            );
        }
        self.pop_clip();
        self.hit(id, r, Sense::Text, None, true);
        if let Some(n) = self.node(id, Role::TextInput, name, r) {
            n.value = Some(e.text.clone());
            n.focusable = true;
            n.clickable = true;
        }
    }

    /// A horizontal slider; dragging is handled by the app via its id.
    pub fn slider(&mut self, id: Id, r: Rect, frac: f32, name: &str, value: &str) {
        let t = self.t;
        let frac = frac.clamp(0.0, 1.0);
        let track = Rect::new(r.x + 8.0, r.cy() - 2.0, r.w - 16.0, 4.0);
        self.cv.rounded(track, 2.0, t.border);
        self.cv.rounded(
            Rect::new(track.x, track.y, track.w * frac, 4.0),
            2.0,
            t.accent,
        );
        let kx = track.x + track.w * frac;
        self.cv.circle(kx, r.cy(), 8.0, t.accent);
        if self.hovered(id) || self.pressed(id) {
            self.cv.circle(kx, r.cy(), 12.0, t.accent.alpha(0.16));
        }
        if self.ring(id) {
            self.focus_ring(Rect::new(kx - 9.0, r.cy() - 9.0, 18.0, 18.0), 9.0);
        }
        self.hit(id, r, Sense::Drag, None, true);
        if let Some(n) = self.node(id, Role::Slider, name, r) {
            n.value = Some(value.to_string());
            n.focusable = true;
        }
    }

    /// A raised card / popup background. Clicks on its blank areas stay
    /// inside it (they don't fall through to the scrim and close it).
    pub fn card(&mut self, r: Rect, radius: f32) {
        self.surface(r, radius);
        self.hit(
            id(("card", r.x.to_bits(), r.y.to_bits())),
            r,
            Sense::Click,
            None,
            false,
        );
    }

    /// A raised, non-interactive surface (tooltips, bubbles).
    pub fn surface(&mut self, r: Rect, radius: f32) {
        let t = self.t;
        // A soft shadow from a couple of translucent outlines.
        for (d, a) in [(6.0, 0.05), (3.0, 0.07), (1.0, 0.10)] {
            self.cv
                .rounded(r.inset(-d), radius + d, Color([0.0, 0.0, 0.0, a]));
        }
        self.cv.rounded(r, radius, t.surface);
        self.cv.rounded_border(r, radius, t.border, 1.0);
    }

    /// Dim everything and close the overlay when clicked outside it.
    pub fn scrim(&mut self, msg: Msg, dim: bool) {
        let r = Rect::new(0.0, 0.0, self.width(), self.height());
        if dim {
            self.cv.fill_rect(r, Color([0.0, 0.0, 0.0, 0.30]));
        }
        self.hit(id("scrim"), r, Sense::Click, Some(msg), false);
    }

    /// Begin a vertical scroll area; returns the scroll offset to subtract.
    pub fn scroll_begin(&mut self, id: Id, r: Rect) -> f32 {
        self.hit(id, r, Sense::Scroll, None, false);
        self.push_clip(r);
        let s = self.scrolls.entry(id).or_default();
        s.view = r.h;
        s.offset = s.offset.clamp(0.0, s.max());
        s.offset
    }

    /// End a scroll area whose content was `content` tall.
    pub fn scroll_end(&mut self, id: Id, r: Rect, content: f32) {
        self.pop_clip();
        let t = self.t;
        let s = self.scrolls.entry(id).or_default();
        s.content = content;
        s.offset = s.offset.clamp(0.0, s.max());
        if s.max() > 0.0 {
            let h = (r.h * r.h / content).max(24.0);
            let y = r.y + (r.h - h) * (s.offset / s.max());
            self.cv
                .rounded(Rect::new(r.right() - 5.0, y, 3.0, h), 1.5, t.fg_faint);
        }
    }

    /// Scroll so that `item` (in content coordinates) is visible.
    pub fn reveal(&mut self, id: Id, top: f32, bottom: f32) {
        if let Some(s) = self.scrolls.get_mut(&id) {
            if top < s.offset {
                s.offset = top;
            } else if bottom > s.offset + s.view {
                s.offset = bottom - s.view;
            }
        }
    }
}
