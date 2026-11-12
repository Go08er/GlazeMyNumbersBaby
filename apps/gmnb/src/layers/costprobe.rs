//! The frame-cost probe: an ignored test that checks what a theme's frame is
//! charged (`gmnb_theme::cost`) against what the software renderer (GSK's
//! cairo renderer) really spends drawing it, kind of call by kind of call.
//!
//! For each case it records a layer with `reps` of one drawing operation,
//! through the same replay the app uses (`replay::replay`), renders the
//! picture with `gsk::CairoRenderer`, and prints the real milliseconds
//! (the median of several warm frames, in thread CPU time), the charge
//! (1000 points, the share 1.0, are 100 ms over 760 x 640) and their ratio.
//! A ratio above 1.5 is a call the charge under-states; below 0.3, one it
//! over-states. It ends with a summary table of the worst of each.
//!
//! See `tools/theming/README.md`, "The frame-cost probe", for how to run it
//! and for its settings.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gmnb_theme::layer::{DisplayValue, Frame, GlyphBox, Glyphs, LayerKind, Role};
use gmnb_theme::theme::{Source, Theme};
use gmnb_theme::{Ctx, Limits};
use gtk::prelude::*;
use gtk::{gdk, graphene, gsk};

use super::replay;

/// What the points of a frame are, in milliseconds: 1000 of them (the share
/// 1.0) are 100 ms over 760 x 640.
const MS_PER_SHARE: f64 = 100.0;

/// Processor time of this thread, so that other work on the machine does
/// not move the numbers.
struct Cpu(u64);

impl Cpu {
    fn now() -> Cpu {
        let s = std::fs::read_to_string("/proc/thread-self/schedstat").ok();
        let ns = s
            .and_then(|s| s.split_whitespace().next().and_then(|n| n.parse().ok()))
            .unwrap_or_else(|| {
                // No schedstat: wall time.
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64)
            });
        Cpu(ns)
    }

    fn ms(&self) -> f64 {
        (Cpu::now().0.saturating_sub(self.0)) as f64 / 1e6
    }
}

/// One thing to time: a kind of call, with its parameters, drawn `reps`
/// times (`@N@` in `body`).
struct Case {
    group: &'static str,
    name: String,
    /// Luau before the hook (pictures).
    pre: String,
    /// The hook's body: `w` and `h` are the layer's size, `frame` its frame.
    body: String,
    /// The display layer, with its two glyphs, rather than the background.
    display: bool,
    /// A fixed number of repeats (a panel of keys), not as many as fill a
    /// share of the allowance.
    fixed: Option<usize>,
}

fn case(group: &'static str, name: impl Into<String>, body: impl Into<String>) -> Case {
    Case {
        group,
        name: name.into(),
        pre: String::new(),
        body: body.into(),
        display: false,
        fixed: None,
    }
}

impl Case {
    fn pre(mut self, pre: &str) -> Case {
        self.pre = pre.into();
        self
    }

    fn display(mut self) -> Case {
        self.display = true;
        self
    }

    fn fixed(mut self, n: usize) -> Case {
        self.fixed = Some(n);
        self
    }

    fn source(&self, reps: usize) -> String {
        format!(
            r##"{pre}
local inter = {{ family = "Inter Variable", size = 18, weight = 400 }}
return {{ api = 1, name = "Probe", layers = {{ {layer} = {{ draw = function(draw, frame)
local w, h = frame.width, frame.height
{body}
end }} }} }}"##,
            pre = self.pre,
            layer = if self.display { "display" } else { "background" },
            body = self.body.replace("@N@", &reps.to_string()),
        )
    }
}

// Translucent, so that the toolkit can't skip what is covered over.
const LIN2: &str = r##"gmnb.linear({ angle = 30, colors = { "#336699cc", "#cc9933cc" } })"##;
const LIN16: &str = r##"gmnb.linear({ angle = 30, colors = { "#ff0000cc", "#ff8000cc", "#ffff00cc", "#80ff00cc", "#00ff00cc", "#00ff80cc", "#00ffffcc", "#0080ffcc", "#0000ffcc", "#8000ffcc", "#ff00ffcc", "#ff0080cc", "#ff0000cc", "#00ff00cc", "#0000ffcc", "#ffffffcc" } })"##;
const RADIAL: &str = r##"gmnb.radial({ at = { 0.5, 0.5 }, radius = "farthest-corner", colors = { "#ff8040cc", "#80ff40cc", "#4080ffcc", "#202020cc" } })"##;
const CONIC: &str = r##"gmnb.conic({ at = { 0.5, 0.5 }, angle = 45, colors = { "#ff0000cc", "#00ff00cc", "#0000ffcc", "#ff0000cc" } })"##;
const COLOR: &str = r##""#336699cc""##;

/// A loop of `@N@` calls of `call`, with `i` the count.
fn rep(call: &str) -> String {
    format!("for i = 1, @N@ do\n{call}\nend")
}

/// Every case: the kinds of call and parameters the charge has a weight for.
fn cases() -> Vec<Case> {
    let mut v: Vec<Case> = Vec::new();
    let paints: [(&str, &str); 5] = [
        ("color", COLOR),
        ("lin2", LIN2),
        ("lin16", LIN16),
        ("radial", RADIAL),
        ("conic", CONIC),
    ];

    // Rectangles: the whole layer, and small ones.
    for (n, p) in paints {
        v.push(case("rect", format!("full_{n}"), rep(&format!("draw:rect(0, 0, w, h, {p})"))));
    }
    // The same gradient axis-aligned, and opaque: the cost is the paint's.
    v.push(case("rect", "full_lin2_axis", rep(r##"draw:rect(0, 0, w, h, gmnb.linear({ angle = 90, colors = { "#336699cc", "#cc9933cc" } }))"##)));
    v.push(case("rect", "full_lin2_opaque", rep(r##"draw:rect(0, 0, w, h, gmnb.linear({ angle = 30, colors = { "#336699", "#cc9933" } }))"##)));
    v.push(case("rect", "full_lin2_axis_opaque", rep(r##"draw:rect(0, 0, w, h, gmnb.linear({ angle = 90, colors = { "#336699", "#cc9933" } }))"##)));
    v.push(case("rect", "small20_color", rep(&format!("draw:rect((i * 7) % 700, (i * 5) % 600, 20, 20, {COLOR})"))));
    v.push(case("rect", "small20_conic", rep(&format!("draw:rect((i * 7) % 700, (i * 5) % 600, 20, 20, {CONIC})"))));

    // Rounded rectangles, by radius (200 x 200 with a gradient).
    for r in [0, 4, 10, 30, 100, 999] {
        v.push(case("rrect", format!("200_r{r}_color"), rep(&format!("draw:rounded_rect(40 + i % 400, 40, 200, 200, {r}, {COLOR})"))));
        v.push(case("rrect", format!("200_r{r}_lin2"), rep(&format!("draw:rounded_rect(40 + i % 400, 40, 200, 200, {r}, {LIN2})"))));
    }
    v.push(case("rrect", "full_r30_color", rep(&format!("draw:rounded_rect(0, 0, w, h, 30, {COLOR})"))));
    v.push(case("rrect", "tiny_r4_color", rep(&format!("draw:rounded_rect((i * 7) % 700, (i * 5) % 600, 20, 20, 4, {COLOR})"))));

    // Circles.
    v.push(case("circle", "r300_color", rep(&format!("draw:circle(380, 320, 300, {COLOR})"))));
    v.push(case("circle", "r300_lin2", rep(&format!("draw:circle(380, 320, 300, {LIN2})"))));
    v.push(case("circle", "r300_radial", rep(&format!("draw:circle(380, 320, 300, {RADIAL})"))));
    v.push(case("circle", "r10_color", rep(&format!("draw:circle((i * 7) % 700 + 10, (i * 5) % 600 + 10, 10, {COLOR})"))));

    // Fills: a path with curves, filled over the window.
    let blob = r#"gmnb.path():move_to(10, 10):line_to(w - 10, 20):quad_to(w, h / 2, w - 20, h - 10):curve_to(w / 2, h, 100, h - 40, 10, h - 10):close()"#;
    for (n, p) in [("color", COLOR), ("lin2", LIN2)] {
        v.push(case("fill", format!("blob_{n}"), rep(&format!("draw:fill({blob}, {p})"))));
    }
    v.push(case("fill", "evenodd_color", rep(&format!(
        "draw:fill(gmnb.path():rect(10, 10, w - 20, h - 20):circle(w / 2, h / 2, 250), {COLOR}, \"even-odd\")"
    ))));

    // Strokes: a wide curve; dashes; dashes with round caps.
    let wave = r#"local p = gmnb.path():move_to(0, h / 2)
for k = 1, 40 do p = p:quad_to((k - 0.5) * w / 40, h / 2 + (if k % 2 == 0 then 200 else -200), k * w / 40, h / 2) end"#;
    for (n, stroke) in [
        ("w4", "{ width = 4 }"),
        ("w30_round", "{ width = 30, cap = \"round\", join = \"round\" }"),
        ("dash4", "{ width = 2, dash = { 4, 4 } }"),
        ("dash1_round", "{ width = 3, cap = \"round\", dash = { 0, 4 } }"),
    ] {
        v.push(case("stroke", n, format!("{wave}\nfor i = 1, @N@ do draw:stroke(p, {stroke}, {COLOR}) end")));
    }
    v.push(case("stroke", "w4_lin2", format!("{wave}\nfor i = 1, @N@ do draw:stroke(p, {{ width = 4 }}, {LIN2}) end")));

    // Path strain: a star whose lines cross one another, filled and stroked.
    let star = |n: usize, k: usize| {
        format!(
            "local p = gmnb.path()\nfor j = 0, {n} - 1 do local a = 2 * math.pi * j * {k} / {n}\n\
             local x, y = w / 2 + 300 * math.cos(a), h / 2 + 300 * math.sin(a)\n\
             if j == 0 then p = p:move_to(x, y) else p = p:line_to(x, y) end end\np = p:close()"
        )
    };
    for (n, k) in [(101, 40), (401, 160)] {
        v.push(case("strain", format!("fill_star{n}"), format!("{}\nfor i = 1, @N@ do draw:fill(p, {COLOR}) end", star(n, k))));
        v.push(case("strain", format!("stroke_star{n}"), format!("{}\nfor i = 1, @N@ do draw:stroke(p, {{ width = 2 }}, {COLOR}) end", star(n, k))));
    }

    // Borders.
    v.push(case("border", "full_r0_w2", rep("draw:border(0, 0, w, h, 0, 2, \"#ffffff\")")));
    v.push(case("border", "full_r30_w2", rep("draw:border(0, 0, w, h, 30, 2, \"#ffffff\")")));
    v.push(case("border", "full_r300_w60", rep("draw:border(0, 0, w, h, 300, 60, \"#ffffff\")")));

    // Box shadows: what drives the cost (inset against outset, size, blur,
    // radius and spread).
    let shadow = |name: String, inset: bool, size: i32, blur: i32, radius: i32, spread: i32| {
        let x = if size >= 700 { "10".to_string() } else { format!("(i * 37) % {}", 760 - size.min(740)) };
        let y = if size >= 600 { "10".to_string() } else { format!("(i * 53) % {}", 640 - size.min(620)) };
        case(
            "shadow",
            name,
            rep(&format!(
                "draw:shadow({x}, {y}, {size}, {size}, {radius}, {{ inset = {inset}, blur = {blur}, spread = {spread}, color = \"#000000c0\" }})"
            )),
        )
    };
    for inset in [false, true] {
        let io = if inset { "in" } else { "out" };
        for size in [4, 20, 50, 100, 200, 400, 740] {
            for blur in [0, 3, 10, 20, 30, 50, 100] {
                let radius = 8.min(size / 2);
                v.push(shadow(format!("{io}_s{size}_b{blur}_r{radius}"), inset, size, blur, radius, 0));
                if size / 2 > radius {
                    // The corners as large as the box lets them be.
                    v.push(shadow(format!("{io}_s{size}_b{blur}_rmax"), inset, size, blur, size / 2, 0));
                }
            }
        }
        for size in [200, 740] {
            for radius in [0, 20, 100, 200, 370] {
                if radius <= size / 2 {
                    v.push(shadow(format!("{io}_s{size}_b30_r{radius}"), inset, size, 30, radius, 0));
                }
            }
        }
        for spread in [-20, 20, 60] {
            v.push(shadow(format!("{io}_s200_b30_r8_p{spread}"), inset, 200, 30, 8, spread));
        }
        // A key: 70 x 50, rounded 6.
        for blur in [10, 30, 100] {
            v.push(
                case(
                    "shadow",
                    format!("{io}_key_b{blur}"),
                    rep(&format!(
                        "draw:shadow(5 + (i % 10) * 75, 5 + (i // 10 % 12) * 52, 70, 50, 6, {{ inset = {inset}, blur = {blur}, color = \"#000000c0\" }})"
                    )),
                ),
            );
        }
        // A realistic frame: a shadow under each of 36 keys, blur 30.
        v.push(
            case(
                "shadow",
                format!("{io}_keys36_b30"),
                rep(&format!(
                    "draw:shadow(5 + (i % 6) * 120, 5 + (i // 6) * 100, 110, 90, 6, {{ inset = {inset}, blur = 30, color = \"#000000c0\" }})"
                )),
            )
            .fixed(36),
        );
    }
    // push_shadow around a rectangle, a key's and a panel's.
    for blur in [0, 10, 30, 100] {
        v.push(case("push_shadow", format!("rect200_b{blur}"), format!(
            "for i = 1, @N@ do draw:push_shadow({{ {{ x = 2, y = 4, blur = {blur}, color = \"#000000c0\" }} }})\ndraw:rect(100, 100, 200, 200, {COLOR})\ndraw:pop() end"
        )));
    }
    v.push(case("push_shadow", "full_b30", format!(
        "for i = 1, @N@ do draw:push_shadow({{ {{ x = 2, y = 4, blur = 30, color = \"#000000c0\" }} }})\ndraw:rect(0, 0, w, h, {COLOR})\ndraw:pop() end"
    )));

    // Blur.
    for r in [1, 10, 30, 100] {
        v.push(case("blur", format!("full_r{r}"), format!(
            "for i = 1, @N@ do draw:push_blur({r})\ndraw:rect(0, 0, w, h, {LIN2})\ndraw:pop() end"
        )));
    }
    v.push(case("blur", "small_content_r30", "for i = 1, @N@ do draw:push_blur(30)\ndraw:rect(300, 300, 20, 20, \"#ffffff\")\ndraw:pop() end"));

    // Opacity.
    v.push(case("opacity", "full", format!("for i = 1, @N@ do draw:push_opacity(0.5)\ndraw:rect(0, 0, w, h, {COLOR})\ndraw:pop() end")));
    v.push(case("opacity", "small_many", format!("for i = 1, @N@ do draw:push_opacity(0.5)\ndraw:rect((i * 7) % 700, (i * 5) % 600, 20, 20, {COLOR})\ndraw:pop() end")));

    // The four masks, the mask a window-sized gradient and a small one.
    for mode in ["alpha", "inverted-alpha", "luminance", "inverted-luminance"] {
        v.push(case("mask", format!("{mode}_full"), format!(
            "for i = 1, @N@ do draw:push_mask(\"{mode}\")\ndraw:rect(0, 0, w, h, {LIN2})\ndraw:pop()\ndraw:rect(0, 0, w, h, {COLOR})\ndraw:pop() end"
        )));
        v.push(case("mask", format!("{mode}_tiny"), format!(
            "for i = 1, @N@ do draw:push_mask(\"{mode}\")\ndraw:rect(0, 0, 4, 4, \"#ffffff\")\ndraw:pop()\ndraw:rect(0, 0, w, h, {COLOR})\ndraw:pop() end"
        )));
    }

    // Blend modes, each over the window.
    for mode in [
        "multiply", "screen", "overlay", "darken", "lighten", "color-dodge", "color-burn",
        "hard-light", "soft-light", "difference", "exclusion", "color", "hue", "saturation",
        "luminosity",
    ] {
        v.push(case("blend", mode, format!(
            "for i = 1, @N@ do draw:push_blend(\"{mode}\")\ndraw:rect(0, 0, w, h, {LIN2})\ndraw:pop()\ndraw:rect(0, 0, w, h, {RADIAL})\ndraw:pop() end"
        )));
    }

    // Clips: a window-sized rectangle drawn inside a rounded clip, by
    // radius; inside many turned ones.
    for r in [0, 4, 10, 30, 100] {
        v.push(case("clip", format!("rounded_r{r}"), format!(
            "for i = 1, @N@ do draw:push_clip(0, 0, w, h, {r})\ndraw:rect(0, 0, w, h, {COLOR})\ndraw:pop() end"
        )));
    }
    v.push(case("clip", "nested30_rect", format!(
        "for i = 1, 30 do draw:push_clip(0, 0, w, h, 40) end\nfor i = 1, @N@ do draw:rect(0, 0, w, h, {COLOR}) end\nfor i = 1, 30 do draw:pop() end"
    )));
    v.push(case("clip", "turned", format!(
        "draw:rotate(10)\nfor i = 1, @N@ do draw:push_clip(100, 100, 400, 300)\ndraw:rect(0, 0, w, h, {COLOR})\ndraw:pop() end"
    )));

    // Text: Latin; many characters; a heavy script; a font for each size.
    v.push(case("text", "latin_short", rep(&format!("draw:text(\"Hello world 123\", (i * 7) % 600, (i * 5) % 600, inter, {COLOR})"))));
    v.push(case("text", "latin_gradient", rep(&format!("draw:text(\"Hello world 123\", (i * 7) % 600, (i * 5) % 600, inter, {LIN2})"))));
    v.push(case("text", "latin_big200", rep(&format!("draw:text(\"Hello\", 10, (i * 5) % 400, {{ family = \"Inter Variable\", size = 200 }}, {COLOR})"))));
    v.push(case("text", "latin_many_chars", rep(&format!(
        "draw:text(\"The quick brown fox jumps over the lazy dog; PACK MY BOX WITH FIVE DOZEN LIQUOR JUGS 0123456789\", 10, (i * 7) % 600, inter, {COLOR})"
    ))));
    let cjk = "local s = ''\nfor c = 0x4E00, 0x4E00 + 84 do s = s .. utf8.char(c) end";
    v.push(case("text", "cjk_85_warm", format!("{cjk}\nfor i = 1, @N@ do draw:text(s, 10, (i * 7) % 600, inter, {COLOR}) end")));
    // The sizes move each frame: the glyphs are made anew.
    v.push(case("text", "cjk_85_moving", format!(
        "{cjk}\nlocal size = 10 + (frame.time * 1000) % 40\nfor i = 1, @N@ do draw:text(s, 10, (i * 7) % 600, {{ family = \"Inter Variable\", size = size }}, {COLOR}) end"
    )));
    v.push(case("text", "latin_moving", format!(
        "local size = 10 + (frame.time * 1000) % 40\nfor i = 1, @N@ do draw:text(\"The quick brown fox jumps over the lazy dog 0123456789\", 10, (i * 7) % 600, {{ family = \"Inter Variable\", size = size }}, {COLOR}) end"
    )));
    v.push(case("text", "arabic_warm", format!("for i = 1, @N@ do draw:text(string.rep(\"مرحبا\", 12), 10, (i * 7) % 600, inter, {COLOR}) end")));
    v.push(case("text", "emoji_warm", format!("for i = 1, @N@ do draw:text(\"😀😁😂🤣😃😄😅😆😉😊\", 10, (i * 7) % 600, inter, {COLOR}) end")));
    v.push(case("text", "fonts_moving_weight", format!(
        "for i = 1, @N@ do draw:text(\"Hello\", 10, (i * 7) % 600, {{ family = \"Inter Variable\", size = 18, weight = 100 + (i * 7 + frame.time * 1000) % 800 }}, {COLOR}) end"
    )));

    // A glyph of the display layer.
    v.push(case("glyph", "color_size120", "for i = 1, @N@ do for _, g in frame.value.glyphs do draw:glyph(g, \"#ffffff\") end end").display());
    v.push(case("glyph", "gradient_size120", format!("for i = 1, @N@ do for _, g in frame.value.glyphs do draw:glyph(g, {LIN2}) end end")).display());

    // Pictures: scaled into a rectangle, by the picture's own size and
    // filter; repeated.
    for (size, filter) in [(512, "trilinear"), (512, "nearest"), (2048, "trilinear"), (2048, "linear")] {
        let pre = format!("local pic = gmnb.noise({{ size = {size}, seed = 3 }})");
        v.push(case("picture", format!("{size}_full_{filter}"), format!("for i = 1, @N@ do draw:image(pic, 0, 0, w, h, {{ filter = \"{filter}\" }}) end")).pre(&pre));
        v.push(case("picture", format!("{size}_small76_{filter}"), format!("for i = 1, @N@ do draw:image(pic, (i * 7) % 600, 10, 76, 64, {{ filter = \"{filter}\" }}) end")).pre(&pre));
    }
    for size in [16, 128, 512, 2048] {
        let pre = format!("local pic = gmnb.noise({{ size = {size}, seed = 3 }})");
        v.push(case("tile", format!("{size}_full"), "for i = 1, @N@ do draw:tile(pic, 0, 0, w, h) end").pre(&pre));
        v.push(case("tile", format!("{size}_small10"), "for i = 1, @N@ do draw:tile(pic, (i * 7) % 600, 10, 10, 10) end").pre(&pre));
    }
    v
}

/// What a case came to.
struct Row {
    group: &'static str,
    name: String,
    reps: usize,
    /// The charge in ms, and the real cost in ms (above an empty frame's).
    charge: f64,
    real: f64,
    note: String,
}

impl Row {
    fn ratio(&self) -> f64 {
        if self.charge > 0.0 { self.real / self.charge } else { f64::INFINITY }
    }
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

struct Rig {
    dir: PathBuf,
    pango: gtk::pango::Context,
    renderer: gsk::CairoRenderer,
    size: (f32, f32),
    frames: usize,
    cache: replay::Cache,
    told: Glyphs,
    glyphs: Vec<replay::Glyph>,
    /// An empty frame's rendering, ms.
    base: f64,
}

impl Rig {
    fn theme(&self, source: &str) -> Result<Theme, String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        std::fs::write(self.dir.join("theme.luau"), source).map_err(|e| e.to_string())?;
        let limits = Limits {
            load: Duration::from_secs(60),
            frame: Duration::from_secs(60),
            ..Limits::default()
        };
        let theme = Theme::load(Source::folder("t", &self.dir), limits).map_err(|e| e.to_string())?;
        theme.api().set_measure(Rc::new(replay::Measure(self.pango.clone())));
        Ok(theme)
    }

    /// One frame recorded and (if `render`) drawn: its charge as a share,
    /// and the milliseconds the toolkit took.
    fn frame(&self, theme: &Theme, display: bool, n: usize, render: bool) -> Result<(f64, f64), String> {
        let (w, h) = self.size;
        let mut frame = Frame::sized(w, h);
        frame.time = n as f64 * 0.37;
        if display {
            frame.value = Some(DisplayValue {
                readout: 1,
                version: 1,
                text: "42",
                glyphs: &self.told,
                band: [0.0, 0.0, w, h],
                error: false,
                align_end: true,
                role: Role::Calculator,
                change: None,
                previous: None,
            });
        }
        let kind = if display { LayerKind::Display } else { LayerKind::Background };
        let drawn = theme.draw(kind, &Ctx::default(), &frame).map_err(|e| format!("{e:?}"))?;
        let share = theme.api().frame_cost();
        if !render {
            return Ok((share, 0.0));
        }
        let s = gtk::Snapshot::new();
        let scene = replay::Scene {
            cache: &self.cache,
            theme: Some(theme),
            pango: &self.pango,
            glyphs: [&self.glyphs, &[]],
            scale: 1.0,
        };
        replay::replay(&s, &drawn.list, &scene);
        let node = s.to_node().ok_or("no node")?;
        let bounds = graphene::Rect::new(0.0, 0.0, w, h);
        let t = Cpu::now();
        let texture = self.renderer.render_texture(&node, Some(&bounds));
        let ms = t.ms();
        // Reading the texture back is not drawing it, but the toolkit may
        // draw lazily.
        let mut px = vec![0u8; (w as usize) * (h as usize) * 4];
        texture.download(&mut px, w as usize * 4);
        Ok((share, ms))
    }

    fn run(&self, c: &Case, target: f64) -> Row {
        let mut row = Row {
            group: c.group,
            name: c.name.clone(),
            reps: 0,
            charge: 0.0,
            real: 0.0,
            note: String::new(),
        };
        // Once, to see what one repeat is charged.
        let one = match self.theme(&c.source(1)).and_then(|t| self.frame(&t, c.display, 0, false)) {
            Ok((share, _)) => share,
            Err(e) => {
                row.note = format!("N=1 refused: {}", e.chars().take(60).collect::<String>());
                return row;
            }
        };
        let mut reps = c.fixed.unwrap_or_else(|| {
            if one > 0.0 { ((target / one) as usize).clamp(1, 3000) } else { 200 }
        });
        for _ in 0..6 {
            let theme = match self.theme(&c.source(reps)) {
                Ok(t) => t,
                Err(e) => {
                    row.note = format!("load: {e}");
                    return row;
                }
            };
            let first = self.frame(&theme, c.display, 0, true);
            let Ok((share, ms0)) = first else {
                // Over its allowance: fewer.
                if c.fixed.is_some() {
                    row.reps = reps;
                    row.note = "refused".into();
                    return row;
                }
                reps = (reps * 6 / 10).max(1);
                continue;
            };
            let frames = if ms0 > 1500.0 { 2 } else { self.frames };
            let mut times = vec![ms0];
            for n in 1..=frames {
                match self.frame(&theme, c.display, n, true) {
                    Ok((_, ms)) => times.push(ms),
                    Err(e) => {
                        row.note = e;
                        return row;
                    }
                }
            }
            // The first frame is the warm-up (fonts, glyphs, pictures).
            let real = median(&mut times[1..]);
            row.reps = reps;
            row.charge = share * MS_PER_SHARE;
            row.real = (real - self.base).max(0.0);
            if row.real < 3.0 {
                row.note = "noisy (<3 ms)".into();
            }
            return row;
        }
        row.note = "refused at every N".into();
        row
    }
}

fn glyph_fixtures(pango: &gtk::pango::Context) -> (Glyphs, Vec<replay::Glyph>) {
    let font = gmnb_theme::types::Font::new(&["Inter Variable".to_string()], 120.0, 400.0);
    let glyphs: Vec<replay::Glyph> = ["4", "2"]
        .iter()
        .enumerate()
        .map(|(i, text)| replay::Glyph {
            layout: replay::layout(pango, text, &font),
            x: 20.0 + i as f32 * 90.0,
            y: 30.0,
        })
        .collect();
    let told = Glyphs {
        glyphs: glyphs.iter().map(|g| GlyphBox { x: g.x, y: g.y, w: 85.0, h: 140.0 }).collect(),
        size: 120.0,
        rect: [20.0, 30.0, 180.0, 140.0],
    };
    (told, glyphs)
}

#[test]
#[ignore = "the frame-cost probe; see tools/theming/README.md: GMNB_PROBE_ONLY=shadow cargo test --release -p gmnb costprobe -- --ignored --nocapture"]
fn probe() {
    gtk::init().expect("GTK on the test's display");
    let dir = std::env::var_os("GMNB_PROBE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("gmnb-costprobe-{}", std::process::id())));
    std::fs::create_dir_all(&dir).expect("a folder");
    let font = dir.join("Inter.ttf");
    std::fs::write(&font, crate::INTER).expect("the font");
    crate::add_font_file(&font);
    let pango = pangocairo::FontMap::default().create_context();
    let size: Vec<f32> = std::env::var("GMNB_PROBE_SIZE")
        .unwrap_or("760x640".into())
        .split('x')
        .map(|s| s.parse().expect("GMNB_PROBE_SIZE=WxH"))
        .collect();
    let frames: usize = std::env::var("GMNB_PROBE_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
    // The charge each case is sized to (as a share of the allowance).
    let target: f64 = std::env::var("GMNB_PROBE_TARGET").ok().and_then(|v| v.parse().ok()).unwrap_or(0.3);
    let only: Vec<String> = std::env::var("GMNB_PROBE_ONLY")
        .map(|v| v.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    let renderer = gsk::CairoRenderer::new();
    renderer.realize(None::<&gdk::Surface>).expect("the cairo renderer");
    let (told, glyphs) = glyph_fixtures(&pango);
    let mut rig = Rig {
        dir: dir.join("theme"),
        pango,
        renderer,
        size: (size[0], size[1]),
        frames,
        cache: replay::Cache::default(),
        told,
        glyphs,
        base: 0.0,
    };
    // An empty frame's cost: a rectangle of one pixel.
    let empty = case("base", "empty", "draw:rect(0, 0, 1, 1, \"#000000\")");
    let theme = rig.theme(&empty.source(1)).expect("the empty theme");
    let mut times: Vec<f64> = (0..=frames + 2)
        .map(|n| rig.frame(&theme, false, n, true).expect("an empty frame").1)
        .collect();
    rig.base = median(&mut times[1..]);
    println!("PROBE size {}x{}, frames {frames}, target {target}, an empty frame {:.2} ms", size[0], size[1], rig.base);
    println!("PROBE {:12} {:28} {:>5} {:>9} {:>9} {:>7}  note", "group", "case", "reps", "charge ms", "real ms", "ratio");
    let mut rows = Vec::new();
    for c in cases() {
        let id = format!("{}/{}", c.group, c.name);
        if !only.is_empty() && !only.iter().any(|o| id.contains(o.as_str())) {
            continue;
        }
        let row = rig.run(&c, target);
        println!(
            "PROBE {:12} {:28} {:>5} {:>9.2} {:>9.2} {:>7.2}  {}",
            row.group, row.name, row.reps, row.charge, row.real, row.ratio(), row.note
        );
        rows.push(row);
    }
    rig.renderer.unrealize();
    summary(&rows);
}

/// The worst of each kind: what the charge under-states (real over
/// charge, at least 3 ms real) and what it over-states.
fn summary(rows: &[Row]) {
    let mut real: Vec<&Row> = rows.iter().filter(|r| r.charge > 0.0 && r.real >= 3.0).collect();
    real.sort_by(|a, b| b.ratio().total_cmp(&a.ratio()));
    println!("PROBE");
    println!("PROBE SUMMARY: under-charged (real / charge), the dearest first; a ratio over 1.5 is a call the charge under-states");
    println!("PROBE {:12} {:28} {:>5} {:>9} {:>9} {:>7}", "group", "case", "reps", "charge ms", "real ms", "ratio");
    for r in real.iter().take(25) {
        println!("PROBE {:12} {:28} {:>5} {:>9.2} {:>9.2} {:>7.2}", r.group, r.name, r.reps, r.charge, r.real, r.ratio());
    }
    println!("PROBE");
    println!("PROBE SUMMARY: over-charged (the cheapest ratios; a ratio under 0.3 is a call the charge over-states)");
    for r in real.iter().rev().take(15) {
        println!("PROBE {:12} {:28} {:>5} {:>9.2} {:>9.2} {:>7.2}", r.group, r.name, r.reps, r.charge, r.real, r.ratio());
    }
    println!("PROBE");
    println!("PROBE SUMMARY: the worst ratio of each group");
    let mut groups: Vec<&'static str> = rows.iter().map(|r| r.group).collect();
    groups.dedup();
    for g in groups {
        let worst = real.iter().filter(|r| r.group == g).max_by(|a, b| a.ratio().total_cmp(&b.ratio()));
        if let Some(r) = worst {
            println!("PROBE {:12} {:28} {:>5} {:>9.2} {:>9.2} {:>7.2}", r.group, r.name, r.reps, r.charge, r.real, r.ratio());
        }
    }
    let refused: Vec<_> = rows.iter().filter(|r| r.charge == 0.0).collect();
    for r in refused {
        println!("PROBE unmeasured {}/{}: {}", r.group, r.name, r.note);
    }
}
