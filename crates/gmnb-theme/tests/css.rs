//! The generated style against the structure stylesheet, the two
//! providers' text, and the readability rules (docs/theming.md, "Tests the
//! engine must have": 1 and 8). No toolkit: the stylesheet is read as
//! text.

mod common;

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

/// Sprinkles' flavours: System (here on a desktop that shares no accent) and
/// Freestyle (in its own colours) too.
const FLAVOURS: [&str; 7] = [
    "strawberry",
    "blueberry",
    "matcha",
    "lemon",
    "caramel",
    "system",
    "custom",
];

fn built_in(id: &str) -> Theme {
    Theme::load(Source::built_in(id).unwrap(), common::limits()).unwrap()
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
    let mut sprinkles = built_in("sprinkles");
    for dark in [false, true] {
        for opacity in [1.0, 0.5, 0.1] {
            let ctx = Ctx {
                dark,
                opacity,
                ..Ctx::default()
            };
            out.push((format!("base dark={dark} {opacity}"), Style::base(&ctx)));
            for flavour in FLAVOURS {
                let ctx = with(&sprinkles, dark, opacity, &[("flavour", text(flavour))]);
                let style = sprinkles.style(&ctx).unwrap().style.clone();
                out.push((format!("sprinkles {flavour} dark={dark} {opacity}"), style));
            }
            for palette in PALETTES {
                let ctx = with(&aurora, dark, opacity, &[("palette", text(palette))]);
                let style = aurora.style(&ctx).unwrap().style.clone();
                out.push((format!("aurora {palette} dark={dark} {opacity}"), style));
            }
        }
    }
    // Swirls in each set of bands under each glass (one look in light and
    // in dark: `swirls_is_dark_in_both_modes`).
    out.extend(swirls_styles());
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
        gmnb_theme::library::register(lua, gmnb_theme::library::without_drawing)
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
        gmnb_theme::library::register(lua, gmnb_theme::library::without_drawing)
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

/// An opacity drag in the base look changes a few properties: the scrims,
/// and at the ends the halo (the window's and its texts').
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
                // At the first step the halo appears, on the window and on
                // each text on the backdrop that the stylesheet names.
                assert!(w.live_len() <= 24, "{opacity}: {}", w.live_len());
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

/// Rule 3: every text on what is under it, at full opacity. Rule 4: the
/// same on a see-through window, over a black and over a white desktop:
/// 3 at least, and in the base look 4.5 for the text colour (its scrim is
/// worked out to).
#[test]
fn every_text_reads_on_what_is_under_it() {
    let bad = unreadable(styles_where(|name| {
        !name.starts_with("aurora") && !name.starts_with("sprinkles") && !name.starts_with("swirls")
    }));
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// Rules 3 and 4 for Sprinkles, in each of its five flavours: the keys, the
/// chrome and the pages. Its other options (the doughnut, the falling
/// sprinkles, the fountain) change no colour. Everything of it that
/// carries text is a solid card, so on a see-through window its page
/// texts count as they do at full opacity. What its layers paint (the
/// awning under the header, the icing on the digits) is checked where
/// the layers are run: `tests/sprinkles.rs`, `what_glazed_paints_reads`.
#[test]
fn sprinkles_page_texts_read() {
    let styles = styles_where(|name| name.starts_with("sprinkles"));
    assert_eq!(styles.len(), FLAVOURS.len() * 2 * 3);
    let bad = unreadable(styles);
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// Rules 3 and 4 for Aurora, in each of its seven palettes: the keys, the
/// chrome and the pages (0.2's own values for many of these were never
/// checked, and missed; the theme raised them).
#[test]
fn auroras_page_texts_read() {
    let bad = unreadable(styles_where(|name| name.starts_with("aurora")));
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// Rules 3 and 4 for the bits' indexes (63, 59, ... under each nibble).
/// DGMNB draws them in `fg_faint` (2.46:1 light, 3.52:1 dark on its
/// background), and the base look is DGMNB's: like the placeholder they
/// need 2.45 (open point 9). On a see-through window the base look
/// deepens them toward `fg_dim` as far as that needs.
#[test]
fn the_bits_indexes_read() {
    let mut bad = Vec::new();
    for (name, style) in every_style() {
        let Some(Value::Text(t)) = style.get("programmer.bit_index") else {
            panic!("no programmer.bit_index");
        };
        let plain = style.fill("plain.background");
        let desktops: Vec<Option<Rgba>> = if style.opacity >= 1.0 {
            vec![None]
        } else {
            vec![Some(BLACK), Some(WHITE)]
        };
        for desktop in desktops {
            let got = worst(t.color, &[&plain], &backdrop(&style, desktop));
            let want = PLACEHOLDER_CONTRAST;
            if got < want {
                bad.push(format!("{name} {desktop:?}: {got:.2} < {want}"));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Those of `every_style` whose name `wanted` says yes to.
fn styles_where(wanted: impl Fn(&str) -> bool) -> Vec<(String, Style)> {
    let mut styles = every_style();
    styles.retain(|(name, _)| wanted(name));
    styles
}

/// Rules 3 and 4 for Aurora's generated palettes: System and Freestyle
/// from the extreme colours of rule 2 (with the complement, and with the
/// colour twice), light and dark, at the three opacities.
#[test]
fn auroras_generated_palettes_read() {
    let bad = unreadable(auroras_extreme_styles());
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// The colours rule 2 generates Aurora's palettes from.
const EXTREMES: [(&str, u32); 7] = [
    ("white", 0xffffff),
    ("black", 0x000000),
    ("grey", 0x808080),
    ("yellow", 0xffff00),
    ("blue", 0x0000ff),
    ("lime", 0x00ff00),
    ("violet", 0x7c4dff),
];

/// Aurora's System and Freestyle palettes from each of the extreme
/// colours, light and dark, at three opacities.
fn auroras_extreme_styles() -> Vec<(String, Style)> {
    let mut out = Vec::new();
    let mut aurora = built_in("aurora");
    for dark in [false, true] {
        for opacity in [1.0, 0.5, 0.1] {
            for (name, hex) in EXTREMES {
                let colour = appcore::color::hex(hex);
                let as_text = |c: [f32; 3]| text(&appcore::color::to_hex(c));
                let mut ctx = with(&aurora, dark, opacity, &[("palette", text("system"))]);
                ctx.accent = Some(colour);
                out.push((
                    format!("aurora system {name} dark={dark} {opacity}"),
                    aurora.style(&ctx).unwrap().style.clone(),
                ));
                for (kind, second) in [
                    ("complement", appcore::color::complement(colour)),
                    ("twice", colour),
                ] {
                    let ctx = with(
                        &aurora,
                        dark,
                        opacity,
                        &[
                            ("palette", text("custom")),
                            ("primary", as_text(colour)),
                            ("secondary", as_text(second)),
                        ],
                    );
                    out.push((
                        format!("aurora freestyle {name} {kind} dark={dark} {opacity}"),
                        aurora.style(&ctx).unwrap().style.clone(),
                    ));
                }
            }
        }
    }
    out
}

/// Sprinkles' System flavour on a desktop of each extreme accent, and
/// Freestyle iced in each (its second colour the complement, and the
/// icing again), light and dark, at three opacities.
fn sprinkles_extreme_styles() -> Vec<(String, Style)> {
    let mut out = Vec::new();
    let mut sprinkles = built_in("sprinkles");
    for dark in [false, true] {
        for opacity in [1.0, 0.5, 0.1] {
            for (name, hex) in EXTREMES {
                let colour = appcore::color::hex(hex);
                let as_text = |c: [f32; 3]| text(&appcore::color::to_hex(c));
                let mut ctx = with(&sprinkles, dark, opacity, &[("flavour", text("system"))]);
                ctx.accent = Some(colour);
                out.push((
                    format!("sprinkles system {name} dark={dark} {opacity}"),
                    sprinkles.style(&ctx).unwrap().style.clone(),
                ));
                for (kind, second) in [
                    ("complement", appcore::color::complement(colour)),
                    ("twice", colour),
                ] {
                    let ctx = with(
                        &sprinkles,
                        dark,
                        opacity,
                        &[
                            ("flavour", text("custom")),
                            ("icing", as_text(colour)),
                            ("second", as_text(second)),
                        ],
                    );
                    out.push((
                        format!("sprinkles freestyle {name} {kind} dark={dark} {opacity}"),
                        sprinkles.style(&ctx).unwrap().style.clone(),
                    ));
                }
            }
        }
    }
    out
}

/// Rules 3 and 4 for Sprinkles' System and Freestyle flavours iced in the
/// extreme colours of rule 2, light and dark, at the three opacities.
#[test]
fn sprinkles_own_flavours_read() {
    let bad = unreadable(sprinkles_extreme_styles());
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// Rules 3, 4 and 5 for Sprinkles while its header becomes the awning and
/// its window's own colour goes (from 1 to 0.95), in every flavour and in
/// System and Freestyle in rule 2's colours, light and dark.
#[test]
fn sprinkles_reads_as_the_header_becomes_the_awning() {
    let mut sprinkles = built_in("sprinkles");
    let mut styles = Vec::new();
    for dark in [false, true] {
        for opacity in [0.995, 0.99, 0.98, 0.97, 0.96, 0.95] {
            for flavour in FLAVOURS {
                let ctx = with(&sprinkles, dark, opacity, &[("flavour", text(flavour))]);
                let style = sprinkles.style(&ctx).unwrap().style.clone();
                styles.push((format!("sprinkles {flavour} dark={dark} {opacity}"), style));
            }
            for (name, hex) in EXTREMES {
                let colour = appcore::color::hex(hex);
                let mut ctx = with(&sprinkles, dark, opacity, &[("flavour", text("system"))]);
                ctx.accent = Some(colour);
                let style = sprinkles.style(&ctx).unwrap().style.clone();
                styles.push((
                    format!("sprinkles system {name} dark={dark} {opacity}"),
                    style,
                ));
                let as_text = |c: [f32; 3]| text(&appcore::color::to_hex(c));
                let ctx = with(
                    &sprinkles,
                    dark,
                    opacity,
                    &[
                        ("flavour", text("custom")),
                        ("icing", as_text(colour)),
                        ("second", as_text(appcore::color::complement(colour))),
                    ],
                );
                let style = sprinkles.style(&ctx).unwrap().style.clone();
                styles.push((
                    format!("sprinkles freestyle {name} dark={dark} {opacity}"),
                    style,
                ));
            }
        }
    }
    let mut bad = unreadable(styles.clone());
    bad.extend(rings_unreadable(styles));
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
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
                    bad.push(format!("{name}{over}: {what} {got:.2} < {want}"));
                }
            }
        }
    }
    bad
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
        for (name, hex) in EXTREMES {
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

// ---- Aurora: text over light that moves ---------------------------------------------

/// A `ctx` for each thing Aurora can look like: its seven palettes, and
/// System and Freestyle from rule 2's extreme colours (with the
/// complement, and with the colour twice), light and dark, at three
/// opacities.
fn auroras_ctxs(aurora: &Theme) -> Vec<(String, Ctx)> {
    let mut out = Vec::new();
    for dark in [false, true] {
        // (Also at the opacities between the rules' three: what a
        // see-through window shows changes all the way down.)
        for opacity in [1.0, 0.85, 0.7, 0.5, 0.3, 0.15, 0.1] {
            for palette in PALETTES {
                let ctx = with(aurora, dark, opacity, &[("palette", text(palette))]);
                out.push((format!("aurora {palette} dark={dark} {opacity}"), ctx));
            }
            for (name, hex) in EXTREMES {
                let colour = appcore::color::hex(hex);
                let as_text = |c: [f32; 3]| text(&appcore::color::to_hex(c));
                let mut ctx = with(aurora, dark, opacity, &[("palette", text("system"))]);
                ctx.accent = Some(colour);
                out.push((format!("aurora system {name} dark={dark} {opacity}"), ctx));
                for (kind, second) in [
                    ("complement", appcore::color::complement(colour)),
                    ("twice", colour),
                ] {
                    let options = [
                        ("palette", text("custom")),
                        ("primary", as_text(colour)),
                        ("secondary", as_text(second)),
                    ];
                    out.push((
                        format!("aurora freestyle {name} {kind} dark={dark} {opacity}"),
                        with(aurora, dark, opacity, &options),
                    ));
                }
            }
        }
    }
    out
}

/// Every colour Aurora's window can show under what is in it, over
/// `desktop` (none: the window is opaque). The sky is the window's own
/// background, a gradient top to bottom, which the app thins to the
/// window's opacity. Over it the background layer, which thins itself
/// (`fade = false`), draws stars, and through a mask the palette's lights
/// (a gradient by height, as strong as the layer makes them at that
/// opacity): so a pixel at some height is the sky there over the desktop,
/// the light there over that, or a blend of the two; or a star, and a
/// star is no brighter than one colour. `tests/aurora.rs` holds the layer
/// to that, standing, drifting and with every kind of pulse
/// (`the_background_is_lights_and_stars_and_nothing_else`).
///
/// Sampled at 17 heights: the sky, the light, and three blends of them;
/// and a star at its brightest, at the top and at the bottom.
fn auroras_sky_colours(
    aurora: &Theme,
    ctx: &Ctx,
    style: &Style,
    desktop: Option<Rgba>,
) -> Vec<Rgba> {
    use gmnb_theme::draw::Op;
    use gmnb_theme::layer::{Frame, LayerKind};
    let Some(Paint::Gradient(sky)) = style.fill("window.background").0.first().cloned() else {
        panic!("Aurora's window.background is the sky's gradient");
    };
    assert!(sky.colors.iter().all(|c| c[3] == 1.0), "{sky:?}");
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
            } => {
                assert!(lights.is_none(), "one gradient: the lights");
                lights = Some(g.clone());
            }
            // A star: the brightest is the one most opaque.
            Op::Rect {
                paint: Paint::Color(c),
                ..
            } if star.is_none_or(|s| c[3] > s[3]) => star = Some(*c),
            _ => {}
        }
    }
    let lights = lights.expect("the lights");
    // An opaque window shows all of each light.
    assert!(
        ctx.opacity < 1.0 || lights.colors.iter().all(|c| c[3] == 1.0),
        "{lights:?}"
    );
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

/// Rules 3 and 4 at the aurora's worst: every pair of the table with each
/// colour the window can show under it in turn (the sky, each light at
/// its height, the blends between them, a star), opaque and see-through
/// over black and over white (the sky thinned to the window's opacity,
/// the lights hanging over the desktop as strong as the layer draws
/// them), in each of the seven palettes and in System and Freestyle from
/// the extreme colours. Wherever a curtain wanders, and whatever pulse
/// plays, each pixel is one of those.
#[test]
fn auroras_texts_read_over_every_colour_of_the_sky() {
    let mut aurora = built_in("aurora");
    let mut bad = Vec::new();
    for (name, ctx) in auroras_ctxs(&aurora) {
        let style = aurora.style(&ctx).unwrap().style.clone();
        let all = {
            let mut all = page_pairs(&style, true);
            all.extend(chrome_pairs(&style));
            all.extend(pairs(&style, true));
            all
        };
        let desktops: &[Option<Rgba>] = if style.opacity >= 1.0 {
            &[None]
        } else {
            &[Some(BLACK), Some(WHITE)]
        };
        for desktop in desktops {
            let over_what = match desktop {
                None => "",
                Some(d) if *d == BLACK => " over black",
                Some(_) => " over white",
            };
            let colours = auroras_sky_colours(&aurora, &ctx, &style, *desktop);
            // The worst of each pair over all the sky's colours.
            let mut worst_of = vec![(f32::INFINITY, 0usize); all.len()];
            for (k, shown) in colours.iter().enumerate() {
                let behind = [*shown; 3];
                for (i, (_, text, _, fills)) in all.iter().enumerate() {
                    let fills: Vec<&Fill> = fills.iter().collect();
                    let got = worst(*text, &fills, &behind);
                    if got < worst_of[i].0 {
                        worst_of[i] = (got, k);
                    }
                }
            }
            for ((what, _, at_rest, _), (got, k)) in all.iter().zip(worst_of) {
                let want = match desktop {
                    None => *at_rest,
                    Some(_) => at_rest.min(UI_CONTRAST),
                };
                if got < want {
                    bad.push(format!(
                        "{name}{over_what}: {what} {got:.2} < {want} (over colour {k})"
                    ));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// Rule 5 at the aurora's worst: around a key the ring lies on the sky or
/// on a curtain, any colour the background can show; its edge keeps it
/// seen where the light is the ring's own colour. In every palette and
/// from the extreme colours, opaque and see-through.
#[test]
fn auroras_focus_ring_reads_over_every_colour_of_the_sky() {
    let mut aurora = built_in("aurora");
    let mut bad = Vec::new();
    for (name, ctx) in auroras_ctxs(&aurora) {
        let style = aurora.style(&ctx).unwrap().style.clone();
        let desktops: &[Option<Rgba>] = if style.opacity >= 1.0 {
            &[None]
        } else {
            &[Some(BLACK), Some(WHITE)]
        };
        for desktop in desktops {
            let behind = auroras_sky_colours(&aurora, &ctx, &style, *desktop);
            bad.extend(rings_read(&format!("{name} {desktop:?}"), &style, &behind));
        }
    }
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

// ---- Swirls: text over paint that moves ------------------------------------------

/// Swirls' sets of bands and strengths of glass: every value of the two
/// options of its that change a colour (System here on a desktop that
/// shares no accent, Freestyle in its own colours; `swirls_own_cases`
/// has them in many more).
const SWIRLS_BANDS: [&str; 8] = [
    "party", "lava", "lagoon", "orchid", "absinthe", "marble", "system", "custom",
];
const SWIRLS_GLASS: [&str; 3] = ["clear", "smoked", "panel"];

/// Swirls in every set of bands under every glass, at three opacities.
/// (It is one look in light and in dark: `swirls_is_dark_in_both_modes`.)
fn swirls_styles() -> Vec<(String, Style)> {
    let mut out = Vec::new();
    let mut swirls = built_in("swirls");
    for opacity in [1.0, 0.5, 0.1] {
        for bands in SWIRLS_BANDS {
            for glass in SWIRLS_GLASS {
                let options = [("bands", text(bands)), ("glass", text(glass))];
                let ctx = with(&swirls, true, opacity, &options);
                let style = swirls.style(&ctx).unwrap().style.clone();
                out.push((format!("swirls {bands} {glass} {opacity}"), style));
            }
        }
    }
    out
}

/// Swirls' System on a desktop of each of `colours`, and Freestyle from
/// each: with its complement over a dark band of the next colour, and the
/// colour three times over. Under each of `glasses`, at each of
/// `opacities`.
fn swirls_own_cases(colours: &[[f32; 3]], glasses: &[&str], opacities: &[f32]) -> Vec<Swirl> {
    let mut out = Vec::new();
    let (mut swirls, clock) = swirls_on_a_clock();
    let hex = |c: [f32; 3]| appcore::color::to_hex(c);
    for &opacity in opacities {
        for &glass in glasses {
            for (i, &c) in colours.iter().enumerate() {
                let mut ctx = with(
                    &swirls,
                    true,
                    opacity,
                    &[("bands", text("system")), ("glass", text(glass))],
                );
                ctx.accent = Some(c);
                let name = format!("swirls system {} {glass} {opacity}", hex(c));
                out.push(swirl(&mut swirls, &clock, name, &ctx));
                let next = colours[(i + 1) % colours.len()];
                for [first, second, dark] in [[c, appcore::color::complement(c), next], [c, c, c]] {
                    let ctx = with(
                        &swirls,
                        true,
                        opacity,
                        &[
                            ("bands", text("custom")),
                            ("glass", text(glass)),
                            ("paint_first", text(&hex(first))),
                            ("paint_second", text(&hex(second))),
                            ("paint_dark", text(&hex(dark))),
                        ],
                    );
                    let name = format!(
                        "swirls freestyle {} {} {} {glass} {opacity}",
                        hex(first),
                        hex(second),
                        hex(dark)
                    );
                    out.push(swirl(&mut swirls, &clock, name, &ctx));
                }
            }
        }
    }
    out
}

/// Tests 1 and 8 for Swirls: every property the stylesheet reads is
/// written, and rules 3 and 4 hold in every set of bands under every
/// glass, the bits' indexes included. Here what is under the window is
/// the three bands themselves (its `window.background` is a gradient of
/// them, which `backdrop` samples at both ends and the middle);
/// `swirls_texts_read_over_every_colour_of_the_field` goes through the
/// blends between them too.
#[test]
fn swirls_page_texts_read() {
    let writer = Writer::new(STYLESHEET);
    let styles = swirls_styles();
    let mut bad = Vec::new();
    for (name, style) in &styles {
        let missing = writer.missing(&css::properties(style));
        assert!(missing.is_empty(), "{name}: not emitted: {missing:#?}");
        let Some(Value::Text(index)) = style.get("programmer.bit_index") else {
            panic!("no programmer.bit_index");
        };
        let plain = style.fill("plain.background");
        let desktops: &[Option<Rgba>] = if style.opacity >= 1.0 {
            &[None]
        } else {
            &[Some(BLACK), Some(WHITE)]
        };
        for desktop in desktops {
            let got = worst(index.color, &[&plain], &backdrop(style, *desktop));
            if got < PLACEHOLDER_CONTRAST {
                bad.push(format!("{name} {desktop:?}: the bits' indexes {got:.2}"));
            }
        }
    }
    bad.extend(unreadable(styles));
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// The app's Light and Dark are one look in Swirls, as the page it comes
/// from has one: the generated style is the same, property for property.
#[test]
fn swirls_is_dark_in_both_modes() {
    let mut swirls = built_in("swirls");
    for opacity in [1.0, 0.5] {
        let light = with(&swirls, false, opacity, &[]);
        let dark = with(&swirls, true, opacity, &[]);
        let light = css::properties(&swirls.style(&light).unwrap().style);
        let dark = css::properties(&swirls.style(&dark).unwrap().style);
        assert_eq!(light, dark);
    }
}

/// One look of Swirls: its style, and the colours its paint can show,
/// taken from what its background layer draws.
#[derive(Clone)]
struct Swirl {
    name: String,
    style: Style,
    /// Every entry of the ramp the paint is coloured through (on a
    /// see-through window, the paint thinned: each with its opacity), the
    /// first band's first. Its layer is drawn at full strength
    /// (`fade = false`), in no group of its own, over `under`.
    /// `tests/swirls.rs` holds the layers to it: every pixel they draw,
    /// stirred or still, is one of these.
    field: Vec<Rgba>,
    /// On a see-through window, what lies under the paint:
    /// `window.background`, one colour there (the veil of the dark band,
    /// gone from 0.95 down), which the app thins with the window.
    under: Rgba,
    /// What the figures are painted in: the first band of the bands
    /// themselves, solid at any opacity.
    figures: Vec<Rgba>,
}

/// Every entry of `ramp`'s table, its colour and its opacity.
fn entries(ramp: &gmnb_media::Ramp) -> Vec<Rgba> {
    (0..ramp.colors() * 256)
        .map(|k| {
            let [r, g, b, a] = ramp.at((k as f32 + 0.5) / 256.0);
            let a = a as f32 / 255.0;
            let straight = |c: u8| {
                if a > 0.0 {
                    (c as f32 / 255.0 / a).min(1.0)
                } else {
                    0.0
                }
            };
            [straight(r), straight(g), straight(b), a]
        })
        .collect()
}

/// The ramp Swirls' background is drawn through for `ctx`, drawn on a
/// clock of its own (the fields' allowance runs on it); the field in no
/// opacity group, as the paint's own opacity is all its thinning.
fn paint_ramp(
    theme: &Theme,
    ctx: &Ctx,
    clock: &std::rc::Rc<std::cell::Cell<f64>>,
) -> std::rc::Rc<gmnb_media::Ramp> {
    use gmnb_theme::draw::Op;
    use gmnb_theme::layer::{Frame, LayerKind};
    clock.set(clock.get() + 1.0);
    assert!(
        theme.def.layers["background"].unfaded,
        "Swirls' paint thins itself"
    );
    let drawn = theme
        .draw(LayerKind::Background, ctx, &Frame::sized(360.0, 600.0))
        .unwrap_or_else(|_| panic!("the background didn't draw"));
    let mut ramps = Vec::new();
    for op in &drawn.list.ops {
        match op {
            Op::PushOpacity(_) => panic!("the paint is thinned in a group"),
            Op::Field { picture, .. } => ramps.push(picture.ramp.clone()),
            _ => {}
        }
    }
    assert_eq!(ramps.len(), 1);
    ramps.remove(0)
}

/// Swirl for `ctx`: its style, its paint and its figures.
fn swirl(
    theme: &mut Theme,
    clock: &std::rc::Rc<std::cell::Cell<f64>>,
    name: String,
    ctx: &Ctx,
) -> Swirl {
    let style = theme.style(ctx).unwrap().style.clone();
    let mut under = [0.0; 4];
    if ctx.opacity < 1.0 {
        let veil = style.fill("window.background").samples_over([0.0; 4]);
        assert!(
            veil.iter().all(|c| *c == veil[0]),
            "{name}: under the paint, more than one colour"
        );
        under = veil[0];
    }
    let field = entries(&paint_ramp(theme, ctx, clock));
    let whole = Ctx {
        opacity: 1.0,
        ..ctx.clone()
    };
    let figures = entries(&paint_ramp(theme, &whole, clock))[..256].to_vec();
    assert!(
        figures.iter().all(|c| c[3] == 1.0),
        "{name}: figures not solid"
    );
    Swirl {
        name,
        style,
        field,
        under,
        figures,
    }
}

/// Swirls, with a clock for its fields.
fn swirls_on_a_clock() -> (Theme, std::rc::Rc<std::cell::Cell<f64>>) {
    let theme = built_in("swirls");
    let clock = std::rc::Rc::new(std::cell::Cell::new(100.0));
    let read = clock.clone();
    theme.api().set_clock(std::rc::Rc::new(move || read.get()));
    (theme, clock)
}

/// `swirls_styles`, with their paint: every set of bands under every
/// glass at `opacities`.
fn swirls_cases(opacities: &[f32], glasses: &[&str]) -> Vec<Swirl> {
    let (mut swirls, clock) = swirls_on_a_clock();
    let mut out = Vec::new();
    for &opacity in opacities {
        for bands in SWIRLS_BANDS {
            for &glass in glasses {
                let options = [("bands", text(bands)), ("glass", text(glass))];
                let ctx = with(&swirls, true, opacity, &options);
                let name = format!("swirls {bands} {glass} {opacity}");
                out.push(swirl(&mut swirls, &clock, name, &ctx));
            }
        }
    }
    out
}

fn named(cases: &[Swirl]) -> Vec<(String, Style)> {
    cases
        .iter()
        .map(|c| (c.name.clone(), c.style.clone()))
        .collect()
}

/// Rules 3 and 4 at the field's worst: every pair of the table, in every
/// set of bands under every glass, with each of the 768 colours its ramp
/// holds under the window in turn (the three bands and every blend
/// between two of them), opaque and see-through over black and over
/// white. Whatever the paint is doing, each pixel of it is one of those.
///
/// And the figures made of paint: they are drawn in the first band's
/// colours (the warm one to the cool one), and each of those reads on the
/// readout's panel, on the converter's fields and on the date page's
/// result, over every colour of the field.
#[test]
fn swirls_texts_read_over_every_colour_of_the_field() {
    let bad = swirls_unreadable_over_the_field(&swirls_cases(&[1.0, 0.5, 0.1], &SWIRLS_GLASS));
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// The paint thins with the window's opacity all the way down, and the
/// glass with it: so the same at every twentieth from 0.1 to 1, in every
/// set of bands under the clearest glass (the others are only ever
/// stronger), every pair of the table, the figures and the focus ring over
/// every colour of the field.
#[test]
fn swirls_reads_at_every_opacity() {
    let mut opacities: Vec<f32> = (2..=20).map(|step| step as f32 / 20.0).collect();
    // And through the veil's going, from 1 to 0.7, by fiftieths.
    opacities.extend([
        0.98, 0.96, 0.94, 0.92, 0.88, 0.86, 0.84, 0.82, 0.78, 0.76, 0.74, 0.72,
    ]);
    let cases = swirls_cases(&opacities, &["clear"]);
    let mut bad = swirls_unreadable_over_the_field(&cases);
    bad.extend(swirls_rings_unreadable_over_the_field(&cases));
    bad.extend(unreadable(named(&cases)));
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// The same for System under a desktop of each extreme accent of rule 2,
/// and Freestyle from each (with its complement over a dark band of the
/// next, and the colour three times over), under every glass, opaque and
/// see-through: rules 3, 4 and 5 over the bands worked out from them and
/// every blend between.
#[test]
fn swirls_own_colours_read_over_every_colour_of_the_field() {
    let extremes = EXTREMES.map(|(_, hex)| appcore::color::hex(hex));
    // (0.95, 0.85 and 0.75: inside the veil's going.)
    let cases = swirls_own_cases(
        &extremes,
        &SWIRLS_GLASS,
        &[1.0, 0.95, 0.85, 0.75, 0.5, 0.15, 0.1],
    );
    let mut bad = swirls_unreadable_over_the_field(&cases);
    bad.extend(unreadable(named(&cases)));
    bad.extend(swirls_rings_unreadable_over_the_field(&cases));
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// And under any desktop accent, and any colours picked: System from
/// colours all round the wheel (every 15° of hue, light, dark, strong and
/// grey) and from sixty at random, Freestyle from each with its
/// complement over a dark band of the next, and three times over; under
/// the clearest glass, opaque and see-through, rules 3, 4 and 5 over
/// every colour of the field.
#[test]
#[ignore = "a long sweep (a minute or more): cargo test --release -p gmnb-theme --test css -- --ignored"]
fn swirls_under_any_accent_read_over_every_colour_of_the_field() {
    let mut colours = Vec::new();
    for hue in (0..360).step_by(15) {
        for (s, l) in [(1.0, 0.5), (1.0, 0.3), (1.0, 0.75), (0.45, 0.5), (0.1, 0.4)] {
            colours.push(appcore::color::from_hsl(hue as f32, s, l));
        }
    }
    // And sixty colours at random.
    let mut seed = 17u64;
    for _ in 0..60 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        colours.push(appcore::color::hex(((seed >> 33) & 0xff_ffff) as u32));
    }
    let cases = swirls_own_cases(&colours, &["clear"], &[1.0, 0.5, 0.1]);
    let mut bad = swirls_unreadable_over_the_field(&cases);
    bad.extend(swirls_rings_unreadable_over_the_field(&cases));
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// What a colour of the field shows as: itself on an opaque window, and
/// on a see-through one laid over the desktop (nothing else is under it).
fn shown(case: &Swirl, colour: Rgba, desktop: Option<Rgba>) -> Rgba {
    match desktop {
        None => {
            assert_eq!(colour[3], 1.0, "the paint is solid on an opaque window");
            colour
        }
        Some(d) => over(colour, over(alpha(case.under, case.style.opacity), d)),
    }
}

fn swirls_unreadable_over_the_field(cases: &[Swirl]) -> Vec<String> {
    let mut bad = Vec::new();
    for case in cases {
        let Swirl {
            name,
            style,
            field: colours,
            figures,
            ..
        } = case;
        let all = {
            let mut all = page_pairs(style, true);
            all.extend(chrome_pairs(style));
            all.extend(pairs(style, true));
            all
        };
        // What the figures lie on: the readout; a converter's field, at
        // rest and being typed into, on its pane; a date's result on its.
        let plain = style.fill("plain.background");
        let field = style.control("converter.field").unwrap();
        let Some(Value::Surface(result)) = style.get("date.result_panel") else {
            panic!("no date.result_panel");
        };
        let under_figures: Vec<(&str, Vec<Fill>)> = vec![
            ("the readout", vec![style.fill("display.background")]),
            (
                "a converter's value",
                vec![field.finished(&[]).background, plain.clone()],
            ),
            (
                "a converter's value, typed into",
                vec![field.finished(&[Which::Checked]).background, plain.clone()],
            ),
            (
                "a date's result",
                vec![result.background.clone(), plain.clone()],
            ),
        ];
        let desktops: &[Option<Rgba>] = if style.opacity >= 1.0 {
            &[None]
        } else {
            &[Some(BLACK), Some(WHITE)]
        };
        for desktop in desktops {
            let over_what = match desktop {
                None => "",
                Some(d) if *d == BLACK => " over black",
                Some(_) => " over white",
            };
            // The worst of each pair over all the field's colours.
            let mut worst_of = vec![(f32::INFINITY, 0usize); all.len()];
            let mut worst_figure = vec![(f32::INFINITY, 0usize); under_figures.len()];
            for (k, colour) in colours.iter().enumerate() {
                let behind = [shown(case, *colour, *desktop); 3];
                for (i, (_, text, _, fills)) in all.iter().enumerate() {
                    let fills: Vec<&Fill> = fills.iter().collect();
                    let got = worst(*text, &fills, &behind);
                    if got < worst_of[i].0 {
                        worst_of[i] = (got, k);
                    }
                }
                for (i, (_, fills)) in under_figures.iter().enumerate() {
                    let fills: Vec<&Fill> = fills.iter().collect();
                    // (Every eighth of the first band's entries, and its last.)
                    for figure in figures.iter().step_by(8).chain(&figures[255..256]) {
                        let got = worst(*figure, &fills, &behind);
                        if got < worst_figure[i].0 {
                            worst_figure[i] = (got, k);
                        }
                    }
                }
            }
            for ((what, _, at_rest, _), (got, k)) in all.iter().zip(worst_of) {
                let want = match desktop {
                    None => *at_rest,
                    Some(_) => at_rest.min(UI_CONTRAST),
                };
                if got < want {
                    bad.push(format!(
                        "{name}{over_what}: {what} {got:.2} < {want} (over entry {k})"
                    ));
                }
            }
            // A figure may be as small as 14 px: text.
            for ((what, _), (got, k)) in under_figures.iter().zip(worst_figure) {
                let want = match desktop {
                    None => TEXT_CONTRAST,
                    Some(_) => UI_CONTRAST,
                };
                if got < want {
                    bad.push(format!(
                        "{name}{over_what}: figures of paint on {what} {got:.2} < {want} (over entry {k})"
                    ));
                }
            }
        }
    }
    bad
}

// ---- Rule 5: the keyboard's focus ring ------------------------------------------

/// What a style's ring lies on besides a key: the page's surfaces a
/// control may sit on, each over the window's background.
const RING_SURFACES: [&str; 4] = [
    "plain.background",
    "header.background",
    "nav.background",
    "panel",
];

/// Rule 5 for one style, over each colour of `behind` in turn (what the
/// window's background shows as): every ring at least 1 px wide and
/// reaching 3 on what it lies on. Around a key: the window's background,
/// and the key's own fill where the ring is inside it, the ring or its
/// edge counting. Around a control on one of the page's surfaces: that
/// surface, the ring alone. A ring left to the toolkit is judged as
/// libadwaita draws it.
fn rings_read(name: &str, style: &Style, behind: &[Rgba]) -> Vec<String> {
    use gmnb_theme::focus::{self, LEAST_CONTRAST, LEAST_WIDTH};
    let get = |p: &str| style.get(p).cloned();
    let mut bad = Vec::new();
    let mut check = |what: &str, got: f32| {
        if got < LEAST_CONTRAST {
            bad.push(format!("{name}: {what} {got:.2} < {LEAST_CONTRAST}"));
        }
    };
    for path in focus::RINGS {
        let native = path == "focus" && style.native().contains(&"focus");
        let ring = if native {
            focus::native_ring(style.color("stock.accent"))
        } else {
            focus::ring_at(get, path).unwrap_or_else(|| panic!("{name}: no ring at {path}"))
        };
        if ring.width < LEAST_WIDTH {
            check(&format!("{path}, {} px wide,", ring.width), 0.0);
        }
        let grounds = focus::grounds(get, &style.tokens, path, behind);
        let got = focus::worst(&ring, &grounds.under(&ring), grounds.edge && !native);
        check(
            if path == "focus" {
                "focus around a key"
            } else {
                path
            },
            got,
        );
        if path == "focus" {
            // On a control on a surface: the ring alone.
            for surface in RING_SURFACES {
                let fill = match style.get(surface) {
                    Some(Value::Surface(s)) => s.background.clone(),
                    _ => style.fill(surface),
                };
                let on: Vec<Rgba> = behind.iter().flat_map(|b| fill.samples_over(*b)).collect();
                check(
                    &format!("focus on {surface}"),
                    focus::worst(&ring, &on, false),
                );
            }
        }
    }
    bad
}

/// Rule 5: in the base look and every built-in theme, every palette and
/// flavour, light and dark, opaque and see-through over a black and a
/// white desktop, the keyboard's focus ring reads on what it lies on.
/// None of them leans on the floor: their own rings pass, with no note.
#[test]
fn every_focus_ring_reads() {
    let mut styles = every_style();
    // Aurora's System and Freestyle from the extreme colours too, and
    // Sprinkles' and Swirls'.
    styles.extend(auroras_extreme_styles());
    styles.extend(sprinkles_extreme_styles());
    let mut bad = rings_unreadable(styles);
    for id in ["aurora", "sprinkles", "swirls"] {
        let mut theme = built_in(id);
        for dark in [false, true] {
            for opacity in [1.0, 0.5, 0.1] {
                let ctx = with(&theme, dark, opacity, &[]);
                for note in &theme.style(&ctx).unwrap().notes.list {
                    if note.message.contains("focus") {
                        bad.push(format!("{id} dark={dark} {opacity}: {note}"));
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// Rule 5 between those opacities as well: every twentieth from 0.1 to
/// 1, over a black and a white desktop, for the base look, Sprinkles'
/// flavours and Aurora's palettes. (Their rings and edges are less than
/// 9:1 apart: that they read at 0.5 and 0.1 doesn't say they read at 0.7.)
#[test]
fn every_focus_ring_reads_at_every_opacity() {
    let mut styles = Vec::new();
    let mut aurora = built_in("aurora");
    let mut sprinkles = built_in("sprinkles");
    for dark in [false, true] {
        for step in 2..=20 {
            let opacity = step as f32 / 20.0;
            let ctx = Ctx {
                dark,
                opacity,
                ..Ctx::default()
            };
            styles.push((format!("base dark={dark} {opacity}"), Style::base(&ctx)));
            for flavour in FLAVOURS {
                let ctx = with(&sprinkles, dark, opacity, &[("flavour", text(flavour))]);
                let style = sprinkles.style(&ctx).unwrap().style.clone();
                styles.push((format!("sprinkles {flavour} dark={dark} {opacity}"), style));
            }
            for palette in PALETTES {
                let ctx = with(&aurora, dark, opacity, &[("palette", text(palette))]);
                let style = aurora.style(&ctx).unwrap().style.clone();
                styles.push((format!("aurora {palette} dark={dark} {opacity}"), style));
            }
        }
    }
    let bad = rings_unreadable(styles);
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// Rule 5 for each style, opaque, or see-through over a black and a white
/// desktop: what fails.
fn rings_unreadable(styles: Vec<(String, Style)>) -> Vec<String> {
    let mut bad = Vec::new();
    for (name, style) in styles {
        let desktops: Vec<Option<Rgba>> = if style.opacity >= 1.0 {
            vec![None]
        } else {
            vec![Some(BLACK), Some(WHITE)]
        };
        for desktop in desktops {
            let behind = gmnb_theme::focus::backdrop(
                |p| style.get(p).cloned(),
                style.tokens.colors.bg,
                desktop.map(|d| (d, style.opacity)),
            );
            let over = match desktop {
                None => "",
                Some(d) if d == BLACK => " over black",
                Some(_) => " over white",
            };
            bad.extend(rings_read(&format!("{name}{over}"), &style, &behind));
        }
    }
    bad
}

/// Rule 5 for Swirls at the field's worst: around a key its ring lies on
/// the paint itself, any of the 768 colours of its ramp, the ring's gold
/// among them; its edge keeps it seen there. In every set of bands under
/// every glass, opaque and see-through.
#[test]
fn swirls_focus_ring_reads_over_every_colour_of_the_field() {
    let cases = swirls_cases(&[1.0, 0.5, 0.1], &SWIRLS_GLASS);
    let bad = swirls_rings_unreadable_over_the_field(&cases);
    // The gold alone vanishes on the gold paint: what reads is its edge.
    // (So in the design's own sets, on an opaque window. In one's own the
    // ring may have been lifted off the first band, and then it reads alone
    // as well.)
    for Swirl {
        name, style, field, ..
    } in &cases
    {
        if name.contains("system") || name.contains("custom") || style.opacity < 1.0 {
            continue;
        }
        let Some(Value::FocusRing(ring)) = style.get("focus") else {
            panic!("{name}: no focus ring");
        };
        let bare = FocusRing {
            edge: None,
            ..*ring
        };
        let got = gmnb_theme::focus::worst(&bare, field, false);
        assert!(
            got < 1.5,
            "{name}: the bare ring reads ({got:.2}): the edge isn't tested"
        );
    }
    assert!(bad.is_empty(), "{} fail:\n{}", bad.len(), bad.join("\n"));
}

/// Rule 5 for each look of Swirls over every colour of its field, opaque
/// or see-through over a black and a white desktop: what fails.
fn swirls_rings_unreadable_over_the_field(cases: &[Swirl]) -> Vec<String> {
    let mut bad = Vec::new();
    for case in cases {
        let Swirl {
            name, style, field, ..
        } = case;
        let desktops: &[Option<Rgba>] = if style.opacity >= 1.0 {
            &[None]
        } else {
            &[Some(BLACK), Some(WHITE)]
        };
        for desktop in desktops {
            let behind: Vec<Rgba> = field.iter().map(|c| shown(case, *c, *desktop)).collect();
            bad.extend(rings_read(&format!("{name} {desktop:?}"), style, &behind));
        }
    }
    bad
}

/// The floor (rule 5's): a ring that would hide the keyboard's focus,
/// under 1 px, transparent or the colour of what is under it, is the base
/// look's ring instead, with a note; a ring left to the toolkit that
/// wouldn't read is the app's. A ring that reads stands.
#[test]
fn a_ring_that_would_hide_the_focus_is_the_base_looks() {
    use gmnb_theme::focus::{self, LEAST_CONTRAST};
    let ctx = Ctx::default();
    let base = Style::base(&ctx);
    let engine = Engine::with_globals(Limits::default(), |lua| {
        gmnb_theme::library::register(lua, gmnb_theme::library::without_drawing)
    })
    .unwrap();
    let style_of = |src: &str| resolve(&ctx, &engine.load("=style", src).unwrap());
    let hex = |c: Rgba| appcore::color::to_hex(rgb3(c));
    let bg = hex(base.tokens.colors.bg);
    let accent = hex(base.tokens.colors.accent_text);
    for (what, src) in [
        ("0 px", "return { focus = { width = 0 } }".to_string()),
        ("0.5 px", "return { focus = { width = 0.5 } }".to_string()),
        (
            "transparent",
            "return { focus = { color = '#00000000' } }".to_string(),
        ),
        (
            "the background's colour",
            format!("return {{ focus = {{ color = '{bg}', width = 3 }} }}"),
        ),
        (
            "inside keys of its own colour",
            format!(
                "return {{ focus = {{ offset = 0 }}, keys = {{ number = {{ background = '{accent}' }} }} }}"
            ),
        ),
        (
            "the converter's, hidden",
            "return { converter = { value = { focus = { width = 0 } } } }".to_string(),
        ),
        (
            "the calendar's, hidden",
            "return { date = { calendar = { focus = { color = '#00000000' } } } }".to_string(),
        ),
    ] {
        let resolved = style_of(&src);
        let noted = resolved
            .notes
            .list
            .iter()
            .any(|n| n.message.contains("would hide the keyboard's focus"));
        assert!(noted, "{what}: no note: {:?}", resolved.notes.list);
        let get = |p: &str| resolved.style.get(p).cloned();
        let behind = focus::backdrop(get, resolved.style.tokens.colors.bg, None);
        for path in focus::RINGS {
            let ring = focus::ring_at(get, path).unwrap();
            assert!(ring.width >= 1.0, "{what}: {path} {ring:?}");
            let grounds = focus::grounds(get, &resolved.style.tokens, path, &behind);
            let got = focus::worst(&ring, &grounds.under(&ring), grounds.edge);
            assert!(got >= LEAST_CONTRAST, "{what}: {path} {got:.2}");
        }
    }
    // A ring that reads stands, and so does the base look's own.
    let fine = style_of("return { focus = { width = 3, offset = 4 } }");
    assert!(fine.notes.list.is_empty(), "{:?}", fine.notes.list);
    let ring = focus::ring_at(|p| fine.style.get(p).cloned(), "focus").unwrap();
    assert_eq!((ring.width, ring.offset), (3.0, 4.0));
    let empty = gmnb_theme::ThemeValue::Map(BTreeMap::new());
    assert!(resolve(&ctx, &empty).notes.list.is_empty());
    // The toolkit's ring, the accent at half opacity, inside keys of the
    // accent's colour: the app's ring is drawn instead.
    let native = style_of(&format!(
        "return {{ native = {{ 'focus' }}, keys = {{ number = {{ background = '{accent}' }} }} }}"
    ));
    assert!(
        !native.style.native().contains(&"focus"),
        "{:?}",
        native.notes.list
    );
}

/// R4-A-1: a ring drawn inside its control (`offset` 0 or less) was judged
/// on the keys only, so a ring that reads inside a key but is the colour of
/// the header, the sidebar or a panel (where the controls that are not keys
/// are transparent) hid the keyboard's focus round every header button.
#[test]
fn a_ring_inside_its_control_that_hides_on_a_surface_is_the_base_looks() {
    use gmnb_theme::focus::{self, LEAST_CONTRAST};
    let ctx = Ctx::default();
    let engine = Engine::with_globals(Limits::default(), |lua| {
        gmnb_theme::library::register(lua, gmnb_theme::library::without_drawing)
    })
    .unwrap();
    for surface in ["header", "nav", "panel"] {
        let value = if surface == "panel" {
            "panel = { background = '#000000' }".to_string()
        } else {
            format!("{surface} = {{ background = '#000000' }}")
        };
        let src = format!(
            "return {{ focus = {{ color = '#000000', width = 2, offset = 0 }}, \
             window = {{ background = '#ffffff' }}, \
             keys = {{ number = {{ background = '#ffffff' }}, func = {{ background = '#ffffff' }}, \
             operator = {{ background = '#ffffff' }}, equals = {{ background = '#ffffff' }} }}, \
             {value} }}"
        );
        let resolved = resolve(&ctx, &engine.load("=style", &src).unwrap());
        let noted = resolved
            .notes
            .list
            .iter()
            .any(|n| n.message.contains("would hide the keyboard's focus"));
        assert!(noted, "{surface}: no note: {:?}", resolved.notes.list);
        let get = |p: &str| resolved.style.get(p).cloned();
        let behind = focus::backdrop(get, resolved.style.tokens.colors.bg, None);
        let ring = focus::ring_at(get, "focus").unwrap();
        let grounds = focus::grounds(get, &resolved.style.tokens, "focus", &behind);
        let got = focus::worst(&ring, &grounds.surfaces, false);
        assert!(got >= LEAST_CONTRAST, "{surface}: {got:.2}");
    }
}

/// R3-E-1b: the floor judged the `focus` ring on the window's background
/// and the keys only, so a theme whose ring reads there but not on the
/// header, the sidebar or a panel (black on black) hid the keyboard's focus
/// round every header button: the ring is the base look's ring, with a
/// note, when it reads on none of the page's surfaces, as rule 5 says.
#[test]
fn a_ring_that_hides_on_a_surface_of_the_page_is_the_base_looks() {
    use gmnb_theme::focus::{self, LEAST_CONTRAST};
    let ctx = Ctx::default();
    let engine = Engine::with_globals(Limits::default(), |lua| {
        gmnb_theme::library::register(lua, gmnb_theme::library::without_drawing)
    })
    .unwrap();
    for surface in ["header", "nav", "panel"] {
        let value = if surface == "panel" {
            "panel = { background = '#000000' }".to_string()
        } else {
            format!("{surface} = {{ background = '#000000' }}")
        };
        let src = format!(
            "return {{ focus = {{ color = '#000000', width = 2, offset = 2 }}, \
             window = {{ background = '#ffffff' }}, {value} }}"
        );
        let resolved = resolve(&ctx, &engine.load("=style", &src).unwrap());
        let noted = resolved
            .notes
            .list
            .iter()
            .any(|n| n.message.contains("would hide the keyboard's focus"));
        assert!(noted, "{surface}: no note: {:?}", resolved.notes.list);
        let get = |p: &str| resolved.style.get(p).cloned();
        let behind = focus::backdrop(get, resolved.style.tokens.colors.bg, None);
        let ring = focus::ring_at(get, "focus").unwrap();
        let grounds = focus::grounds(get, &resolved.style.tokens, "focus", &behind);
        let got = focus::worst(&ring, &grounds.surfaces, false);
        assert!(got >= LEAST_CONTRAST, "{surface}: {got:.2}");
    }
    // (R3-E-2) With the theme's own `accent_text` and `fg` black as well,
    // none of the base look's candidates reads on a white page and a black
    // header at once: the ring is a grey that reads on both.
    let src = "return { colors = { accent_text = '#000000', fg = '#000000' }, \
               focus = { color = '#000000', width = 2, offset = 2 }, \
               window = { background = '#ffffff' }, header = { background = '#000000' } }";
    let resolved = resolve(&ctx, &engine.load("=style", src).unwrap());
    let get = |p: &str| resolved.style.get(p).cloned();
    let behind = focus::backdrop(get, resolved.style.tokens.colors.bg, None);
    let ring = focus::ring_at(get, "focus").unwrap();
    let grounds = focus::grounds(get, &resolved.style.tokens, "focus", &behind);
    for (what, on) in [("surfaces", &grounds.surfaces), ("page", &grounds.outside)] {
        let got = focus::worst(&ring, on, false);
        assert!(got >= LEAST_CONTRAST, "{what}: {got:.2} ({ring:?})");
    }
}
