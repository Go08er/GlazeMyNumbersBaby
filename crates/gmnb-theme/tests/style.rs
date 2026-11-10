//! The style pipeline without a toolkit: resolution, the checks' notes,
//! the limits that apply to loading and `style`, and the built-in themes
//! in the real engine (docs/theming.md, "Tests the engine must have": 3,
//! 4, 5 and 7).

use std::collections::BTreeMap;
use std::time::Duration;

use appcore::metrics;
use gmnb_theme::theme::{self, LoadError, Source, Theme};
use gmnb_theme::types::*;
use gmnb_theme::{Ctx, Engine, Limits, OptionValue, Style, ThemeError, ThemeValue, resolve};

fn ctx(dark: bool, opacity: f32) -> Ctx {
    Ctx {
        dark,
        opacity,
        ..Ctx::default()
    }
}

/// What a script's `return …` converts to, with the `gmnb` library there.
fn value(src: &str) -> ThemeValue {
    let engine = Engine::with_globals(Limits::default(), |lua| {
        gmnb_theme::library::register(lua, gmnb_theme::library::without_drawing)
    })
    .unwrap();
    engine.load("=style", src).unwrap()
}

/// A theme in a folder of its own, from its `theme.luau`.
fn theme_dir(name: &str, source: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gmnb-theme-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("theme.luau"), source).unwrap();
    dir
}

fn load(name: &str, source: &str) -> Result<Theme, LoadError> {
    let dir = theme_dir(name, source);
    let limits = Limits {
        load: Duration::from_millis(100),
        style: Duration::from_millis(50),
        ..Limits::default()
    };
    let theme = Theme::load(Source::folder(name, &dir), limits);
    let _ = std::fs::remove_dir_all(&dir);
    theme
}

// ---- 3: `{}` is the base ---------------------------------------------------

#[test]
fn an_empty_style_is_exactly_the_base() {
    for dark in [false, true] {
        for opacity in [1.0, 0.5, 0.1] {
            let c = ctx(dark, opacity);
            let r = resolve(&c, &value("return {}"));
            assert!(r.notes.is_empty(), "{:?}", r.notes);
            assert_eq!(r.style, Style::base(&c));
        }
    }
    let mut theme = load(
        "empty",
        "return { api = 1, name = 'Empty', style = function() return {} end }",
    )
    .unwrap();
    let c = ctx(true, 1.0);
    assert_eq!(theme.style(&c).unwrap().style, Style::base(&c));
    // And so is a theme with no `style` at all.
    let mut theme = load("none", "return { api = 1, name = 'None' }").unwrap();
    assert_eq!(theme.style(&c).unwrap().style, Style::base(&c));
}

/// The resolved base is DGMNB's: pinned against the constants DGMNB draws
/// with, not against copies of them.
#[test]
fn the_base_is_dgmnbs() {
    for dark in [false, true] {
        let c = ctx(dark, 1.0);
        let s = Style::base(&c);
        let p = appcore::palette::Palette::new(dark, None);
        assert_eq!(s.fill("window.background"), Fill::color(p.bg));
        assert_eq!(s.color("window.color"), p.fg);
        assert_eq!(s.number("keys.gap"), metrics::calc::KEY_GAP);
        assert_eq!(s.number("keys.gap_compact"), metrics::calc::KEY_GAP_COMPACT);
        assert_eq!(s.radius("keys.radius"), Radius::all(metrics::shape::RADIUS));
        let number = s.control("keys.number").unwrap();
        assert_eq!(number.background, Fill::color(p.surface));
        assert_eq!(number.color, p.fg);
        assert_eq!(
            number.font.size,
            metrics::text::KEY.size + metrics::text::NUMBER_KEY_SIZE_ADD
        );
        assert_eq!(number.font.weight, metrics::text::NUMBER_KEY_WEIGHT);
        assert_eq!(number.font.family[0], INTER);
        assert_eq!(number.radius, Radius::all(metrics::shape::RADIUS));
        let func = s.control("keys.func").unwrap();
        assert_eq!(
            (func.background.clone(), func.color),
            (Fill::color(p.surface2), p.fg)
        );
        assert_eq!(func.font.size, metrics::text::KEY.size);
        assert_eq!(func.font.weight, metrics::text::KEY.weight);
        let op = s.control("keys.operator").unwrap();
        assert_eq!(
            (op.background.clone(), op.color),
            (Fill::color(p.surface2), p.accent_text)
        );
        let eq = s.control("keys.equals").unwrap();
        assert_eq!(
            (eq.background.clone(), eq.color),
            (Fill::color(p.accent), p.on_accent)
        );
        // Hover and press are overlays, a disabled key fades its glyph, and
        // a toggle that is on is the accent's.
        assert_eq!(func.hover, State::overlay(p.hover));
        assert_eq!(func.active, State::overlay(p.press));
        assert_eq!(func.disabled.fade, Some(metrics::controls::DISABLED_ALPHA));
        assert_eq!(func.checked, State::filled(p.accent, p.on_accent));
        assert_eq!(
            s.icon("keys.icon").unwrap().size,
            metrics::controls::KEY_ICON
        );
        let tab = s.control("panel.tabs.item").unwrap();
        assert_eq!(tab.min_height, metrics::panel::TAB_H);
        assert_eq!(tab.background, Fill::none());
        assert_eq!(tab.checked, State::filled(p.accent, p.on_accent));
        assert_eq!(
            (tab.font.size, tab.font.weight),
            (metrics::text::STRONG.size, metrics::text::STRONG.weight)
        );
        assert!(s.flag("panel.tabs.stretch"));
        assert_eq!(s.number("panel.tabs.gap"), 0.0);
        let seg = s.control("segmented.item").unwrap();
        assert_eq!(seg.min_height, metrics::controls::SEGMENTED_H);
        assert_eq!(
            seg.padding,
            Spacing::vh(0.0, metrics::controls::BUTTON_PAD_X)
        );
        assert_eq!(tab.padding, seg.padding);
        assert_eq!(seg.background, Fill::color(p.surface2));
        assert_eq!(seg.font.size, metrics::text::SMALL.size);
        assert_eq!(s.number("segmented.gap"), metrics::controls::SEGMENTED_GAP);
        // Nothing is see-through at full opacity.
        assert!(s.shadows("window.text_halo").is_empty());
        assert_eq!(s.fill("plain.background").as_color().unwrap()[3], 0.0);
        assert_eq!(s.number("icons.stroke"), metrics::shape::ICON_STROKE);
        assert_eq!(s.number("layout.calculator.panel_width"), 292.0);
    }
    // See-through: a scrim under text and a halo around it.
    let c = ctx(true, 0.1);
    let s = Style::base(&c);
    let p = appcore::palette::Palette::new(true, None);
    assert_eq!(s.fill("plain.background"), Fill::color(p.text_scrim(0.1)));
    assert_eq!(s.fill("header.background"), Fill::color(p.text_scrim(0.1)));
    assert_eq!(s.fill("display.background"), Fill::color(p.text_scrim(0.1)));
    let halo = s.shadows("window.text_halo");
    assert_eq!(halo.len(), 2);
    assert_eq!((halo[0].blur, halo[0].color[3]), (3.0, 0.9));
    assert_eq!((halo[1].blur, halo[1].color[3]), (10.0, 0.75));
    assert_eq!(s.shadows("display.halo"), halo);
    // The keys stay opaque surfaces.
    assert_eq!(
        s.control("keys.number").unwrap().background,
        Fill::color(p.surface)
    );
}

/// Tokens come first: an element's base follows the theme's tokens, and a
/// key class follows `keys`' shared values, unless it says otherwise.
#[test]
fn elements_derive_from_the_merged_tokens() {
    let c = ctx(true, 1.0);
    let r = resolve(
        &c,
        &value(
            "return {
                colors = { accent = '#ff0000', on_accent = '#000000', surface = '#102030' },
                shape = { radius = 3 },
                text = { key = { size = 30 } },
                fonts = { ui = 'Fira Sans' },
                keys = {
                    radius = 14,
                    hover = { brightness = 1.1 },
                    equals = { hover = { scale = 0.9 }, radius = 20 },
                    number = { font = { weight = 700 } },
                },
            }",
        ),
    );
    assert!(r.notes.is_empty(), "{:?}", r.notes);
    let s = &r.style;
    let red = [1.0, 0.0, 0.0, 1.0];
    assert_eq!(s.tokens.colors.accent, red);
    let eq = s.control("keys.equals").unwrap();
    assert_eq!(eq.background, Fill::color(red));
    assert_eq!(eq.color, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        s.control("keys.func").unwrap().checked.background,
        Some(Fill::color(red))
    );
    assert_eq!(
        s.control("segmented.item").unwrap().radius,
        Radius::all(3.0)
    );
    assert_eq!(
        s.control("segmented.item").unwrap().font.family,
        ["Fira Sans"]
    );
    // `keys.radius` wins over `shape.radius` for every class...
    assert_eq!(s.control("keys.func").unwrap().radius, Radius::all(14.0));
    // ...and a class's own over it.
    assert_eq!(eq.radius, Radius::all(20.0));
    // A state a theme gives replaces the base state whole: no overlay is
    // left. The class's own replaces the shared one.
    let hover = State {
        brightness: Some(1.1),
        ..State::default()
    };
    assert_eq!(s.control("keys.func").unwrap().hover, hover);
    assert_eq!(eq.hover.scale, Some(0.9));
    assert_eq!(eq.hover.brightness, None);
    // Fonts merge field by field.
    let number = s.control("keys.number").unwrap();
    assert_eq!((number.font.size, number.font.weight), (32.0, 700.0));
    assert_eq!(number.font.family, ["Fira Sans"]);
    assert_eq!(number.background, Fill::color(parse("#102030")));
}

fn parse(hex: &str) -> Rgba {
    gmnb_theme::check::parse_hex(hex).unwrap()
}

// ---- 4: unknown keys and wrong types ---------------------------------------------

#[test]
fn unknown_keys_and_wrong_types_warn_and_keep_the_base() {
    let c = ctx(false, 1.0);
    let base = Style::base(&c);
    let r = resolve(
        &c,
        &value(
            "return {
                colours = { accent = '#ff0000' },
                colors = { accent = 'red', accentt = '#ff0000', series = { '#ffffff' } },
                fonts = { ui = { 'Fine', 'Bad\\nName' }, mono = 'x' },
                text = { key = { size = 'big', wieght = 3 } },
                shape = { radius = 'round' },
                window = { color = 12, colour = '#000000', font = { family = '' } },
                keys = {
                    gap = '4',
                    radius = { 1, 2 },
                    number = { background = { 1, 2 }, checked = { color = '#ff0000' }, shadow = 3 },
                    func = { hover = 'bright', font = { features = { 'tabular' } } },
                    operator = 7,
                    nope = {},
                },
                segmented = { item = { transition = { colour = 100 } }, gap = {} },
                panel = { tabs = { stretch = 'yes', item = { min_height = true } } },
                nav = { row = { colour = '#fff' } },
                display = { heights = { standard = 'tall' }, value = { glow = { blur = 'x' } } },
                native = { 'switch', 'sliders' },
                layout = 3,
                motion = { page = '1s' },
            }",
        ),
    );
    let noted: Vec<&str> = r.notes.list.iter().map(|n| n.path.as_str()).collect();
    for path in [
        "colours",
        "colors.accent",
        "colors.accentt",
        "colors.series",
        "fonts.ui[2]",
        "fonts.mono",
        "text.key.size",
        "text.key.wieght",
        "shape.radius",
        "window.color",
        "window.colour",
        "window.font.family[1]",
        "keys.gap",
        "keys.radius",
        "keys.number.background[1]",
        "keys.number.checked",
        "keys.number.shadow",
        "keys.func.hover",
        "keys.func.font.features",
        "keys.operator",
        "keys.nope",
        "segmented.item.transition.colour",
        "segmented.gap",
        "panel.tabs.stretch",
        "panel.tabs.item.min_height",
        "nav.row.colour",
        "display.heights.standard",
        "display.value.glow.blur",
        "native[2]",
        "layout",
        "motion.page",
    ] {
        assert!(noted.contains(&path), "no note for {path}: {noted:#?}");
    }
    assert_eq!(noted.len(), 31, "{:#?}", r.notes.list);
    // Nothing but the one right entry of `native` changed.
    assert_eq!(r.style.native(), ["switch"]);
    let mut expected = base.clone();
    assert_ne!(r.style, expected);
    expected = resolve(&c, &value("return { native = { 'switch' } }")).style;
    assert_eq!(r.style, expected);
}

#[test]
fn numbers_out_of_range_are_clamped_with_a_note() {
    let c = ctx(false, 1.0);
    let r = resolve(
        &c,
        &value(
            "return {
                keys = {
                    gap = -5,
                    number = {
                        opacity = 3,
                        font = { weight = 5000, size = 0 },
                        shadow = { { blur = 500, color = { r = 2, g = 0, b = 0 } } },
                        transition = { background = 99999 },
                    },
                    hover = { fade = -1 },
                },
            }",
        ),
    );
    assert_eq!(r.notes.list.len(), 8, "{:#?}", r.notes.list);
    assert!(
        r.notes
            .list
            .iter()
            .all(|n| n.message.contains("out of range"))
    );
    let s = &r.style;
    assert_eq!(s.number("keys.gap"), 0.0);
    let number = s.control("keys.number").unwrap();
    assert_eq!(number.opacity, 1.0);
    assert_eq!((number.font.weight, number.font.size), (1000.0, 1.0));
    assert_eq!(number.shadow[0].blur, 100.0);
    assert_eq!(number.shadow[0].color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(number.transition.0[0].1, 5000.0);
    assert_eq!(s.control("keys.func").unwrap().hover.fade, Some(0.0));
}

/// Lists, states and transitions are taken whole; `false` clears; a
/// gradient placed in pixels is a draw hook's, not a style's.
#[test]
fn lists_are_taken_whole_and_false_clears() {
    let c = ctx(true, 0.5);
    let r = resolve(
        &c,
        &value(
            "local g = gmnb.linear({ angle = 135, colors = { '#ff7ad9', '#8a63ff' } })
            return {
                window = { text_halo = {}, background = { g, '#00000033' } },
                keys = {
                    equals = { background = g, border = { width = 1, color = '#fff' },
                               shadow = { { y = 8, blur = 24, spread = -8, color = gmnb.alpha('#8a63ff', 0.7) } } },
                    func = { background = gmnb.linear({ from = { 0, 0 }, to = { 9, 9 }, colors = { '#000', '#fff' } }) },
                    operator = { border = false },
                },
            }",
        ),
    );
    let noted: Vec<&str> = r.notes.list.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(noted, ["keys.func.background"], "{:#?}", r.notes.list);
    let s = &r.style;
    // The see-through halo is gone: the theme's list replaced it.
    assert!(s.shadows("window.text_halo").is_empty());
    assert_eq!(s.fill("window.background").0.len(), 2);
    let eq = s.control("keys.equals").unwrap();
    assert!(matches!(eq.background.0.as_slice(), [Paint::Gradient(g)] if g.angle == 135.0));
    assert_eq!(eq.border.unwrap().width, 1.0);
    assert_eq!(eq.shadow.len(), 1);
    assert_eq!(s.control("keys.operator").unwrap().border, None);
    assert_eq!(
        s.control("keys.func").unwrap().background,
        Fill::color(s.tokens.colors.surface2)
    );
}

// ---- 5: the limits of loading and `style` -------------------------------------

#[test]
fn a_theme_that_does_not_load_is_an_error() {
    let script = |r: Result<Theme, LoadError>| match r {
        Err(LoadError::Script(e)) => e,
        Err(other) => panic!("{other}"),
        Ok(_) => panic!("it loaded"),
    };
    assert!(matches!(
        script(load("syntax", "return {")),
        ThemeError::Syntax(_)
    ));
    assert!(matches!(
        script(load("error", "error('no')")),
        ThemeError::Runtime { .. }
    ));
    assert!(matches!(
        script(load("slow", "while true do end")),
        ThemeError::Timeout(_)
    ));
    assert!(matches!(
        script(load(
            "big",
            "local t = {} while true do t[#t + 1] = string.rep('x', 4096) .. #t end"
        )),
        ThemeError::Memory(_)
    ));
    // A string over 4 KiB is refused, not cut.
    assert!(matches!(
        script(load(
            "long",
            "return { api = 1, name = string.rep('x', 5000) }"
        )),
        ThemeError::Limit(_)
    ));
    assert!(matches!(
        load("nothing", "local x = 1"),
        Err(LoadError::NotATable)
    ));
    assert!(matches!(
        load("list", "return { 1, 2 }"),
        Err(LoadError::NotATable)
    ));
    assert!(matches!(
        load("noapi", "return { name = 'x' }"),
        Err(LoadError::NoApi)
    ));
    assert!(matches!(
        load("noname", "return { api = 1 }"),
        Err(LoadError::NoName)
    ));
    match load("newer", "return { api = 2, name = 'Tomorrow' }") {
        Err(e @ LoadError::NewerApi(2)) => assert!(e.to_string().contains("needs a newer GMNB")),
        other => panic!("{:?}", other.err()),
    }
    // Over 512 KiB of source.
    let big = format!(
        "return {{ api = 1, name = 'x' }} --{}",
        "x".repeat(513 << 10)
    );
    assert!(matches!(load("huge", &big), Err(LoadError::Read(_))));
    // Bytecode is refused.
    assert!(load("bytecode", "\u{1b}Lua").is_err());
}

#[test]
fn a_style_that_fails_leaves_the_last_one_standing() {
    let mut theme = load(
        "flaky",
        "return { api = 1, name = 'Flaky',
           options = { { id = 'mode', type = 'choice', default = 'ok',
                         choices = { { id = 'ok' }, { id = 'error' }, { id = 'slow' }, { id = 'big' }, { id = 'long' } } } },
           style = function(ctx)
             local mode = ctx.options.mode
             if mode == 'error' then error('boom') end
             if mode == 'slow' then while true do end end
             if mode == 'long' then return { fonts = { ui = string.rep('x', 5000) } } end
             if mode == 'big' then local t = {} while true do t[#t + 1] = string.rep('y', 4096) .. #t end end
             return { keys = { gap = 9 } }
           end }",
    )
    .unwrap();
    let mut c = ctx(false, 1.0);
    let with = |c: &mut Ctx, mode: &str| {
        c.options = BTreeMap::from([("mode".to_string(), OptionValue::Text(mode.into()))]);
    };
    with(&mut c, "ok");
    assert_eq!(theme.style(&c).unwrap().style.number("keys.gap"), 9.0);
    with(&mut c, "error");
    assert!(matches!(theme.style(&c), Err(ThemeError::Runtime { .. })));
    with(&mut c, "slow");
    assert!(matches!(theme.style(&c), Err(ThemeError::Timeout(_))));
    with(&mut c, "long");
    assert!(matches!(theme.style(&c), Err(ThemeError::Limit(_))));
    // The `ctx` it last worked for still gets that style, without a call.
    with(&mut c, "ok");
    assert_eq!(theme.style(&c).unwrap().style.number("keys.gap"), 9.0);
    // Three failures: `style` is switched off, and says so.
    c.dark = true;
    assert!(matches!(theme.style(&c), Err(ThemeError::Disabled(3))));
    assert!(!theme.is_dead());

    // The memory limit is fatal: the theme is to be unloaded.
    let mut theme = load(
        "greedy",
        "return { api = 1, name = 'Greedy', style = function()
            local t = {} while true do t[#t + 1] = string.rep('y', 4096) .. #t end end }",
    )
    .unwrap();
    assert!(matches!(theme.style(&c), Err(ThemeError::Memory(_))));
    assert!(theme.is_dead());
}

/// `style` runs only when `ctx` changes.
#[test]
fn style_runs_only_when_ctx_changes() {
    let mut theme = load(
        "counting",
        "local calls = 0
         return { api = 1, name = 'Counting', style = function(ctx)
            calls += 1
            return { keys = { gap = calls } }
         end }",
    )
    .unwrap();
    let c = ctx(false, 1.0);
    assert_eq!(theme.style(&c).unwrap().style.number("keys.gap"), 1.0);
    assert_eq!(theme.style(&c).unwrap().style.number("keys.gap"), 1.0);
    assert_eq!(
        theme
            .style(&ctx(true, 1.0))
            .unwrap()
            .style
            .number("keys.gap"),
        2.0
    );
    assert_eq!(
        theme
            .style(&ctx(true, 0.5))
            .unwrap()
            .style
            .number("keys.gap"),
        3.0
    );
    assert_eq!(
        theme
            .style(&ctx(true, 0.5))
            .unwrap()
            .style
            .number("keys.gap"),
        3.0
    );
}

/// `ctx` is exactly the contract's, and read-only.
#[test]
fn ctx_is_what_the_contract_says() {
    let mut theme = load(
        "ctx",
        "return { api = 1, name = 'Ctx',
           options = { { id = 'tint', type = 'color', default = '#102030' },
                       { id = 'on', type = 'toggle', default = true },
                       { id = 'grain', type = 'slider', min = 0, max = 1, step = 0.25, default = 0.5 },
                       { id = 'why', type = 'note', label = 'Because.' } },
           style = function(ctx)
             local keys = {}
             for k in ctx do table.insert(keys, k) end
             table.sort(keys)
             assert(table.concat(keys, ' ') == 'accent base dark font motion opacity options', table.concat(keys, ' '))
             assert(ctx.dark == true and ctx.motion == false)
             assert(math.abs(ctx.opacity - 0.5) < 1e-6)
             assert(math.abs(ctx.accent.r - 1) < 1e-6 and ctx.accent.g == 0 and ctx.accent.a == 1)
             assert(ctx.font.family == 'Cantarell' and ctx.font.size == 14)
             assert(ctx.options.tint == '#102030' and ctx.options.on == false and ctx.options.grain == 0.75)
             assert(ctx.options.why == nil)
             local base = gmnb.base(true, ctx.accent)
             for name, c in base do
               if name ~= 'series' then
                 assert(gmnb.to_hex(c) == gmnb.to_hex(ctx.base.colors[name]), name)
               end
             end
             assert(#ctx.base.colors.series == 6)
             assert(ctx.base.text.key.size == 19 and ctx.base.text.key.weight == 400)
             assert(ctx.base.text.body.family[1] == 'Inter Variable')
             assert(ctx.base.shape.radius == 8 and ctx.base.shape.card_radius == 12)
             assert(ctx.base.text_scrim.a > 0 and gmnb.to_hex(ctx.base.text_scrim) == gmnb.to_hex(base.bg))
             for _, t in { ctx, ctx.options, ctx.base, ctx.base.colors, ctx.base.colors.bg, ctx.base.text.key, ctx.font } do
               assert(not pcall(function() t.x = 1 end))
             end
             return {}
           end }",
    )
    .unwrap();
    assert!(theme.notes.is_empty(), "{:?}", theme.notes);
    let saved = BTreeMap::from([
        ("on".to_string(), OptionValue::Bool(false)),
        // Onto a step; a value that doesn't fit its option is ignored.
        ("grain".to_string(), OptionValue::Number(0.7)),
        ("tint".to_string(), OptionValue::Text("blue".into())),
        ("gone".to_string(), OptionValue::Bool(true)),
    ]);
    let c = Ctx {
        dark: true,
        accent: Some([1.0, 0.0, 0.0]),
        opacity: 0.5,
        options: theme::complete_options(&theme.def.options, &saved),
        motion: false,
        font: ("Cantarell".into(), 14.0),
    };
    assert_eq!(c.options.len(), 3);
    match theme.style(&c) {
        Ok(r) => assert!(r.notes.is_empty()),
        Err(e) => panic!("{e}"),
    }
}

// ---- 7: the built-in themes -------------------------------------------------------

const PALETTES: [&str; 7] = [
    "aurora", "ember", "abyss", "orchard", "graphite", "system", "custom",
];

fn built_in(id: &str) -> Theme {
    let theme = Theme::load(Source::built_in(id).unwrap(), Limits::default())
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    assert!(theme.notes.is_empty(), "{id}: {:#?}", theme.notes);
    theme
}

/// Aurora and Mono load in the real engine, their `style` runs for every
/// combination of options that matters, and every key they return is one
/// the schema knows, of the right type: no notes at all.
#[test]
fn the_built_in_themes_load_and_return_only_known_keys() {
    let mut aurora = built_in("aurora");
    assert_eq!(aurora.def.meta.name, "Aurora");
    assert_eq!(aurora.def.fonts, ["fonts/Outfit-Variable.ttf"]);
    assert_eq!(aurora.credits()[0].0, "Outfit typeface");
    assert!(aurora.credits()[0].1.contains("SIL OPEN FONT LICENSE"));
    assert_eq!(aurora.font_files().len(), 1);
    let ids: Vec<&str> = aurora.def.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "palette",
            "system_note",
            "primary",
            "secondary",
            "living",
            "motion_note"
        ]
    );
    for name in ["background", "keypad", "display"] {
        assert!(aurora.def.layers[name].draw.is_some(), "{name}");
    }
    assert!(aurora.def.layers["keypad"].place.is_some());
    assert!(aurora.def.layers["keypad"].pointer);
    for palette in PALETTES {
        for dark in [false, true] {
            for opacity in [1.0, 0.5, 0.1] {
                for accent in [None, Some([0.2, 0.8, 0.4])] {
                    let saved = BTreeMap::from([(
                        "palette".to_string(),
                        OptionValue::Text(palette.to_string()),
                    )]);
                    let c = Ctx {
                        dark,
                        accent,
                        opacity,
                        options: theme::complete_options(&aurora.def.options, &saved),
                        ..Ctx::default()
                    };
                    let r = aurora
                        .style(&c)
                        .unwrap_or_else(|e| panic!("{palette} {dark} {opacity}: {e}"));
                    assert!(
                        r.notes.is_empty(),
                        "{palette} {dark} {opacity}: {:#?}",
                        r.notes
                    );
                    let s = &r.style;
                    assert_eq!(s.radius("keys.radius"), Radius::all(14.0));
                    assert_eq!(s.number("keys.gap"), 5.0);
                    assert!(!s.flag("panel.tabs.stretch"));
                    assert!(s.native().contains(&"segmented"));
                    let eq = s.control("keys.equals").unwrap();
                    assert!(matches!(eq.background.0.first(), Some(Paint::Gradient(_))));
                    assert_eq!(eq.font.family, ["Outfit"]);
                    assert_eq!(eq.hover.brightness, Some(1.12));
                    assert_eq!(eq.hover.overlay, None);
                }
            }
        }
    }
    assert!(aurora.take_log().is_empty());
    // The options' own functions.
    let c = |palette: &str| Ctx {
        options: theme::complete_options(
            &aurora.def.options,
            &BTreeMap::from([("palette".to_string(), OptionValue::Text(palette.into()))]),
        ),
        ..Ctx::default()
    };
    let option = |id: &str| aurora.def.options.iter().find(|o| o.id == id).unwrap();
    assert!(!aurora.visible(option("primary"), &c("aurora")));
    assert!(aurora.visible(option("primary"), &c("custom")));
    assert!(aurora.visible(option("system_note"), &c("system")));
    assert!(aurora.visible(option("living"), &c("aurora")));
    let action = &option("secondary").actions[0];
    assert_eq!(action.label, "Complement");
    let changed = aurora.run_action(action, &c("custom")).unwrap();
    assert_eq!(
        changed,
        BTreeMap::from([("secondary".to_string(), OptionValue::Text("#d0ff4d".into()))])
    );

    let mut mono = built_in("mono");
    assert_eq!(mono.def.meta.name, "Mono");
    for dark in [false, true] {
        for opacity in [1.0, 0.5, 0.1] {
            for square in [true, false] {
                let saved = BTreeMap::from([("square".to_string(), OptionValue::Bool(square))]);
                let c = Ctx {
                    dark,
                    opacity,
                    options: theme::complete_options(&mono.def.options, &saved),
                    ..Ctx::default()
                };
                let r = mono.style(&c).unwrap();
                assert!(r.notes.is_empty(), "{:#?}", r.notes);
                let s = &r.style;
                let corner = if square { 2.0 } else { 8.0 };
                assert_eq!(s.radius("keys.radius"), Radius::all(corner));
                assert_eq!(s.number("keys.gap"), 2.0);
                assert_eq!(s.control("keys.number").unwrap().font.family, ["monospace"]);
                // The accent is the option's, worked out as the base's is.
                let p = appcore::palette::Palette::new(dark, Some(appcore::color::hex(0xe5484d)));
                assert_eq!(s.tokens.colors.accent, p.accent);
                assert_eq!(
                    s.control("keys.equals").unwrap().background,
                    Fill::color(p.accent)
                );
            }
        }
    }
}

/// The themes Settings lists, and "your copy".
#[test]
fn discovery_lists_built_ins_then_the_users() {
    let root = std::env::temp_dir().join(format!("gmnb-themes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let write = |id: &str, text: &str| {
        std::fs::create_dir_all(root.join(id)).unwrap();
        std::fs::write(root.join(id).join("theme.luau"), text).unwrap();
    };
    write(
        "zebra",
        "return { api = 1, name = 'Zebra', description = 'Stripes.' }",
    );
    write("mono", "return { api = 1, name = 'My Mono' }");
    write("broken", "return {");
    write("Bad Name", "return { api = 1, name = 'x' }");
    write("base", "return { api = 1, name = 'x' }");
    std::fs::create_dir_all(root.join("empty")).unwrap();
    let dev = root.join("zebra");
    let sources = theme::discover(Some(&root), Some(&dev));
    let ids: Vec<(&str, theme::Origin)> =
        sources.iter().map(|s| (s.id.as_str(), s.origin)).collect();
    use theme::Origin::*;
    assert_eq!(
        ids,
        [
            ("aurora", BuiltIn),
            ("mono", UserCopy),
            ("glazed", BuiltIn),
            ("broken", User),
            ("zebra", User),
            ("dev", Dev)
        ]
    );
    let listed = theme::list(&sources, &Limits::default());
    assert_eq!(listed[0].meta.as_ref().unwrap().name, "Aurora");
    assert_eq!(listed[1].meta.as_ref().unwrap().name, "My Mono");
    assert_eq!(listed[2].meta.as_ref().unwrap().name, "Glazed");
    assert!(
        listed[3]
            .meta
            .as_ref()
            .unwrap_err()
            .contains("syntax error")
    );
    assert_eq!(
        listed[4].meta.as_ref().unwrap().description.as_deref(),
        Some("Stripes.")
    );
    assert_eq!(listed[5].meta.as_ref().unwrap().name, "Zebra");
    assert_eq!(theme::discover(None, None).len(), 3);
    let _ = std::fs::remove_dir_all(&root);
}

/// Files a theme names stay inside its folder.
#[test]
fn theme_files_stay_inside_the_folder() {
    let root = std::env::temp_dir().join(format!("gmnb-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("theme");
    std::fs::create_dir_all(dir.join("fonts")).unwrap();
    std::fs::write(root.join("secret.txt"), "secret").unwrap();
    std::fs::write(dir.join("fonts/a.ttf"), "font").unwrap();
    std::fs::write(dir.join("notes.txt"), "thanks").unwrap();
    std::os::unix::fs::symlink(root.join("secret.txt"), dir.join("fonts/link.ttf")).unwrap();
    std::os::unix::fs::symlink(dir.join("fonts/a.ttf"), dir.join("fonts/inside.ttf")).unwrap();
    std::fs::write(
        dir.join("theme.luau"),
        "return { api = 1, name = 'Files',
           fonts = { 'fonts/a.ttf', '../secret.txt', '/etc/passwd', 'fonts/link.ttf',
                     'fonts/inside.ttf', 'fonts/missing.ttf', 'fonts/../../secret.txt', 7 },
           credits = { { title = 'Thanks', file = 'notes.txt' },
                       { title = 'No', file = '../secret.txt' },
                       { title = 'Gone', file = 'gone.txt' } } }",
    )
    .unwrap();
    let theme = Theme::load(Source::folder("files", &dir), Limits::default()).unwrap();
    assert_eq!(theme.def.fonts, ["fonts/a.ttf", "fonts/inside.ttf"]);
    assert_eq!(theme.font_files().len(), 2);
    assert_eq!(
        theme.credits(),
        [("Thanks".to_string(), "thanks".to_string())]
    );
    let noted: Vec<&str> = theme.notes.list.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(
        noted,
        [
            "fonts[2]",
            "fonts[3]",
            "fonts[7]",
            "fonts[8]",
            "credits[2].",
            "fonts/link.ttf",
            "fonts/missing.ttf",
            "gone.txt"
        ],
        "{:#?}",
        theme.notes.list
    );
    for bad in [
        "", ".", "..", "a/../b", "/a", "a//b", "a\\b", "a/./b", "a\nb",
    ] {
        assert!(!theme::safe_relative(bad), "{bad:?}");
    }
    assert!(theme::safe_relative("fonts/My Font.ttf"));
    for (id, ok) in [
        ("mono", true),
        ("my-theme_2", true),
        ("2cool", true),
        ("", false),
        ("Mono", false),
        ("-x", false),
        ("a b", false),
        ("base", false),
        ("x".repeat(33).as_str(), false),
    ] {
        assert_eq!(theme::valid_id(id), ok, "{id:?}");
    }
    let _ = std::fs::remove_dir_all(&root);
}
