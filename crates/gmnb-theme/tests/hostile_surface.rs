//! Round 3, area E: the API surface the first round didn't see, handed to
//! hostile themes: ramps churned, `fade` given anything, `visible` and
//! actions that fail, run long or return a table of a million entries,
//! colour and slider options given any value.

use std::time::{Duration, Instant};

use gmnb_theme::layer::{Frame, LayerKind};
use gmnb_theme::theme::{OptionKind, Source, Theme};
use gmnb_theme::tokens::OptionValue;
use gmnb_theme::{Ctx, Limits};

fn load_source(name: &str, source: &str) -> Theme {
    let dir = std::env::temp_dir().join(format!("gmnb-hs-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("theme.luau"), source).unwrap();
    let t =
        Theme::load(Source::folder("t", &dir), Limits::default()).unwrap_or_else(|e| panic!("{e}"));
    let _ = std::fs::remove_dir_all(&dir);
    t
}

fn frame(theme: &Theme) -> (Duration, String) {
    let t = Instant::now();
    let r = theme.draw(
        LayerKind::Background,
        &Ctx::default(),
        &Frame::sized(400.0, 300.0),
    );
    (
        t.elapsed(),
        match r {
            Ok(d) => format!("drawn {}", d.list.ops.len()),
            Err(e) => format!("{e:?}"),
        },
    )
}

/// A theme that makes ramps without end, and draws with ones the app let
/// go of long ago (each is made again from its own table): a frame that
/// is cut off by its limit, not one that grows or panics, however long it
/// goes on.
#[test]
fn ramps_made_by_the_thousand_and_drawn_again_are_bounded() {
    let theme = load_source(
        "ramps",
        r##"
        local kept = {}
        local buf = buffer.create(2 * 2 * 4)
        return { api = 1, name = 'T', layers = { background = { draw = function(draw, frame)
            for i = 1, 3000 do
                local r = gmnb.ramp({ colors = { gmnb.rgb((i % 97) / 97, 0, 0), "#0000ff", gmnb.rgb(0, (i % 89) / 89, 0) }, edge = { 0.1, 0.9 }, cycle = (i % 2) == 0 })
                kept[#kept + 1] = r
                if #kept > 100 then table.remove(kept, 1) end
            end
            for i = 1, #kept do
                draw:field(0, 0, 40, 40, { data = buf, cols = 2, rows = 2, channels = 1, ramp = kept[i] })
            end
        end } } }
        "##,
    );
    for _ in 0..50 {
        let (took, said) = frame(&theme);
        assert!(took < Duration::from_millis(200), "{took:?} {said}");
    }
}

/// `fade` is a boolean on the background layer and nothing else: any other
/// value, and `fade` on another layer, is a note and the layer is faded.
#[test]
fn fade_given_anything_is_a_note_not_a_failure() {
    for (value, layer) in [
        ("'no'", "background"),
        ("0", "background"),
        ("{}", "background"),
        ("function() end", "background"),
        ("false", "overlay"),
        ("false", "keys"),
        ("false", "display"),
    ] {
        let theme = load_source(
            "fade",
            &format!(
                "return {{ api = 1, name = 'T', layers = {{ {layer} = {{ fade = {value}, draw = function(d, f) end }} }} }}"
            ),
        );
        assert!(
            !theme.notes.list.is_empty(),
            "{layer} fade = {value}: no note"
        );
        let bg = theme.layer(LayerKind::Background);
        assert!(bg.is_none_or(|l| !l.unfaded), "{layer} fade = {value}");
    }
    let theme = load_source(
        "fade-ok",
        "return { api = 1, name = 'T', layers = { background = { fade = false, draw = function(d, f) end } } }",
    );
    assert!(theme.layer(LayerKind::Background).unwrap().unfaded);
    assert!(theme.notes.list.is_empty());
}

fn options_theme() -> Theme {
    load_source(
        "options",
        r##"
        local big = function(n)
            local t = {}
            for i = 1, n do t["k" .. i] = i end
            return t
        end
        local opts = {
            { id = "a", type = "toggle", label = "A", default = true,
              visible = function() return {} end },
            { id = "b", type = "toggle", label = "B", default = true,
              visible = function() error("no") end },
            { id = "c", type = "toggle", label = "C", default = true,
              visible = function() while true do end end },
            { id = "d", type = "color", label = "D", default = "#123456",
              actions = {
                { label = "huge", run = function() return big(200000) end },
                { label = "deep", run = function() local t = {} local c = t for i = 1, 100000 do c.x = {} c = c.x end return t end },
                { label = "loop", run = function() while true do end end },
                { label = "weird", run = function() return { d = "#12345g", a = 5, [1] = 2, [{}] = 3 } end },
                { label = "long", run = function() return { d = string.rep("#", 5000) } end },
              } },
            { id = "s", type = "slider", label = "S", min = 0, max = 1, step = 0.1, default = 0.5 },
        }
        return { api = 1, name = 'T', options = opts }
        "##,
    )
}

/// `visible` that returns a table, errors or never returns: the option is
/// shown (only `false` hides it), each within its own limit.
#[test]
fn visible_that_fails_shows_the_option() {
    let theme = options_theme();
    let ctx = Ctx::default();
    for id in ["a", "b", "c"] {
        let option = theme.def.options.iter().find(|o| o.id == id).unwrap();
        let t = Instant::now();
        assert!(theme.visible(option, &ctx), "{id}");
        assert!(
            t.elapsed() < Duration::from_millis(100),
            "{id}: {:?}",
            t.elapsed()
        );
    }
}

/// An action that returns a table of 200 000 entries, one nested 100 000
/// deep, one that never returns, or values of the wrong kind: each is
/// bounded in time, and what doesn't fit its option is left out.
#[test]
fn actions_that_return_anything_are_bounded() {
    let theme = options_theme();
    let ctx = Ctx::default();
    let option = theme.def.options.iter().find(|o| o.id == "d").unwrap();
    for action in &option.actions {
        let t = Instant::now();
        let r = theme.run_action(action, &ctx);
        let took = t.elapsed();
        assert!(
            took < Duration::from_millis(200),
            "{}: {took:?}",
            action.label
        );
        if let Ok(changed) = r {
            for (id, v) in changed {
                let def = theme.def.options.iter().find(|o| o.id == id).unwrap();
                assert!(def.accept(&v).is_some(), "{}: {id} {v:?}", action.label);
            }
        }
    }
}

/// A colour or slider option given anything (a saved file's, an action's).
#[test]
fn option_values_of_any_shape_are_taken_or_refused() {
    let theme = options_theme();
    let colour = theme.def.options.iter().find(|o| o.id == "d").unwrap();
    assert!(matches!(colour.kind, OptionKind::Color));
    for bad in [
        "",
        "#",
        "#12345",
        "#1234567",
        "#12345g",
        "123456",
        "#+12345",
        "# 12345",
        "#１２３４５６",
        "#12345\0",
        "rgb(1,2,3)",
    ] {
        assert!(
            colour.accept(&OptionValue::Text(bad.into())).is_none(),
            "{bad:?}"
        );
    }
    assert_eq!(
        colour.accept(&OptionValue::Text("#ABCDEF".into())),
        Some(OptionValue::Text("#abcdef".into()))
    );
    let slider = theme.def.options.iter().find(|o| o.id == "s").unwrap();
    for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(slider.accept(&OptionValue::Number(n)).is_none());
    }
    for n in [-1e300, 1e300, 0.04999999, 0.05, 0.95000001, 1e-300] {
        let Some(OptionValue::Number(got)) = slider.accept(&OptionValue::Number(n)) else {
            panic!("{n}")
        };
        assert!((0.0..=1.0).contains(&got), "{n} -> {got}");
    }
}

/// A theme folder whose files are links out of it, pipes, folders, and
/// names with `..`: loads at once, reads none of them, and an ordinary
/// file beside them is read.
#[test]
fn a_folders_files_that_are_not_its_own_are_not_read() {
    use std::os::unix::fs::symlink;
    let dir = std::env::temp_dir().join(format!("gmnb-hs-{}-files", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("dir.txt")).unwrap();
    std::fs::write(dir.join("ok.txt"), "licence").unwrap();
    symlink("/etc/passwd", dir.join("link.txt")).unwrap();
    symlink("/dev/zero", dir.join("zero.png")).unwrap();
    symlink("..", dir.join("up")).unwrap();
    let fifo = std::process::Command::new("mkfifo")
        .arg(dir.join("fifo.txt"))
        .status();
    assert!(fifo.is_ok_and(|s| s.success()));
    std::fs::write(
        dir.join("theme.luau"),
        r##"
        local ok, e = pcall(gmnb.image, "zero.png")
        local ok2, e2 = pcall(gmnb.image, "fifo.txt")
        local ok3, e3 = pcall(gmnb.animation, "up/etc/passwd")
        return { api = 1, name = 'T',
          fonts = { "link.txt", "fifo.txt", "dir.txt", "../x", "up/etc/passwd", "ok.txt" },
          credits = { { title = "link", file = "link.txt" }, { title = "fifo", file = "fifo.txt" },
                      { title = "dir", file = "dir.txt" }, { title = "up", file = "up/etc/passwd" },
                      { title = "dots", file = "../x" }, { title = "ok", file = "ok.txt" } } }
        "##,
    )
    .unwrap();
    let t = Instant::now();
    let theme = Theme::load(Source::folder("t", &dir), Limits::default()).unwrap();
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    let credits = theme.credits();
    assert_eq!(credits, vec![("ok".to_string(), "licence".to_string())]);
    assert_eq!(theme.font_files().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
