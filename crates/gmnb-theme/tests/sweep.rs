//! The built-in themes' own scripts as code, swept: every option at
//! every value and the combinations that interact, light and dark, at
//! opacities 0 to 1, with and without a desktop accent and with motion
//! off; each layer at odd sizes on every page with odd events and odd
//! timings; the `sound` hooks; the options' `visible`, `actions` and
//! previews. The readability helpers are css.rs's, copied. The three
//! longest sweeps (Aurora's and Swirls' styles, Swirls' layers: about
//! three minutes together) are ignored: `--release -- --ignored`.

use std::collections::BTreeMap;

use appcore::color::{TEXT_CONTRAST, UI_CONTRAST, contrast};
use gmnb_theme::states::{Which, over};
use gmnb_theme::theme::{self, OptionDef, OptionKind, Source, Theme};
use gmnb_theme::types::*;
use gmnb_theme::{Ctx, Limits, OptionValue, Style, css};

const THEMES: [&str; 3] = ["aurora", "sprinkles", "swirls"];

/// The limits a theme really has when `GMNB_REAL_LIMITS` is set; otherwise
/// four times each hook's time. The sweeps check what the themes do, not
/// how fast the machine running them is (CI's runners and the packages'
/// builders are slower than the limits are set for, in unoptimised builds
/// too); `tests/swirls.rs` and `tests/media.rs` hold the budgets.
fn limits() -> Limits {
    let real = Limits::default();
    if std::env::var_os("GMNB_REAL_LIMITS").is_some() {
        return real;
    }
    Limits {
        load: real.load * 4,
        style: real.style * 4,
        frame: real.frame * 4,
        preview: real.preview * 4,
        visible: real.visible * 4,
        action: real.action * 4,
        place: real.place * 4,
        ..real
    }
}

fn built_in(id: &str) -> Theme {
    Theme::load(Source::built_in(id).unwrap(), limits()).unwrap()
}

/// A small theme of the test's own (docs/theming.md's "A theme in twenty
/// lines"): the base colours worked out from an accent option, and corners.
/// It is what stands for any theme that takes its colours from
/// `gmnb.base`.
const ACCENT_THEME: &str = r##"return {
	api = 1,
	name = "Typewriter",
	options = {
		{ id = "accent", type = "color", label = "Accent", default = "#e5484d" },
		{ id = "square", type = "toggle", label = "Square corners", default = true },
	},
	style = function(ctx)
		local corner = if ctx.options.square then 2 else 8
		return {
			colors = gmnb.base(ctx.dark, ctx.options.accent),
			fonts = { ui = "monospace", display = "monospace" },
			shape = { radius = corner, row_radius = corner, card_radius = corner * 2 },
			keys = { gap = 2 },
		}
	end,
}"##;

fn accent_theme() -> Theme {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "gmnb-sweep-accent-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("theme.luau"), ACCENT_THEME).unwrap();
    let theme = Theme::load(Source::folder("typewriter", &dir), limits()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    theme
}

fn text(s: &str) -> OptionValue {
    OptionValue::Text(s.into())
}

const COLOURS: [&str; 6] = [
    "#000000", "#ffffff", "#808080", "#ffff00", "#0000ff", "#7c4dff",
];

/// A repeatable stream of numbers.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn hex(&mut self) -> String {
        format!("#{:06x}", self.next() & 0xffffff)
    }
    fn unit(&mut self) -> f32 {
        (self.next() % 10_000) as f32 / 10_000.0
    }
}

fn ctx_of(
    theme: &Theme,
    dark: bool,
    opacity: f32,
    accent: Option<[f32; 3]>,
    motion: bool,
    options: &BTreeMap<String, OptionValue>,
) -> Ctx {
    Ctx {
        dark,
        opacity,
        accent,
        motion,
        options: theme::complete_options(&theme.def.options, options),
        ..Ctx::default()
    }
}

/// Every value an option is swept over (a colour from a short list).
fn values_of(o: &OptionDef) -> Vec<OptionValue> {
    match &o.kind {
        OptionKind::Choice { choices, .. } => choices.iter().map(|c| text(&c.id)).collect(),
        OptionKind::Toggle => vec![OptionValue::Bool(false), OptionValue::Bool(true)],
        OptionKind::Slider { min, max, step, .. } => {
            let mut v = vec![*min, *max, (min + max) / 2.0, min + step];
            v.dedup();
            v.into_iter().map(OptionValue::Number).collect()
        }
        OptionKind::Color => COLOURS.iter().map(|c| text(c)).collect(),
        OptionKind::Note => Vec::new(),
    }
}

/// Every set of saved options the sweep visits: all of the choices,
/// toggles and sliders at once (colours at their defaults), then, for each
/// value of each choice, every colour from the list (two colours: all
/// pairs; three: 36 of the triples).
fn option_sets(theme: &Theme) -> Vec<BTreeMap<String, OptionValue>> {
    let defs = &theme.def.options;
    let plain: Vec<&OptionDef> = defs
        .iter()
        .filter(|o| !matches!(o.kind, OptionKind::Color | OptionKind::Note))
        .collect();
    let colours: Vec<&OptionDef> = defs
        .iter()
        .filter(|o| matches!(o.kind, OptionKind::Color))
        .collect();
    let mut sets = vec![BTreeMap::new()];
    for o in &plain {
        let mut next = Vec::new();
        for set in &sets {
            for v in values_of(o) {
                let mut s = set.clone();
                s.insert(o.id.clone(), v);
                next.push(s);
            }
        }
        sets = next;
    }
    let choices: Vec<&OptionDef> = plain
        .iter()
        .copied()
        .filter(|o| matches!(o.kind, OptionKind::Choice { .. }))
        .collect();
    if !colours.is_empty() {
        let mut pick = Vec::new();
        for o in &choices {
            for v in values_of(o) {
                pick.push((o.id.clone(), v));
            }
        }
        if pick.is_empty() {
            pick.push((String::new(), text("")));
        }
        let mut tuples: Vec<BTreeMap<String, OptionValue>> = vec![BTreeMap::new()];
        for c in &colours {
            let mut next = Vec::new();
            for t in &tuples {
                for cv in values_of(c) {
                    let mut t = t.clone();
                    t.insert(c.id.clone(), cv);
                    next.push(t);
                }
            }
            tuples = next;
        }
        // Three colours or more (Swirls' Freestyle): 36 of the tuples,
        // spread through them, as two colours give.
        if tuples.len() > 36 {
            let n = tuples.len();
            tuples = (0..36).map(|i| tuples[i * 37 % n].clone()).collect();
        }
        for (id, v) in pick {
            for t in &tuples {
                let mut s = t.clone();
                if !id.is_empty() {
                    s.insert(id.clone(), v.clone());
                }
                sets.push(s);
            }
        }
    }
    sets
}

fn bad_number(s: &str) -> bool {
    s.contains("NaN") || s.contains("inf")
}

fn rgb3(c: Rgba) -> [f32; 3] {
    [c[0], c[1], c[2]]
}

/// The lowest contrast of `text` on `fills` (topmost first) over
/// `backdrop`: gradients at both ends and the middle, a translucent text
/// colour composited too.
fn worst(text: Rgba, fills: &[&Fill], backdrop: &[Rgba; 3]) -> f32 {
    let mut under = *backdrop;
    for fill in fills.iter().rev() {
        for (i, slot) in under.iter_mut().enumerate() {
            *slot = fill.samples_over(*slot)[i];
        }
    }
    under
        .iter()
        .map(|b| contrast(rgb3(over(text, *b)), rgb3(*b)))
        .fold(f32::INFINITY, f32::min)
}

/// What the window's background shows as: itself at full opacity; below
/// it, over a desktop.
fn backdrop(style: &Style, desktop: Option<Rgba>) -> [Rgba; 3] {
    let bg = style.fill("window.background");
    let samples = match desktop {
        None => bg.samples_over(style.tokens.colors.bg),
        Some(desktop) => {
            let faded = Fill(
                bg.0.iter()
                    .map(|p| match p {
                        Paint::Color(c) => Paint::Color(alpha(*c, style.opacity)),
                        Paint::Gradient(g) => {
                            let mut g = g.clone();
                            for c in &mut g.colors {
                                *c = alpha(*c, style.opacity);
                            }
                            Paint::Gradient(g)
                        }
                    })
                    .collect(),
            );
            faded.samples_over(desktop)
        }
    };
    [samples[0], samples[1], samples[2]]
}

/// The contrast a text needs: 4.5, or 3 where it is large (24 px, or 19 px
/// in bold).
fn needed(font: &Font) -> f32 {
    if font.size >= 24.0 || (font.size >= 19.0 && font.weight >= 700.0) {
        UI_CONTRAST
    } else {
        TEXT_CONTRAST
    }
}

/// The fixed table of (what, text colour, the contrast it needs, the fills
/// beneath it, topmost first) for the elements this slice resolves. The
/// window's background is under them all. `plain_text`: whether text
/// straight on the backdrop is in it.
fn pairs(style: &Style, plain_text: bool) -> Vec<(String, Rgba, f32, Vec<Fill>)> {
    let mut out = Vec::new();
    let plain = style.fill("plain.background");
    // Text straight on the backdrop (the memory row...), over the scrim.
    // On a see-through window it counts where the style has a scrim there
    // (`unreadable` says for which styles).
    let body = match style.get("window.font") {
        Some(Value::Font(f)) => f.clone(),
        _ => panic!("no window.font"),
    };
    if plain_text {
        out.push((
            "window text".to_string(),
            style.color("window.color"),
            needed(&body),
            vec![plain.clone()],
        ));
    }
    // The keys, and a short keypad's (`keys.short`).
    for keys in ["keys", "keys.short"] {
        for class in ["number", "func", "operator", "equals"] {
            let key = style.control(&format!("{keys}.{class}")).unwrap();
            let rest = key.finished(&[]);
            // The operators' glyphs (÷ × − +) are symbols, not text to
            // read: 3, as for an icon. (DGMNB's own light palette puts
            // them at 4.1 on its second surface.)
            let need = if class == "operator" {
                UI_CONTRAST
            } else {
                needed(&key.font)
            };
            out.push((
                format!("{keys}.{class}"),
                rest.color,
                need,
                vec![rest.background],
            ));
        }
        // A toggle that is on.
        let func = style.control(&format!("{keys}.func")).unwrap();
        let on = func.finished(&[Which::Checked]);
        out.push((
            format!("{keys}.on"),
            on.color,
            needed(&func.font),
            vec![on.background],
        ));
    }
    // The toolkit draws a native family itself, in the stock colours.
    if !style.native().contains(&"segmented") {
        for (name, path, under) in [
            // The date page's and the graph's switches sit in plain
            // containers.
            ("segmented", "segmented.item", plain.clone()),
            // The History/Memory switch sits on the panel (the tokens'
            // second surface, until `panel` is resolved).
            (
                "panel.tabs",
                "panel.tabs.item",
                Fill::color(style.tokens.colors.surface2),
            ),
        ] {
            let item = style.control(path).unwrap();
            for (state, states) in [("", &[][..]), (" checked", &[Which::Checked][..])] {
                let f = item.finished(states);
                out.push((
                    format!("{name}{state}"),
                    f.color,
                    needed(&item.font),
                    vec![f.background, under.clone()],
                ));
            }
        }
    }
    out
}

/// A Text's colour and the contrast it needs.
fn text_of(style: &Style, path: &str) -> (Rgba, f32) {
    match style.get(path) {
        Some(Value::Text(t)) => (t.color, needed(&t.font)),
        other => panic!("{path}: {other:?}"),
    }
}

/// The chrome's pairs: the header, the sidebar, the calculator's rows, the
/// History and Memory panel and the programmer's radix list and bits.
///
/// A selected row's label (the sidebar's current page, the current radix)
/// is `accent_text` on the accent at .16: DGMNB's own colours give it
/// 3.6:1 light and 4.4:1 dark, so it is held to 3, as a mark of the
/// selection its tint shows too (docs/theming.md, open point 11; moving
/// `accent_text` would change DGMNB's pixels).
fn chrome_pairs(style: &Style) -> Vec<(String, Rgba, f32, Vec<Fill>)> {
    let mut out = Vec::new();
    let plain = &style.fill("plain.background");
    let control = |path: &str| style.control(path).unwrap_or_else(|| panic!("no {path}"));
    // A control's label at rest and in `states`, over `under` (topmost
    // first) and its own background.
    let mut states = |name: &str, path: &str, which: &[&[Which]], under: &[Fill]| {
        let c = control(path);
        for states in which {
            let f = c.finished(states);
            let mut fills = vec![f.background];
            fills.extend(under.iter().cloned());
            let state = if states.is_empty() { "" } else { " checked" };
            let selection = !states.is_empty() && name == "nav.row";
            let need = if selection {
                UI_CONTRAST
            } else {
                needed(&c.font)
            };
            out.push((format!("{name}{state}"), f.color, need, fills));
        }
    };
    let rest_and_on: &[&[Which]] = &[&[], &[Which::Checked]];
    let header = style.fill("header.background");
    let nav = style.fill("nav.background");
    let panel = match style.get("panel") {
        Some(Value::Surface(s)) => s.background.clone(),
        other => panic!("panel: {other:?}"),
    };
    // On the backdrop's scrims.
    states(
        "text_button",
        "text_button",
        rest_and_on,
        std::slice::from_ref(plain),
    );
    states(
        "flyout_button",
        "flyout_button",
        rest_and_on,
        std::slice::from_ref(plain),
    );
    states(
        "programmer.bit",
        "programmer.bit",
        rest_and_on,
        std::slice::from_ref(plain),
    );
    states(
        "programmer.radix.row",
        "programmer.radix.row",
        rest_and_on,
        std::slice::from_ref(plain),
    );
    // On the sidebar, and on the panel.
    states(
        "nav.row",
        "nav.row",
        rest_and_on,
        std::slice::from_ref(&nav),
    );
    states(
        "panel.memory_button",
        "panel.memory_button",
        &[&[]],
        std::slice::from_ref(&panel),
    );
    let mut texts = |name: &str, under: Vec<Fill>| {
        let (color, need) = text_of(style, name);
        out.push((name.to_string(), color, need, under));
    };
    texts("header.title", vec![header.clone()]);
    texts("nav.title", vec![nav.clone()]);
    texts("nav.header", vec![nav.clone()]);
    texts("panel.expression", vec![panel.clone()]);
    texts("panel.result", vec![panel.clone()]);
    texts("panel.empty", vec![panel.clone()]);
    texts("programmer.radix.value", vec![plain.clone()]);
    texts("programmer.radix.name", vec![plain.clone()]);
    // The readout: its expression, and its value (and its error colour),
    // on its own scrim (`display.background`).
    let readout = style.fill("display.background");
    texts("display.expression", vec![readout.clone()]);
    for path in ["display.value.color", "display.value.error_color"] {
        let colors = match style.get(path) {
            Some(Value::Paint(Paint::Color(c))) => vec![*c],
            Some(Value::Paint(Paint::Gradient(g))) => g.colors.clone(),
            other => panic!("{path}: {other:?}"),
        };
        let font = match style.get("display.value.font") {
            Some(Value::Font(f)) => f.clone(),
            other => panic!("display.value.font: {other:?}"),
        };
        for c in colors {
            out.push((path.to_string(), c, needed(&font), vec![readout.clone()]));
        }
    }
    // The current radix: its name in its own colour, on the checked row.
    let row = control("programmer.radix.row").finished(&[Which::Checked]);
    out.push((
        "programmer.radix.name checked".to_string(),
        style.color("programmer.radix.name_checked"),
        UI_CONTRAST,
        vec![row.background, plain.clone()],
    ));
    out
}

/// What a field's placeholder needs: DGMNB's light palette puts `fg_faint`
/// at 2.49:1 on `surface`, and the base look is DGMNB's (docs/theming.md,
/// "Readability rules", open point 9).
const PLACEHOLDER_CONTRAST: f32 = appcore::palette::FAINT_CONTRAST;

/// The date page's, the converter's and graphing's texts, and the
/// toolkit families' the app draws (fields, popovers, toasts, tooltips).
fn page_pairs(style: &Style, plain_text: bool) -> Vec<(String, Rgba, f32, Vec<Fill>)> {
    let mut out = Vec::new();
    let c = &style.tokens.colors;
    let plain = style.fill("plain.background");
    let surface = |path: &str| match style.get(path) {
        Some(Value::Surface(s)) => s.background.clone(),
        _ => panic!("no surface at {path}"),
    };
    let text = |path: &str| match style.get(path) {
        Some(Value::Text(t)) => t.clone(),
        _ => panic!("no text at {path}"),
    };
    let part = |path: &str, name: &str| match style.get(path) {
        Some(Value::Record(r)) => r.get(name).cloned(),
        _ => None,
    };
    // Text straight on a page, over the scrim (on a see-through window:
    // where the style has one, as `unreadable` says).
    if plain_text {
        for path in [
            "date.label",
            "date.subresult",
            "converter.symbol",
            "converter.caption",
            "converter.ratio",
            "converter.timestamp",
            "graph.error",
            "graph.variables",
            "graph.variable",
            "graph.analysis.title",
            "graph.analysis.card_title",
            "graph.analysis.value",
            "graph.analysis.text",
            "graph.analysis.note",
        ] {
            let t = text(path);
            out.push((
                path.to_string(),
                t.color,
                needed(&t.font),
                vec![plain.clone()],
            ));
        }
        // A result's and a converter value's size and font, in the text
        // colour.
        for path in ["date.result", "date.add_result", "converter.value"] {
            if let Some(Value::Font(f)) = part(path, "font") {
                let size = match part(path, "size") {
                    Some(Value::Number(n)) => n as f32,
                    _ => f.size,
                };
                let f = Font { size, ..f };
                out.push((
                    path.to_string(),
                    style.color("window.color"),
                    needed(&f),
                    vec![plain.clone()],
                ));
            }
        }
        // Controls on a page: at rest and checked (a lit date or unit
        // button, the whimsy chip).
        for path in [
            "date.field",
            "converter.unit",
            "converter.chip",
            "converter.link",
            "graph.add",
        ] {
            let control = style.control(path).unwrap();
            for (state, states) in [("", &[][..]), (" checked", &[Which::Checked][..])] {
                let f = control.finished(states);
                out.push((
                    format!("{path}{state}"),
                    f.color,
                    needed(&control.font),
                    vec![f.background, plain.clone()],
                ));
            }
        }
    }
    // Fields: the text and the placeholder (3) on the field.
    if !style.native().contains(&"field") {
        for path in ["field", "graph.entry"] {
            let Some(Value::Field(f)) = style.get(path) else {
                panic!("no field at {path}");
            };
            let under = vec![f.background.clone(), plain.clone()];
            out.push((path.to_string(), f.color, needed(&f.font), under.clone()));
            out.push((
                format!("{path} placeholder"),
                f.placeholder,
                PLACEHOLDER_CONTRAST,
                under,
            ));
        }
    }
    // A popover's texts: the calendar's, the flyouts' headings, menu rows.
    let popover = surface("popover");
    for path in ["date.calendar.title", "date.calendar.weekday", "section"] {
        let t = text(path);
        out.push((
            path.to_string(),
            t.color,
            needed(&t.font),
            vec![popover.clone()],
        ));
    }
    // A card's heading (Settings' groups, the graph's analysis), on the
    // card.
    if !style.native().contains(&"card") {
        let t = text("card.heading");
        out.push((
            "card.heading".to_string(),
            t.color,
            needed(&t.font),
            vec![surface("card")],
        ));
    }
    let day = style.control("date.calendar.day").unwrap();
    for (state, states) in [("", &[][..]), (" checked", &[Which::Checked][..])] {
        let f = day.finished(states);
        out.push((
            format!("date.calendar.day{state}"),
            f.color,
            needed(&day.font),
            vec![f.background, popover.clone()],
        ));
    }
    if !style.native().contains(&"menu") {
        let menu = surface("menu");
        let row = style.control("row").unwrap();
        for (state, states) in [("", &[][..]), (" checked", &[Which::Checked][..])] {
            let f = row.finished(states);
            out.push((
                format!("row{state}"),
                f.color,
                needed(&row.font),
                vec![f.background, menu.clone()],
            ));
        }
    }
    // Toasts and tooltips float over the page.
    for (family, path) in [("toast", "toast"), ("tooltip", "tooltip")] {
        if style.native().contains(&family) {
            continue;
        }
        let (Some(Value::Fill(bg)), Some(Value::Color(fg)), Some(Value::Font(f))) = (
            part(path, "background"),
            part(path, "color"),
            part(path, "font"),
        ) else {
            panic!("no {path}");
        };
        out.push((
            path.to_string(),
            fg,
            needed(&f),
            vec![bg, Fill::color(c.bg)],
        ));
    }
    // The canvas: its labels (text), its axes and every graph colour (3).
    let canvas = match style.get("graph.canvas.background") {
        Some(Value::Paint(p)) => Fill(vec![p.clone()]),
        _ => panic!("no canvas"),
    };
    // (The canvas sits straight on the backdrop: `graphing.rs` puts it in
    // no plain container, so no scrim is counted under it.)
    let label = text("graph.canvas.label");
    out.push((
        "graph.canvas.label".into(),
        label.color,
        needed(&label.font),
        vec![canvas.clone()],
    ));
    if let Some(Value::Color(axis)) = part("graph.canvas.axis", "color") {
        out.push((
            "graph.canvas.axis".into(),
            axis,
            UI_CONTRAST,
            vec![canvas.clone()],
        ));
    }
    for (i, s) in c.series.iter().enumerate() {
        out.push((format!("series {i}"), *s, UI_CONTRAST, vec![canvas.clone()]));
    }
    // The canvas's toolbar buttons, on the toolbar over the canvas.
    let toolbar = surface("graph.toolbar");
    let tool = style.control("graph.toolbar_button").unwrap();
    for (state, states) in [("", &[][..]), (" checked", &[Which::Checked][..])] {
        let f = tool.finished(states);
        out.push((
            format!("graph.toolbar_button{state}"),
            f.icon_color,
            UI_CONTRAST,
            vec![f.background, toolbar.clone(), canvas.clone()],
        ));
    }
    out
}

/// Every pair of the fixed table that reads too faintly, for each of
/// `styles`.
///
/// Text straight on a page counts on a see-through window too wherever
/// the style puts a scrim under it (`plain.background`): the base look's,
/// and Aurora's frost. A theme that gives each element its own fill
/// there instead is checked through those elements' pairs.
fn unreadable(styles: Vec<(String, Style)>) -> Vec<String> {
    let mut bad = Vec::new();
    for (name, style) in styles {
        let base_look = name.starts_with("base");
        let desktops: Vec<Option<Rgba>> = if style.opacity >= 1.0 {
            vec![None]
        } else {
            vec![Some(BLACK), Some(WHITE)]
        };
        for desktop in desktops {
            let behind = backdrop(&style, desktop);
            let plain_text = desktop.is_none()
                || base_look
                || name.starts_with("aurora")
                || name.starts_with("sprinkles")
                || name.starts_with("swirls");
            let mut all = page_pairs(&style, plain_text);
            all.extend(chrome_pairs(&style));
            all.extend(pairs(&style, plain_text));
            for (what, text, at_rest, fills) in all {
                let fills: Vec<&Fill> = fills.iter().collect();
                let got = worst(text, &fills, &behind);
                let fg = text == style.tokens.colors.fg;
                let want = match desktop {
                    None => at_rest,
                    Some(_) if base_look && fg => TEXT_CONTRAST,
                    Some(_) => at_rest.min(UI_CONTRAST),
                };
                if got < want {
                    let over = match desktop {
                        None => "",
                        Some(d) if d == BLACK => " over black",
                        Some(_) => " over white",
                    };
                    bad.push(format!("{name}{over}: {what} {got:.4} < {want}"));
                }
            }
        }
    }
    bad
}

/// Every colour Aurora's window can show under what is in it, over
/// `desktop` (css.rs's, copied): the sky is the window's own background,
/// thinned by the app to the window's opacity; over it the background
/// layer, which thins itself, draws stars, and through a mask the
/// palette's lights (a gradient by height). Sampled at 17 heights: the
/// sky, the light, and three blends of them; and a star at its brightest.
fn auroras_sky_colours(
    aurora: &Theme,
    ctx: &Ctx,
    style: &Style,
    desktop: Option<Rgba>,
) -> Vec<Rgba> {
    use gmnb_theme::draw::Op;
    let Some(Paint::Gradient(sky)) = style.fill("window.background").0.first().cloned() else {
        panic!("Aurora's window.background is the sky's gradient");
    };
    let frame = Frame {
        motion: false,
        ..Frame::sized(760.0, 640.0)
    };
    let drawn = match aurora.draw(LayerKind::Background, ctx, &frame) {
        Ok(d) => d,
        Err(e) => panic!("Aurora's background: {e:?}"),
    };
    let mut lights = None;
    let mut star: Option<Rgba> = None;
    for op in &drawn.list.ops {
        match op {
            Op::Rect {
                paint: Paint::Gradient(g),
                ..
            } => lights = Some(g.clone()),
            Op::Rect {
                paint: Paint::Color(c),
                ..
            } if star.is_none_or(|s| c[3] > s[3]) => star = Some(*c),
            _ => {}
        }
    }
    let lights = lights.expect("the lights");
    let air = |sky: Rgba| match desktop {
        None => sky,
        Some(d) => over(alpha(sky, ctx.opacity), d),
    };
    let mut out = Vec::new();
    for k in 0..=16 {
        let t = k as f32 / 16.0;
        let (s, l) = (air(sky.at_stop(t)), lights.at_stop(t));
        for m in [0.0, 0.25, 0.5, 0.75, 1.0] {
            out.push(over(alpha(l, m), s));
        }
        if let Some(star) = star.filter(|_| k % 16 == 0) {
            out.push(over(star, s));
        }
    }
    out
}

/// Every pair of the fixed table that reads too faintly over Aurora's
/// own aurora: with each colour its window can show under it in turn.
fn unreadable_over_the_aurora(aurora: &Theme, ctx: &Ctx, name: &str, style: &Style) -> Vec<String> {
    let mut bad = Vec::new();
    let mut all = page_pairs(style, true);
    all.extend(chrome_pairs(style));
    all.extend(pairs(style, true));
    let desktops: &[Option<Rgba>] = if style.opacity >= 1.0 {
        &[None]
    } else {
        &[Some(BLACK), Some(WHITE)]
    };
    for desktop in desktops {
        let mut worst_of = vec![f32::INFINITY; all.len()];
        for shown in auroras_sky_colours(aurora, ctx, style, *desktop) {
            let behind = [shown; 3];
            for (i, (_, text, _, fills)) in all.iter().enumerate() {
                let fills: Vec<&Fill> = fills.iter().collect();
                worst_of[i] = worst_of[i].min(worst(*text, &fills, &behind));
            }
        }
        for ((what, _, at_rest, _), got) in all.iter().zip(worst_of) {
            let want = match desktop {
                None => *at_rest,
                Some(_) => at_rest.min(UI_CONTRAST),
            };
            if got < want {
                let over = match desktop {
                    None => "",
                    Some(d) if *d == BLACK => " over black",
                    Some(_) => " over white",
                };
                bad.push(format!(
                    "{name}{over}: over the aurora, {what} {got:.4} < {want}"
                ));
            }
        }
    }
    bad
}

// ---- Styles ------------------------------------------------------------------------

/// Every theme under every set of options, light and dark, at opacity 0 to
/// 1, with and without a desktop accent, with motion on and off: its
/// `style` runs without an error, a note or a log line, every number it
/// writes is finite, and every text reads on what is under it.
fn sweep_styles(id: &str, sets: Vec<BTreeMap<String, OptionValue>>) -> Vec<String> {
    sweep_styles_of(id, built_in(id), sets)
}

fn sweep_styles_of(
    id: &str,
    mut theme: Theme,
    sets: Vec<BTreeMap<String, OptionValue>>,
) -> Vec<String> {
    let mut bad = Vec::new();
    let accents: [Option<[f32; 3]>; 2] = [None, Some([1.0, 1.0, 0.0])];
    for set in &sets {
        let label: Vec<String> = set.iter().map(|(k, v)| format!("{k}={v:?}")).collect();
        for dark in [false, true] {
            for opacity in [0.0, 0.1, 0.5, 1.0] {
                for accent in accents {
                    for motion in [true, false] {
                        let ctx = ctx_of(&theme, dark, opacity, accent, motion, set);
                        let name = format!(
                            "{id} {} dark={dark} op={opacity} accent={accent:?} motion={motion}",
                            label.join(" ")
                        );
                        let resolved = match theme.style(&ctx) {
                            Ok(r) => r.clone(),
                            Err(e) => {
                                bad.push(format!("{name}: style failed: {e}"));
                                continue;
                            }
                        };
                        if !resolved.notes.list.is_empty() || resolved.notes.dropped > 0 {
                            bad.push(format!("{name}: notes {:?}", resolved.notes));
                        }
                        let log = theme.take_log();
                        if !log.is_empty() {
                            bad.push(format!("{name}: log {log:?}"));
                        }
                        for (k, v) in css::properties(&resolved.style) {
                            if bad_number(&v) {
                                bad.push(format!("{name}: {k} = {v}"));
                            }
                        }
                        // The readability tables are slower: the corners of
                        // the sweep (a see-through window, and not).
                        if accent.is_none() && motion && (opacity == 1.0 || opacity == 0.1) {
                            let named = vec![(name.clone(), resolved.style.clone())];
                            bad.extend(unreadable(named));
                            // Aurora's text lies over an aurora: over every
                            // colour of it.
                            if id == "aurora" {
                                bad.extend(unreadable_over_the_aurora(
                                    &theme,
                                    &ctx,
                                    &name,
                                    &resolved.style,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    bad
}

fn report(bad: Vec<String>) {
    let mut seen = std::collections::BTreeSet::new();
    let mut lines = Vec::new();
    for b in &bad {
        // One line per kind of failure: the part after the context.
        let tail = b.split_once(" events=").map_or(b.as_str(), |(_, t)| t);
        let kind: String = tail
            .split(": ")
            .skip(1)
            .collect::<Vec<_>>()
            .join(": ")
            .lines()
            .take(4)
            .collect::<Vec<_>>()
            .join("|");
        if seen.insert(kind) || std::env::var_os("SWEEP_ALL").is_some() {
            lines.push(b.clone());
        }
    }
    assert!(
        bad.is_empty(),
        "{} fail, {} kinds:\n{}",
        bad.len(),
        lines.len(),
        lines.into_iter().take(60).collect::<Vec<_>>().join("\n")
    );
}

#[test]
#[ignore = "a long sweep (a minute or more): cargo test --release -p gmnb-theme --test sweep -- --ignored"]
fn aurora_styles_sweep() {
    let theme = built_in("aurora");
    report(sweep_styles("aurora", option_sets(&theme)));
}

#[test]
fn sprinkles_styles_sweep() {
    let theme = built_in("sprinkles");
    report(sweep_styles("sprinkles", option_sets(&theme)));
}

#[test]
#[ignore = "a long sweep (a minute or more): cargo test --release -p gmnb-theme --test sweep -- --ignored"]
fn swirls_styles_sweep() {
    let theme = built_in("swirls");
    report(sweep_styles("swirls", option_sets(&theme)));
}

#[test]
fn accent_theme_styles_sweep() {
    let theme = accent_theme();
    let mut sets = option_sets(&theme);
    let mut rng = Lcg(7);
    for _ in 0..40 {
        for square in [true, false] {
            sets.push(BTreeMap::from([
                ("accent".to_string(), text(&rng.hex())),
                ("square".to_string(), OptionValue::Bool(square)),
            ]));
        }
    }
    report(sweep_styles_of("typewriter", theme, sets));
}

/// Aurora's Freestyle from random colours: rules 2, 3 and 4.
#[test]
fn aurora_freestyle_random_colours_read() {
    let mut rng = Lcg(11);
    let mut sets = Vec::new();
    for _ in 0..60 {
        sets.push(BTreeMap::from([
            ("palette".to_string(), text("custom")),
            ("primary".to_string(), text(&rng.hex())),
            ("secondary".to_string(), text(&rng.hex())),
        ]));
    }
    let _ = rng.unit();
    report(sweep_styles("aurora", sets));
}

/// Sprinkles' Freestyle iced in random colours: rules 3 and 4.
#[test]
fn sprinkles_freestyle_random_colours_read() {
    let mut rng = Lcg(13);
    let mut sets = Vec::new();
    for _ in 0..60 {
        sets.push(BTreeMap::from([
            ("flavour".to_string(), text("custom")),
            ("icing".to_string(), text(&rng.hex())),
            ("second".to_string(), text(&rng.hex())),
        ]));
    }
    report(sweep_styles("sprinkles", sets));
}

// ---- Layers ------------------------------------------------------------------------

use gmnb_theme::events::{Action, Cause, Event, EventKind, KeyKind, Page, Source as From, Via};
use gmnb_theme::layer::{
    ChangeKind, DisplayValue, Failure, Frame, GlyphBox, Glyphs, KeyCell, Keys, LayerKind, Role,
};

const SIZES: [(f32, f32); 9] = [
    (0.0, 0.0),
    (1.0, 1.0),
    (0.5, 0.25),
    (300.0, 400.0),
    (3840.0, 2160.0),
    (20000.0, 60.0),
    (60.0, 20000.0),
    (1.0, 400.0),
    (344.0, 0.0),
];

/// A keypad of `rows` by `cols` in `w` by `h`.
fn grid(rows: i32, cols: i32, w: f32, h: f32, ids: bool) -> Vec<KeyCell> {
    let mut cells = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let kind = match (row, col) {
                (r, c) if r == rows - 1 && c == cols - 1 => KeyKind::Equals,
                (_, c) if c == cols - 1 => KeyKind::Operator,
                (0, _) => KeyKind::Func,
                (1, 0) => KeyKind::Toggle,
                (1, 1) => KeyKind::Other,
                _ => KeyKind::Number,
            };
            let cw = w / cols as f32;
            let ch = h / rows as f32;
            cells.push(KeyCell {
                id: ids.then_some((row * cols + col) as u32),
                kind,
                rect: [col as f32 * cw, row as f32 * ch, cw * 0.95, ch * 0.95],
                row,
                col,
                radius: 6.0,
                enabled: (row + col) % 7 != 3,
                hovered: (row + col) % 5 == 1,
                pressed: (row + col) % 9 == 2,
                checked: (row + col) % 4 == 0,
            });
        }
    }
    cells
}

fn glyphs_of(n: usize, w: f32, h: f32, size: f32) -> Glyphs {
    let adv = if n == 0 {
        0.0
    } else {
        (w / n as f32).min(size * 0.5)
    };
    let x0 = w - adv * n as f32;
    Glyphs {
        glyphs: (0..n)
            .map(|i| GlyphBox {
                x: x0 + i as f32 * adv,
                y: h * 0.3,
                w: adv,
                h: h * 0.5,
            })
            .collect(),
        size,
        rect: [x0, h * 0.3, adv * n as f32, h * 0.5],
    }
}

/// Events of every kind, on every page, in odd places.
fn odd_events() -> Vec<Event> {
    let mut out = Vec::new();
    let rects: [[f32; 4]; 4] = [
        [0.0, 0.0, 0.0, 0.0],
        [10.0, 10.0, 80.0, 60.0],
        [-500.0, -500.0, 1e7, 1e7],
        [5000.0, 5000.0, 3.0, 3.0],
    ];
    let pages = [
        Page::Calculator,
        Page::Converter,
        Page::Date,
        Page::Graphing,
        Page::Settings,
    ];
    let kinds = [
        EventKind::Press,
        EventKind::Result,
        EventKind::Error,
        EventKind::Show,
        EventKind::Option,
        EventKind::Mode,
    ];
    let causes = [
        None,
        Some(Cause::Equals),
        Some(Cause::Operator),
        Some(Cause::Function),
        Some(Cause::Digit),
        Some(Cause::Clear),
        Some(Cause::Backspace),
        Some(Cause::Memory),
        Some(Cause::Button),
    ];
    let mut i = 0usize;
    for kind in kinds {
        for page in pages {
            for rect in rects {
                for cause in causes {
                    i += 1;
                    let mut e = Event::at(kind, page, rect);
                    e.cause = cause;
                    e.source = [
                        None,
                        Some(From::Key),
                        Some(From::Button),
                        Some(From::Swap),
                        Some(From::Date),
                    ][i % 5];
                    e.via = [
                        None,
                        Some(Via::Pointer),
                        Some(Via::Keyboard),
                        Some(Via::Assistive),
                    ][i % 4];
                    e.action = [
                        None,
                        Some(Action::Clear),
                        Some(Action::Backspace),
                        Some(Action::Memory),
                    ][i % 4];
                    e.key = match i % 8 {
                        0 => None,
                        1 => Some((None, KeyKind::Number)),
                        2 => Some((Some(0), KeyKind::Number)),
                        3 => Some((Some(u32::MAX), KeyKind::Equals)),
                        4 => Some((Some(7), KeyKind::Operator)),
                        5 => Some((Some(3), KeyKind::Func)),
                        6 => Some((Some(9), KeyKind::Toggle)),
                        _ => Some((Some(11), KeyKind::Other)),
                    };
                    if kind == EventKind::Option {
                        e.option = Some(("palette".into(), text("ember")));
                    }
                    if i.is_multiple_of(3) {
                        e.x = f32::MAX / 4.0;
                        e.y = -1e9;
                    }
                    out.push(e);
                }
            }
        }
    }
    out
}

/// Option sets for the draw sweeps: every choice value with the toggles
/// all on, all off, and the sliders at their ends.
fn draw_sets(theme: &Theme) -> Vec<BTreeMap<String, OptionValue>> {
    let defs = &theme.def.options;
    let mut sets = Vec::new();
    let mut bits = vec![true, false];
    bits.dedup();
    for on in bits {
        for end in 0..2 {
            let mut base = BTreeMap::new();
            for o in defs {
                match &o.kind {
                    OptionKind::Toggle => {
                        base.insert(o.id.clone(), OptionValue::Bool(on));
                    }
                    OptionKind::Slider { min, max, .. } => {
                        base.insert(
                            o.id.clone(),
                            OptionValue::Number(if end == 0 { *min } else { *max }),
                        );
                    }
                    _ => {}
                }
            }
            let choices: Vec<&OptionDef> = defs
                .iter()
                .filter(|o| matches!(o.kind, OptionKind::Choice { .. }))
                .collect();
            if choices.is_empty() {
                sets.push(base.clone());
            }
            for c in &choices {
                for v in values_of(c) {
                    let mut s = base.clone();
                    s.insert(c.id.clone(), v);
                    sets.push(s);
                }
            }
        }
    }
    sets
}

fn check_drawn(
    theme: &Theme,
    name: &str,
    r: Result<gmnb_theme::layer::Drawn, Failure>,
    bad: &mut Vec<String>,
) {
    match r {
        Err(Failure::Failed(e)) if e.to_string().contains("deadline") => {}
        Err(Failure::Failed(e)) => bad.push(format!("{name}: failed: {e}")),
        Err(Failure::Off) => bad.push(format!("{name}: off")),
        Ok(d) => {
            let dump = format!("{:?}", d.list);
            if bad_number(&dump) {
                let at = dump.find("NaN").or(dump.find("inf")).unwrap_or(0);
                bad.push(format!(
                    "{name}: a number is not finite: ...{}",
                    &dump[at.saturating_sub(80)..(at + 40).min(dump.len())]
                ));
            }
            for v in [d.again, d.due, d.retry].into_iter().flatten() {
                if !v.is_finite() || v < 0.0 {
                    bad.push(format!("{name}: a delay of {v}"));
                }
            }
            for p in d.places.iter().flatten() {
                let all = [p.dx, p.dy, p.scale, p.opacity];
                if all.iter().any(|v| !v.is_finite()) {
                    bad.push(format!("{name}: a placement {p:?}"));
                }
            }
        }
    }
    let notes = theme.take_draw_notes();
    if !notes.is_empty() {
        bad.push(format!("{name}: draw notes {notes:?}"));
    }
    let log = theme.take_log();
    if !log.is_empty() {
        bad.push(format!("{name}: log {log:?}"));
    }
}

fn sweep_layers(id: &str) -> Vec<String> {
    let mut theme = built_in(id);
    let mut bad = Vec::new();
    let events = odd_events();
    let sets = draw_sets(&theme);
    let clock = std::rc::Rc::new(std::cell::Cell::new(100.0f64));
    let read = clock.clone();
    theme.api().set_clock(std::rc::Rc::new(move || read.get()));
    let value_texts = ["", "0", "-1.5e+9999", "ERROR", &"9".repeat(400)];
    // What each frame is told, besides the size: time, dt, motion, age of
    // the events, how many of them.
    let timings: [(f64, f64); 6] = [
        (0.0, 0.0),
        (12.5, 0.016),
        (1e9, 10.0),
        (-5.0, -1.0),
        (3.0, 0.0),
        (2.0, 1e-9),
    ];
    for (si, set) in sets.iter().enumerate() {
        for dark in [false, true] {
            for (opacity, motion) in [(1.0, true), (0.1, true), (1.0, false)] {
                let ctx = ctx_of(&theme, dark, opacity, None, motion, set);
                if let Err(e) = theme.style(&ctx) {
                    bad.push(format!("{id}: style: {e}"));
                    continue;
                }
                let _ = theme.take_log();
                for (wi, (w, h)) in SIZES.iter().copied().enumerate() {
                    for (ti, (time, dt)) in timings.iter().copied().enumerate() {
                        clock.set(clock.get() + 1.0);
                        // Few combinations for each: the sweep is wide, not
                        // the full product.
                        let pick = si + wi * 3 + ti;
                        let ev_n = [0usize, 1, 5, 20, 32][pick % 5];
                        let ages = [0.0, 0.2, 4.9, 61.0, 1e9, 0.0];
                        let evs: Vec<(f64, &Event)> = (0..ev_n)
                            .map(|k| {
                                (
                                    ages[(k + pick) % ages.len()],
                                    &events[(k * 37 + pick * 11) % events.len()],
                                )
                            })
                            .collect();
                        for kind in LayerKind::ALL {
                            if theme.layer(kind).is_none() {
                                continue;
                            }
                            if !theme.draws(kind) {
                                bad.push(format!(
                                    "{id} {kind:?}: switched off after earlier frames"
                                ));
                                continue;
                            }
                            let name = format!(
                                "{id} {kind:?} set={set:?} dark={dark} op={opacity} motion={motion} {w}x{h} t={time} dt={dt} events={ev_n}"
                            );
                            let mut frame = Frame::sized(w, h);
                            frame.time = time;
                            frame.dt = dt;
                            frame.motion = motion;
                            frame.settled = pick.is_multiple_of(4);
                            frame.active = pick % 7 != 3;
                            frame.pointer = (pick.is_multiple_of(3)).then_some((w / 2.0, h * 2.0));
                            frame.events = evs.clone();
                            let cells;
                            let glyphs;
                            let prev;
                            match kind {
                                LayerKind::Keypad => {
                                    let (rows, cols, ids) = [
                                        (6, 4, true),
                                        (8, 5, true),
                                        (4, 16, false),
                                        (0, 0, true),
                                        (1, 1, true),
                                        (10, 10, true),
                                    ][pick % 6];
                                    cells = grid(rows, cols, w, h, ids);
                                    frame.keys = Some(Keys {
                                        pad: (pick % 3) as u64,
                                        layout: (pick % 5) as u64,
                                        cells: &cells,
                                    });
                                }
                                LayerKind::Display => {
                                    let t = value_texts[pick % value_texts.len()];
                                    glyphs = glyphs_of(
                                        t.chars().count().min(64),
                                        w,
                                        h,
                                        (h * 0.4).min(75.0),
                                    );
                                    prev = glyphs_of(3, w, h, (h * 0.4).min(75.0));
                                    let change = [
                                        None,
                                        Some((ChangeKind::Typing, 0.0, 1)),
                                        Some((ChangeKind::Result, 0.3, 1)),
                                        Some((ChangeKind::Replace, 61.0, 7)),
                                        Some((ChangeKind::Error, 1e9, 0)),
                                    ][pick % 5];
                                    frame.value = Some(DisplayValue {
                                        readout: (pick % 4) as u64,
                                        version: pick as u64,
                                        text: t,
                                        glyphs: &glyphs,
                                        band: [0.0, 0.0, w, h],
                                        error: t == "ERROR",
                                        align_end: pick.is_multiple_of(2),
                                        role: [Role::Calculator, Role::Converter, Role::Date]
                                            [pick % 3],
                                        change,
                                        previous: change.map(|_| &prev),
                                    });
                                }
                                _ => {}
                            }
                            let r = theme.draw(kind, &ctx, &frame);
                            let failed = r.is_err();
                            check_drawn(&theme, &name, r, &mut bad);
                            if failed {
                                theme = built_in(id);
                                let again = clock.clone();
                                theme.api().set_clock(std::rc::Rc::new(move || again.get()));
                                let _ = theme.style(&ctx);
                                let _ = theme.take_log();
                            }
                        }
                    }
                }
            }
        }
    }
    bad
}

#[test]
fn aurora_layers_sweep() {
    report(sweep_layers("aurora"));
}

#[test]
fn sprinkles_layers_sweep() {
    report(sweep_layers("sprinkles"));
}

#[test]
#[ignore = "a long sweep (a minute or more): cargo test --release -p gmnb-theme --test sweep -- --ignored"]
fn swirls_layers_sweep() {
    report(sweep_layers("swirls"));
}

// ---- Sound -------------------------------------------------------------------------

fn sweep_sound(id: &str) -> Vec<String> {
    let mut theme = built_in(id);
    let mut bad = Vec::new();
    let events = odd_events();
    for set in draw_sets(&theme) {
        for dark in [false, true] {
            let ctx = ctx_of(&theme, dark, 1.0, None, true, &set);
            let _ = theme.style(&ctx);
            for (i, e) in events.iter().enumerate() {
                let pan = [-1.0, 0.0, 1.0, 7.0][i % 4];
                match theme.sound_choice(e, pan, &ctx) {
                    None => {}
                    Some(Err(err)) => bad.push(format!("{id} {set:?} {e:?}: {err}")),
                    Some(Ok(_)) => {}
                }
                let notes = theme.take_sound_notes();
                if !notes.is_empty() {
                    bad.push(format!("{id} {set:?} {e:?}: notes {notes:?}"));
                }
            }
            if !theme.asks_sound() && theme.def.sound.is_some() {
                bad.push(format!("{id} {set:?}: sound switched off"));
            }
        }
    }
    bad
}

#[test]
fn aurora_sound_sweep() {
    report(sweep_sound("aurora"));
}

#[test]
fn sprinkles_sound_sweep() {
    report(sweep_sound("sprinkles"));
}

#[test]
fn swirls_sound_sweep() {
    report(sweep_sound("swirls"));
}

// ---- Options: visible, actions, previews ----------------------------------------------

#[test]
fn options_visible_actions_and_previews() {
    let mut bad = Vec::new();
    for id in THEMES {
        let mut theme = built_in(id);
        for set in option_sets(&theme).into_iter().step_by(7) {
            for dark in [false, true] {
                let ctx = ctx_of(&theme, dark, 1.0, Some([0.2, 0.4, 0.9]), true, &set);
                let _ = theme.style(&ctx);
                let defs = theme.def.options.clone();
                for o in &defs {
                    let _ = theme.visible(o, &ctx);
                    for a in &o.actions {
                        match theme.run_action(a, &ctx) {
                            Ok(changed) => {
                                for (k, v) in &changed {
                                    let def = defs.iter().find(|d| &d.id == k).unwrap();
                                    if def.accept(v).as_ref() != Some(v) {
                                        bad.push(format!("{id} {}: action gave {k}={v:?}", o.id));
                                    }
                                }
                            }
                            Err(e) => bad.push(format!("{id} {} action {}: {e}", o.id, a.label)),
                        }
                    }
                    if let OptionKind::Choice {
                        choices,
                        preview: Some(_),
                    } = &o.kind
                    {
                        for c in choices {
                            match theme.preview(o, &c.id, &ctx) {
                                Ok(list) => {
                                    if bad_number(&format!("{list:?}")) {
                                        bad.push(format!("{id} {} {}: non-finite", o.id, c.id));
                                    }
                                    if list.is_empty() {
                                        bad.push(format!("{id} {} {}: empty tile", o.id, c.id));
                                    }
                                }
                                Err(e) => bad.push(format!("{id} {} {}: {e}", o.id, c.id)),
                            }
                        }
                        if let Ok(list) = theme.preview(o, "no-such-choice", &ctx) {
                            let _ = list;
                        }
                    }
                }
                let notes = theme.take_draw_notes();
                if !notes.is_empty() {
                    bad.push(format!("{id}: draw notes {notes:?}"));
                }
                let log = theme.take_log();
                if !log.is_empty() {
                    bad.push(format!("{id}: log {log:?}"));
                }
            }
        }
    }
    report(bad);
}

// ---- Accents ---------------------------------------------------------------------------

/// Colours all round the wheel, light and dark, saturated and not.
fn wheel() -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    for hue in (0..360).step_by(3) {
        for (s, l) in [(1.0, 0.5), (1.0, 0.3), (1.0, 0.7), (0.5, 0.5), (0.2, 0.4)] {
            out.push(appcore::color::from_hsl(hue as f32, s, l));
        }
    }
    out
}

/// The base look under any desktop accent reads (the same table as every
/// theme's).
#[test]
fn the_base_look_under_any_accent_reads() {
    let mut styles = Vec::new();
    for accent in wheel() {
        for dark in [false, true] {
            for opacity in [1.0, 0.1] {
                let ctx = Ctx {
                    dark,
                    opacity,
                    accent: Some(accent),
                    ..Ctx::default()
                };
                styles.push((
                    format!(
                        "base {} dark={dark} {opacity}",
                        appcore::color::to_hex(accent)
                    ),
                    Style::base(&ctx),
                ));
            }
        }
    }
    let bad = unreadable(styles);
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// A theme that takes its colours from `gmnb.base` reads under any accent.
#[test]
fn a_base_theme_under_any_accent_reads() {
    let mut typewriter = accent_theme();
    let mut styles = Vec::new();
    for accent in wheel() {
        for dark in [false, true] {
            for opacity in [1.0, 0.1] {
                let hex = appcore::color::to_hex(accent);
                let saved = BTreeMap::from([("accent".to_string(), text(&hex))]);
                let ctx = ctx_of(&typewriter, dark, opacity, None, true, &saved);
                styles.push((
                    format!("typewriter {hex} dark={dark} {opacity}"),
                    typewriter.style(&ctx).unwrap().style.clone(),
                ));
            }
        }
    }
    let bad = unreadable(styles);
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// Aurora's System palette under any desktop accent, and Freestyle from
/// colours all round the wheel (the complement, and another colour).
#[test]
fn aurora_under_any_accent_reads() {
    let mut aurora = built_in("aurora");
    let mut bad = Vec::new();
    let wheel = wheel();
    for (i, accent) in wheel.iter().enumerate() {
        for dark in [false, true] {
            // (Half clear too: what a see-through Aurora shows, and so the
            // frost under its text, changes all the way down.)
            for opacity in [1.0, 0.5, 0.1] {
                let mut styles = Vec::new();
                let hex = appcore::color::to_hex(*accent);
                let ctx = ctx_of(
                    &aurora,
                    dark,
                    opacity,
                    Some(*accent),
                    true,
                    &BTreeMap::from([("palette".to_string(), text("system"))]),
                );
                let name = format!("aurora system {hex} dark={dark} {opacity}");
                let style = aurora.style(&ctx).unwrap().style.clone();
                // (Over its own aurora too, as everywhere here.)
                bad.extend(unreadable_over_the_aurora(&aurora, &ctx, &name, &style));
                styles.push((name, style));
                let other = wheel[(i * 7 + 11) % wheel.len()];
                for second in [appcore::color::complement(*accent), other] {
                    let saved = BTreeMap::from([
                        ("palette".to_string(), text("custom")),
                        ("primary".to_string(), text(&hex)),
                        (
                            "secondary".to_string(),
                            text(&appcore::color::to_hex(second)),
                        ),
                    ]);
                    let ctx = ctx_of(&aurora, dark, opacity, None, true, &saved);
                    let name = format!(
                        "aurora freestyle {hex}+{} dark={dark} {opacity}",
                        appcore::color::to_hex(second)
                    );
                    let style = aurora.style(&ctx).unwrap().style.clone();
                    bad.extend(unreadable_over_the_aurora(&aurora, &ctx, &name, &style));
                    styles.push((name, style));
                }
                bad.extend(unreadable(styles));
            }
        }
    }
    report(bad);
}

/// Sprinkles' System flavour under any desktop accent, and Freestyle iced
/// in colours all round the wheel (with its own second colour, and with
/// another from the wheel), light and dark, opaque and see-through: rules
/// 3 and 4.
#[test]
fn sprinkles_under_any_accent_reads() {
    let mut sprinkles = built_in("sprinkles");
    let mut bad = Vec::new();
    let wheel = wheel();
    for (i, accent) in wheel.iter().enumerate() {
        for dark in [false, true] {
            for opacity in [1.0, 0.1] {
                let mut styles = Vec::new();
                let hex = appcore::color::to_hex(*accent);
                let system = BTreeMap::from([("flavour".to_string(), text("system"))]);
                let ctx = ctx_of(&sprinkles, dark, opacity, Some(*accent), true, &system);
                styles.push((
                    format!("sprinkles system {hex} dark={dark} {opacity}"),
                    sprinkles.style(&ctx).unwrap().style.clone(),
                ));
                let other = appcore::color::to_hex(wheel[(i * 7 + 11) % wheel.len()]);
                for second in [None, Some(other)] {
                    let mut saved = BTreeMap::from([
                        ("flavour".to_string(), text("custom")),
                        ("icing".to_string(), text(&hex)),
                    ]);
                    if let Some(second) = &second {
                        saved.insert("second".to_string(), text(second));
                    }
                    let ctx = ctx_of(&sprinkles, dark, opacity, None, true, &saved);
                    styles.push((
                        format!("sprinkles freestyle {hex}+{second:?} dark={dark} {opacity}"),
                        sprinkles.style(&ctx).unwrap().style.clone(),
                    ));
                }
                bad.extend(unreadable(styles));
            }
        }
    }
    report(bad);
}

/// The longest value the calculator shows: 64 binary digits in groups of
/// four (79 glyphs), and a few more. Every change a readout can be going
/// through, at a readout's usual sizes.
#[test]
fn displays_draw_the_longest_values() {
    let mut bad = Vec::new();
    for id in ["aurora", "sprinkles", "swirls"] {
        for n in [32usize, 79, 100, 128] {
            for dark in [false, true] {
                let theme = built_in(id);
                let ctx = ctx_of(&theme, dark, 1.0, None, true, &BTreeMap::new());
                let glyphs = glyphs_of(n, 400.0, 100.0, 24.0);
                let prev = glyphs_of(n - 1, 400.0, 100.0, 24.0);
                let t = "1".repeat(n);
                for change in [
                    None,
                    Some((ChangeKind::Typing, 0.05, 1)),
                    Some((ChangeKind::Result, 0.3, 1)),
                    Some((ChangeKind::Replace, 0.2, 3)),
                    Some((ChangeKind::Error, 0.2, 1)),
                ] {
                    for error in [false, true] {
                        let mut frame = Frame::sized(400.0, 100.0);
                        frame.time = 10.0;
                        frame.value = Some(DisplayValue {
                            readout: 1,
                            version: 1,
                            text: &t,
                            glyphs: &glyphs,
                            band: [0.0, 0.0, 400.0, 100.0],
                            error,
                            align_end: true,
                            role: Role::Calculator,
                            change,
                            previous: change.map(|_| &prev),
                        });
                        let r = theme.draw(LayerKind::Display, &ctx, &frame);
                        let name = format!("{id} {n} glyphs dark={dark} {change:?} error={error}");
                        check_drawn(&theme, &name, r, &mut bad);
                        if !theme.draws(LayerKind::Display) {
                            break;
                        }
                    }
                }
            }
        }
    }
    report(bad);
}
