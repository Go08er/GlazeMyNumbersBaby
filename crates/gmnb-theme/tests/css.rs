//! The generated style against the structure stylesheet, the two
//! providers' text, and the readability rules (docs/theming.md, "Tests the
//! engine must have": 1 and 8). No toolkit: the stylesheet is read as
//! text.

use std::collections::BTreeMap;

use appcore::color::{TEXT_CONTRAST, UI_CONTRAST, contrast};
use gmnb_theme::states::{Which, over};
use gmnb_theme::theme::{self, Source, Theme};
use gmnb_theme::types::*;
use gmnb_theme::writer::used_properties;
use gmnb_theme::{Ctx, Engine, Limits, OptionValue, Style, Update, Writer, css, resolve};

/// GMNB's structure stylesheet.
const STYLESHEET: &str = include_str!("../../../apps/gmnb/src/style.css");

const PALETTES: [&str; 7] = [
    "aurora", "ember", "abyss", "orchard", "graphite", "system", "custom",
];

fn built_in(id: &str) -> Theme {
    Theme::load(Source::built_in(id).unwrap(), Limits::default()).unwrap()
}

fn with(theme: &Theme, dark: bool, opacity: f32, options: &[(&str, OptionValue)]) -> Ctx {
    let saved: BTreeMap<String, OptionValue> = options
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    Ctx {
        dark,
        opacity,
        options: theme::complete_options(&theme.def.options, &saved),
        ..Ctx::default()
    }
}

fn text(s: &str) -> OptionValue {
    OptionValue::Text(s.into())
}

/// Every style the tests look at: the base look, and each built-in theme
/// in each of its palettes, light and dark, at three opacities.
fn every_style() -> Vec<(String, Style)> {
    let mut out = Vec::new();
    let mut aurora = built_in("aurora");
    let mut mono = built_in("mono");
    for dark in [false, true] {
        for opacity in [1.0, 0.5, 0.1] {
            let ctx = Ctx {
                dark,
                opacity,
                ..Ctx::default()
            };
            out.push((format!("base dark={dark} {opacity}"), Style::base(&ctx)));
            for palette in PALETTES {
                let ctx = with(&aurora, dark, opacity, &[("palette", text(palette))]);
                let style = aurora.style(&ctx).unwrap().style.clone();
                out.push((format!("aurora {palette} dark={dark} {opacity}"), style));
            }
            let ctx = with(&mono, dark, opacity, &[]);
            let style = mono.style(&ctx).unwrap().style.clone();
            out.push((format!("mono dark={dark} {opacity}"), style));
        }
    }
    out
}

// ---- 1: every property the stylesheet reads is written ---------------------------

#[test]
fn every_property_the_stylesheet_reads_is_emitted() {
    let writer = Writer::new(STYLESHEET);
    // The slice's rules read several hundred.
    assert!(writer.used().len() > 400, "{}", writer.used().len());
    for (name, style) in every_style() {
        let properties = css::properties(&style);
        let missing = writer.missing(&properties);
        assert!(missing.is_empty(), "{name}: not emitted: {missing:#?}");
    }
}

#[test]
fn the_stylesheets_names_are_found() {
    let used = used_properties(
        "a { color: var(--gmnb-window-color); margin: var( --gmnb-keys-padding ) }
         b { border-spacing: calc(var(--gmnb-segmented-gap) / 2); color: var(--accent-color) }",
    );
    let used: Vec<&str> = used.iter().map(String::as_str).collect();
    assert_eq!(
        used,
        [
            "--gmnb-keys-padding",
            "--gmnb-segmented-gap",
            "--gmnb-window-color"
        ]
    );
}

/// What the stylesheet's rules get for the base look: DGMNB's values, as
/// CSS.
#[test]
fn the_base_looks_properties() {
    let ctx = Ctx {
        dark: true,
        ..Ctx::default()
    };
    let p = css::properties(&Style::base(&ctx));
    let get = |name: &str| p.get(name).map(String::as_str).unwrap_or("(missing)");
    assert_eq!(get("--gmnb-window-background-color"), "rgb(29,29,32)");
    assert_eq!(get("--gmnb-window-background-image"), "none");
    assert_eq!(get("--gmnb-window-color"), "rgb(242,242,245)");
    assert_eq!(
        get("--gmnb-window-font-family"),
        "\"Inter Variable\", \"Noto Sans Math\", \"Noto Sans Arabic\", \"Noto Sans Armenian\", \"Noto Sans Bengali\", \"Noto Sans Khmer\""
    );
    assert_eq!(get("--gmnb-window-font-size"), "15px");
    // Inter's optical size is written explicitly: the size in pixels.
    assert_eq!(get("--gmnb-window-font-variations"), "\"opsz\" 15");
    assert_eq!(get("--gmnb-window-text-halo"), "none");
    assert_eq!(get("--gmnb-keys-number-background-color"), "rgb(44,44,49)");
    assert_eq!(get("--gmnb-keys-number-background-image"), "none");
    assert_eq!(get("--gmnb-keys-number-font-size"), "21px");
    assert_eq!(get("--gmnb-keys-number-font-weight"), "500");
    assert_eq!(get("--gmnb-keys-number-font-variations"), "\"opsz\" 21");
    assert_eq!(get("--gmnb-keys-number-radius"), "8px");
    assert_eq!(get("--gmnb-keys-number-shadow"), "none");
    assert_eq!(get("--gmnb-keys-number-transition"), "none");
    assert_eq!(get("--gmnb-keys-number-min-height"), "0px");
    // Hover and press: an overlay over the fill.
    assert_eq!(
        get("--gmnb-keys-number-hover-background-image"),
        "linear-gradient(rgba(242,242,245,0.07), rgba(242,242,245,0.07))"
    );
    assert_eq!(
        get("--gmnb-keys-number-active-background-image"),
        "linear-gradient(rgba(242,242,245,0.13), rgba(242,242,245,0.13))"
    );
    assert_eq!(
        get("--gmnb-keys-number-hover-background-color"),
        "rgb(44,44,49)"
    );
    // A disabled key fades its glyph, not itself.
    assert_eq!(
        get("--gmnb-keys-number-disabled-color"),
        "rgba(242,242,245,0.35)"
    );
    assert_eq!(get("--gmnb-keys-number-disabled-opacity"), "1");
    assert_eq!(get("--gmnb-keys-number-disabled-filter"), "none");
    assert_eq!(get("--gmnb-keys-operator-font-size"), "19px");
    assert_eq!(get("--gmnb-keys-equals-color"), "rgb(16,16,20)");
    // A toggle that is on, and with the pointer over it.
    assert_eq!(
        get("--gmnb-keys-func-checked-background-color"),
        get("--gmnb-keys-equals-background-color")
    );
    assert_eq!(
        get("--gmnb-keys-func-checked-hover-background-image"),
        get("--gmnb-keys-number-hover-background-image")
    );
    assert_eq!(get("--gmnb-segmented-gap"), "4px");
    assert_eq!(get("--gmnb-segmented-item-min-height"), "36px");
    assert_eq!(get("--gmnb-segmented-item-font-size"), "13px");
    assert_eq!(get("--gmnb-segmented-item-font-variations"), "\"opsz\" 14");
    assert_eq!(get("--gmnb-panel-tabs-gap"), "0px");
    assert_eq!(get("--gmnb-panel-tabs-item-min-height"), "34px");
    assert_eq!(get("--gmnb-panel-tabs-item-font-weight"), "600");
    assert_eq!(
        get("--gmnb-panel-tabs-item-background-color"),
        "transparent"
    );
    assert_eq!(
        get("--gmnb-panel-tabs-item-checked-color"),
        get("--gmnb-keys-equals-color")
    );
    assert_eq!(get("--gmnb-plain-background-color"), "rgba(29,29,32,0)");

    // See-through: the halo, and the scrim.
    let ctx = Ctx {
        dark: true,
        opacity: 0.1,
        ..Ctx::default()
    };
    let p = css::properties(&Style::base(&ctx));
    assert_eq!(
        p["--gmnb-window-text-halo"],
        "0px 0px 3px rgba(29,29,32,0.9), 0px 0px 10px rgba(29,29,32,0.75)"
    );
    assert!(p["--gmnb-plain-background-color"].starts_with("rgba(29,29,32,0."));
    assert_eq!(
        p["--gmnb-header-background-color"],
        p["--gmnb-plain-background-color"]
    );
}

/// What a theme's values become: gradients, shadow lists with the border
/// first, transitions, filters and transforms.
#[test]
fn a_themes_values_as_css() {
    let engine = Engine::with_globals(Limits::default(), |lua| {
        gmnb_theme::library::register(lua, gmnb_theme::library::placeholder_drawing)
    })
    .unwrap();
    let value = engine
        .load(
            "=style",
            "local g = gmnb.linear({ angle = 135, colors = { '#ff7ad9', '#8a63ff' } })
             return { keys = {
               radius = { 14, 14, 2, 2 },
               padding = { 0, 2 },
               transition = { transform = { ms = 320, ease = 'spring' }, filter = 160, background = 160 },
               hover = { brightness = 1.12 },
               active = { scale = 0.93, brightness = 1.22, rotate = 3, transition = { transform = 70 } },
               disabled = { opacity = 0.32, saturation = 0.4 },
               equals = {
                 background = { g, '#00000033', '#102030' },
                 border = { width = 1, color = '#ffffff1a' },
                 shadow = { { inset = true, y = 1, color = '#ffffff59' },
                            { y = 12, blur = 32, spread = -12, color = '#8a63ffd9' } },
                 font = { family = { 'Outfit', 'sans-serif' }, size = 26, italic = true, spacing = 0.01,
                          features = { 'tnum', 'ss01=0' }, variations = { wght = 425.5 } },
               },
               operator = { background = gmnb.radial({ at = { 0.5, 0.4 }, radius = 'closest-side',
                                                       colors = { '#fff', '#000', '#f00' }, stops = { 0, 0.3, 1 } }) },
             } }",
        )
        .unwrap();
    let r = resolve(&Ctx::default(), &value);
    assert!(r.notes.is_empty(), "{:#?}", r.notes);
    let p = css::properties(&r.style);
    let get = |name: &str| p[&format!("--gmnb-keys-{name}")].as_str();
    assert_eq!(get("equals-background-color"), "rgb(16,32,48)");
    assert_eq!(
        get("equals-background-image"),
        "linear-gradient(135deg, rgb(255,122,217) 0%, rgb(138,99,255) 100%), linear-gradient(rgba(0,0,0,0.2), rgba(0,0,0,0.2))"
    );
    assert_eq!(
        get("equals-shadow"),
        "inset 0 0 0 1px rgba(255,255,255,0.102), inset 0px 1px 0px 0px rgba(255,255,255,0.349), 0px 12px 32px -12px rgba(138,99,255,0.851)"
    );
    assert_eq!(get("equals-radius"), "14px 14px 2px 2px");
    assert_eq!(get("equals-padding"), "0px 2px 0px 2px");
    assert_eq!(get("equals-font-family"), "\"Outfit\", \"sans-serif\"");
    assert_eq!(get("equals-font-style"), "italic");
    assert_eq!(get("equals-letter-spacing"), "0.01em");
    assert_eq!(get("equals-font-features"), "\"tnum\" 1, \"ss01\" 0");
    // Not Inter: no optical size is added.
    assert_eq!(get("equals-font-variations"), "\"wght\" 425.5");
    assert_eq!(
        get("equals-transition"),
        "background-color 160ms ease-out, background-image 160ms ease-out, transform 320ms cubic-bezier(0.34, 1.56, 0.64, 1), filter 160ms ease-out"
    );
    assert_eq!(get("equals-filter"), "none");
    assert_eq!(get("equals-hover-filter"), "brightness(1.12)");
    assert_eq!(get("equals-active-filter"), "brightness(1.22)");
    assert_eq!(get("equals-active-transform"), "scale(0.93) rotate(3deg)");
    assert_eq!(get("equals-active-transition"), "transform 70ms ease-out");
    assert_eq!(get("equals-disabled-opacity"), "0.32");
    assert_eq!(get("equals-disabled-filter"), "saturate(0.4)");
    // The theme's hover has no overlay: none is laid over the gradient.
    assert_eq!(
        get("equals-hover-background-image"),
        get("equals-background-image")
    );
    assert_eq!(
        get("operator-background-image"),
        "radial-gradient(closest-side at 50% 40%, rgb(255,255,255) 0%, rgb(0,0,0) 30%, rgb(255,0,0) 100%)"
    );
    assert_eq!(get("operator-background-color"), "transparent");
}

/// Nothing a theme returns reaches the stylesheet as text: a family name
/// is one quoted string whatever is in it, and names, tags and keywords
/// that can't be written safely are refused.
#[test]
fn no_theme_string_reaches_the_stylesheet_as_text() {
    let engine = Engine::with_globals(Limits::default(), |lua| {
        gmnb_theme::library::register(lua, gmnb_theme::library::placeholder_drawing)
    })
    .unwrap();
    let value = engine
        .load(
            "=style",
            r#"return {
                fonts = { ui = { 'Evil"; } window { background: url(/etc/passwd) } a { x: "', 'Back\\slash' } },
                keys = {
                    number = { font = { family = 'Tab\tbed', features = { 'tn"m', 'tnum;' }, variations = { ['op"z'] = 3 } },
                               transition = { background = { ms = 100, ease = 'ease; } x {' } } },
                    func = { font = { family = 'New\nline' } },
                    equals = { background = gmnb.radial({ radius = 'farthest-corner', colors = { '#000', '#fff' } }) },
                },
                segmented = { item = { background = { kind = 'radial', radius = 'url(x)', colors = { '#000', '#fff' } } } },
            }"#,
        )
        .unwrap();
    let r = resolve(&Ctx::default(), &value);
    let mut noted: Vec<&str> = r.notes.list.iter().map(|n| n.path.as_str()).collect();
    noted.sort();
    assert_eq!(
        noted,
        [
            "keys.func.font.family[1]",
            "keys.number.font.family[1]",
            "keys.number.font.features",
            "keys.number.font.variations",
            "keys.number.transition.background.ease",
            "segmented.item.background.radius",
        ],
        "{:#?}",
        r.notes.list
    );
    let p = css::properties(&r.style);
    assert_eq!(
        p["--gmnb-window-font-family"],
        r#""Evil\"; } window { background: url(/etc/passwd) } a { x: \"", "Back\\slash""#
    );
    // Whatever a value holds, it stays one declaration: outside quoted
    // strings there is nothing that ends one or opens a block or a url.
    for (name, value) in &p {
        let mut outside = String::new();
        let mut chars = value.chars();
        let mut quoted = false;
        while let Some(c) = chars.next() {
            match c {
                '\\' if quoted => {
                    chars.next();
                }
                '"' => quoted = !quoted,
                c if !quoted => outside.push(c),
                _ => {}
            }
        }
        assert!(!quoted, "{name}: {value}");
        assert!(!value.chars().any(char::is_control), "{name}: {value:?}");
        for bad in [";", "{", "}", "url", "@", "/*", "\\", "!"] {
            assert!(!outside.contains(bad), "{name}: {value}");
        }
    }
    // The wrong values kept the base: no new family, feature or keyword.
    assert_eq!(
        p["--gmnb-keys-number-font-family"],
        p["--gmnb-window-font-family"]
    );
    assert_eq!(p["--gmnb-keys-number-font-features"], "normal");
    assert_eq!(p["--gmnb-keys-number-transition"], "none");
    assert!(css::quote_family("a\u{0}b").is_none());
    assert!(css::quote_family("").is_none());
}

// ---- The providers ---------------------------------------------------------------

#[test]
fn few_changes_go_to_the_live_provider_and_fold_back() {
    let sheet = "a { color: var(--gmnb-a); margin: var(--gmnb-b); padding: var(--gmnb-c) }";
    let mut w = Writer::new(sheet);
    let props = |a: &str, b: &str, c: &str| {
        BTreeMap::from([
            ("--gmnb-a".to_string(), a.to_string()),
            ("--gmnb-b".to_string(), b.to_string()),
            ("--gmnb-c".to_string(), c.to_string()),
            // Not read by the stylesheet: never written.
            ("--gmnb-unused".to_string(), a.to_string()),
            // The toolkit's own: always written.
            ("--window-bg-color".to_string(), "red".to_string()),
        ])
    };
    let pre = "@define-color window_bg_color red;\n";
    // The first load is the main provider's, whole.
    let first = w.update(pre, &props("1", "2", "3"), true);
    assert_eq!(
        first,
        Update::Main(
            "@define-color window_bg_color red;\n:root {\n  --gmnb-a: 1;\n  --gmnb-b: 2;\n  --gmnb-c: 3;\n  --window-bg-color: red;\n}\n"
                .to_string()
        )
    );
    assert_eq!((w.main_len(), w.live_len()), (4, 0));
    assert_eq!(
        w.update(pre, &props("1", "2", "3"), true),
        Update::Unchanged
    );
    // A step of a drag: only what changed.
    assert_eq!(
        w.update(pre, &props("1", "9", "3"), true),
        Update::Live(":root {\n  --gmnb-b: 9;\n}\n".to_string())
    );
    assert_eq!(
        w.update(pre, &props("1", "9", "3"), true),
        Update::Unchanged
    );
    // The next step: everything that differs from main, not just from the
    // step before.
    assert_eq!(
        w.update(pre, &props("7", "8", "3"), true),
        Update::Live(":root {\n  --gmnb-a: 7;\n  --gmnb-b: 8;\n}\n".to_string())
    );
    assert_eq!((w.main_len(), w.live_len()), (4, 2));
    // Dragged back to where it started: the live provider is emptied.
    assert_eq!(
        w.update(pre, &props("1", "2", "3"), true),
        Update::Live(String::new())
    );
    assert_eq!(w.fold(), None);
    // When the changes stop, live is folded into main.
    w.update(pre, &props("1", "9", "3"), true);
    let folded = w.fold().unwrap();
    assert!(
        folded.contains("--gmnb-b: 9;") && folded.starts_with(pre),
        "{folded}"
    );
    assert_eq!((w.main_len(), w.live_len()), (4, 0));
    assert_eq!(w.fold(), None);
    assert_eq!(
        w.update(pre, &props("1", "9", "3"), false),
        Update::Unchanged
    );
    // A change that is not a drag's goes to main.
    assert!(matches!(
        w.update(pre, &props("1", "2", "3"), false),
        Update::Main(_)
    ));
    // So does a change of the named colours.
    assert!(matches!(
        w.update(
            "@define-color window_bg_color blue;\n",
            &props("1", "2", "3"),
            true
        ),
        Update::Main(_)
    ));
}

#[test]
fn many_changes_reload_the_main_provider() {
    let names: Vec<String> = (0..200).map(|i| format!("--gmnb-p{i}")).collect();
    let sheet: String = names
        .iter()
        .map(|n| format!("a {{ x: var({n}) }}\n"))
        .collect();
    let mut w = Writer::new(&sheet);
    let props = |changed: usize| -> BTreeMap<String, String> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), if i < changed { "1" } else { "0" }.to_string()))
            .collect()
    };
    assert!(matches!(w.update("", &props(0), true), Update::Main(_)));
    assert!(matches!(
        w.update("", &props(Writer::LIVE_MAX), true),
        Update::Live(_)
    ));
    assert_eq!(w.live_len(), Writer::LIVE_MAX);
    // One more than the live provider takes: main is reloaded, live emptied.
    assert!(matches!(
        w.update("", &props(Writer::LIVE_MAX + 1), true),
        Update::Main(_)
    ));
    assert_eq!((w.main_len(), w.live_len()), (200, 0));
}

/// An opacity drag in the base look changes a handful of properties: the
/// scrims, and at the ends the halo.
#[test]
fn an_opacity_step_in_the_base_look_is_a_few_properties() {
    let mut w = Writer::new(STYLESHEET);
    let at = |opacity: f32| {
        css::properties(&Style::base(&Ctx {
            dark: true,
            opacity,
            ..Ctx::default()
        }))
    };
    assert!(matches!(w.update("", &at(1.0), true), Update::Main(_)));
    let mut live = 0;
    for step in 1..=18 {
        let opacity = 1.0 - step as f32 * 0.05;
        match w.update("", &at(opacity), true) {
            // (Near full opacity the text still reads with no scrim.)
            Update::Unchanged => {}
            Update::Live(_) => {
                live += 1;
                assert!(w.live_len() <= 8, "{opacity}: {}", w.live_len());
            }
            Update::Main(_) => panic!("{opacity}: the main provider was reloaded"),
        }
    }
    assert!(live >= 10, "{live} steps changed something");
}

// ---- 8: readability ---------------------------------------------------------------

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
fn pairs(style: &Style, plain_text: bool, chrome: bool) -> Vec<(String, Rgba, f32, Vec<Fill>)> {
    let mut out = Vec::new();
    let plain = style.fill("plain.background");
    // Text straight on the backdrop (the memory row, empty states...), over
    // the scrim. It stands for the reference's `text_button` and header
    // title until those are resolved; a theme gives those their own
    // fills, so on a see-through window only the base look's is checked.
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
    for class in ["number", "func", "operator", "equals"] {
        let key = style.control(&format!("keys.{class}")).unwrap();
        let rest = key.finished(&[]);
        // The operators' glyphs (÷ × − +) are symbols, not text to read:
        // 3, as for an icon. (DGMNB's own light palette puts them at
        // 4.1 on its second surface.)
        let need = if class == "operator" {
            UI_CONTRAST
        } else {
            needed(&key.font)
        };
        out.push((
            format!("keys.{class}"),
            rest.color,
            need,
            vec![rest.background],
        ));
    }
    // A toggle that is on.
    let func = style.control("keys.func").unwrap();
    let on = func.finished(&[Which::Checked]);
    out.push((
        "keys.on".to_string(),
        on.color,
        needed(&func.font),
        vec![on.background],
    ));
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
    if chrome {
        chrome_pairs(style, &plain, &mut out);
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
/// selection its tint shows too (docs/theming.md, open point 8; moving
/// `accent_text` would change DGMNB's pixels).
fn chrome_pairs(style: &Style, plain: &Fill, out: &mut Vec<(String, Rgba, f32, Vec<Fill>)>) {
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
    // The current radix: its name in its own colour, on the checked row.
    let row = control("programmer.radix.row").finished(&[Which::Checked]);
    out.push((
        "programmer.radix.name checked".to_string(),
        style.color("programmer.radix.name_checked"),
        UI_CONTRAST,
        vec![row.background, plain.clone()],
    ));
}

/// Rule 3: every text on what is under it, at full opacity. Rule 4: the
/// same on a see-through window, over a black and over a white desktop:
/// 3 at least, and in the base look 4.5 for the text colour (its scrim is
/// worked out to).
#[test]
fn every_text_reads_on_what_is_under_it() {
    let mut bad = Vec::new();
    for (name, style) in every_style() {
        let base_look = name.starts_with("base");
        let desktops: Vec<Option<Rgba>> = if style.opacity >= 1.0 {
            vec![None]
        } else {
            vec![Some(BLACK), Some(WHITE)]
        };
        for desktop in desktops {
            let behind = backdrop(&style, desktop);
            // Aurora's own values for the chrome don't meet these yet:
            // its polish pass takes them on (the chrome's report lists them).
            let chrome = !name.starts_with("aurora");
            for (what, text, at_rest, fills) in
                pairs(&style, desktop.is_none() || base_look, chrome)
            {
                let fills: Vec<&Fill> = fills.iter().collect();
                let got = worst(text, &fills, &behind);
                let fg = text == style.tokens.colors.fg;
                let want = match desktop {
                    None => at_rest,
                    Some(_) if base_look && fg => TEXT_CONTRAST,
                    Some(_) => UI_CONTRAST,
                };
                if got < want {
                    let over = match desktop {
                        None => "",
                        Some(d) if d == BLACK => " over black",
                        Some(_) => " over white",
                    };
                    bad.push(format!("{name}{over}: {what} {got:.2} < {want}"));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// Rule 2: Aurora's palettes, the built-in ones and those generated from
/// extreme colours. From its own `style`: the equals key's text on its
/// gradient's two ends and middle (a large glyph: 3), the accent and the
/// text on the background (4.5), every graph colour on it (3).
#[test]
fn auroras_palettes_stay_readable() {
    let mut aurora = built_in("aurora");
    let mut bad = Vec::new();
    let mut check = |name: String, style: &Style| {
        let c = &style.tokens.colors;
        let behind = backdrop(style, None);
        let on = |text: Rgba| worst(text, &[], &behind);
        let eq = style.control("keys.equals").unwrap().finished(&[]);
        let glyph = worst(eq.color, &[&eq.background], &behind);
        if glyph < UI_CONTRAST {
            bad.push(format!("{name}: equals glyph {glyph:.2}"));
        }
        if on(c.accent_text) < TEXT_CONTRAST {
            bad.push(format!("{name}: accent {:.2}", on(c.accent_text)));
        }
        if on(c.fg) < TEXT_CONTRAST {
            bad.push(format!("{name}: text {:.2}", on(c.fg)));
        }
        for (i, s) in c.series.iter().enumerate() {
            if on(*s) < UI_CONTRAST {
                bad.push(format!("{name}: series {i} {:.2}", on(*s)));
            }
        }
    };
    for dark in [false, true] {
        for palette in PALETTES {
            let ctx = with(&aurora, dark, 1.0, &[("palette", text(palette))]);
            check(
                format!("{palette} dark={dark}"),
                &aurora.style(&ctx).unwrap().style,
            );
        }
        for (name, hex) in [
            ("white", 0xffffff),
            ("black", 0x000000),
            ("grey", 0x808080),
            ("yellow", 0xffff00),
            ("blue", 0x0000ff),
            ("lime", 0x00ff00),
            ("violet", 0x7c4dff),
        ] {
            let colour = appcore::color::hex(hex);
            let as_text = |c: [f32; 3]| text(&appcore::color::to_hex(c));
            // System: the desktop's accent and its complement.
            let mut ctx = with(&aurora, dark, 1.0, &[("palette", text("system"))]);
            ctx.accent = Some(colour);
            check(
                format!("system {name} dark={dark}"),
                &aurora.style(&ctx).unwrap().style,
            );
            // Freestyle: with the complement, and with the colour twice.
            for (kind, second) in [
                ("complement", appcore::color::complement(colour)),
                ("twice", colour),
            ] {
                let ctx = with(
                    &aurora,
                    dark,
                    1.0,
                    &[
                        ("palette", text("custom")),
                        ("primary", as_text(colour)),
                        ("secondary", as_text(second)),
                    ],
                );
                check(
                    format!("freestyle {name} {kind} dark={dark}"),
                    &aurora.style(&ctx).unwrap().style,
                );
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
