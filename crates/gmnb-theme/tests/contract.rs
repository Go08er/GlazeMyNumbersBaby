//! The contract against the code: what `types/gmnb.d.luau` and the docs
//! promise to theme authors, checked on the running engine.

use gmnb_theme::theme::{Source, Theme};
use gmnb_theme::{Ctx, Limits, Style};

struct Dir(std::path::PathBuf);
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(name: &str) -> Dir {
    let dir = std::env::temp_dir().join(format!("gmnb-contract-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Dir(dir)
}

fn load(name: &str, source: &str) -> (Theme, Dir) {
    let d = dir(name);
    std::fs::write(d.0.join("theme.luau"), source).unwrap();
    let t = Theme::load(Source::folder("t", &d.0), Limits::default()).unwrap();
    (t, d)
}

fn declarations() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../types/gmnb.d.luau"
    ))
    .unwrap()
}

/// The field names of `export type <name> = { ... }`, one per line.
fn fields_of(d: &str, name: &str) -> Vec<String> {
    let start = d
        .find(&format!("export type {name} = {{"))
        .unwrap_or_else(|| panic!("no type {name}"));
    let body = &d[start..];
    let end = body.find("\n}\n").expect("the type ends");
    body[..end]
        .lines()
        .skip(1)
        .filter(|l| {
            l.starts_with('\t') && !l.starts_with("\t\t") && !l.trim_start().starts_with("--")
        })
        .filter_map(|l| l.trim_start().split(':').next().map(str::to_string))
        .collect()
}

/// Every name `gmnb` has at run time is declared, and every declared name
/// is there.
#[test]
fn the_gmnb_library_is_what_the_declarations_say() {
    let (t, _d) = load(
        "lib",
        r#"
local keys = {}
for k in pairs(gmnb) do table.insert(keys, k) end
table.sort(keys)
print(table.concat(keys, " "))
return { api = 1, name = "T" }
"#,
    );
    let log = t.take_log();
    let mut running: Vec<&str> = log[0].split(' ').collect();
    running.sort_unstable();
    let d = declarations();
    let mut declared: Vec<String> = fields_of(&d, "Gmnb");
    declared.sort();
    let declared: Vec<&str> = declared.iter().map(String::as_str).collect();
    assert_eq!(running, declared);
}

/// Every call of the draw API in the declarations exists, and nothing
/// else does.
#[test]
fn the_draw_api_is_what_the_declarations_say() {
    let d = declarations();
    let mut declared = fields_of(&d, "Draw");
    declared.sort();
    let (t, _d) = load(
        "draw",
        &format!(
            r#"
return {{ api = 1, name = "T", layers = {{ background = {{ draw = function(draw, frame)
  local missing = {{}}
  for _, n in ipairs({{ {} }}) do
    local ok, v = pcall(function() return draw[n] end)
    if not ok or v == nil then table.insert(missing, n) end
  end
  print("missing: " .. table.concat(missing, " "))
end }} }} }}
"#,
            declared
                .iter()
                .map(|n| format!("{n:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    t.draw(
        gmnb_theme::layer::LayerKind::Background,
        &Ctx::default(),
        &gmnb_theme::layer::Frame::sized(10.0, 10.0),
    )
    .unwrap();
    assert_eq!(t.take_log(), ["missing: "]);
}

/// Every style path the engine accepts can be written by an author who
/// follows `gmnb.d.luau`.
#[test]
fn every_style_key_is_declared() {
    let d = declarations();
    let mut missing = Vec::new();
    for (path, _) in Style::base(&Ctx::default()).values() {
        let last = path.rsplit('.').next().unwrap();
        let declared = d.match_indices(last).any(|(at, _)| {
            let before = d[..at].chars().next_back();
            let after = d[at + last.len()..].chars().next();
            matches!(before, Some('\t' | ' ' | '{' | ',')) && matches!(after, Some(':') | Some('?'))
        });
        if !declared {
            missing.push(path);
        }
    }
    assert!(missing.is_empty(), "not in gmnb.d.luau: {missing:?}");
}

fn app_sources() -> String {
    fn walk(dir: &std::path::Path, out: &mut String) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push_str(&std::fs::read_to_string(&path).unwrap());
            }
        }
    }
    let mut out = String::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/gmnb/src"),
        &mut out,
    );
    out
}

/// The contract lists every `motion.*` and `layout.*` token as something
/// the app does with a number (docs/theming.md, "Reference: tokens"): the
/// app reads each one. (0.3 left `motion.page`, `motion.mode`,
/// `motion.flip` and `layout.graph.pad_height` to constants.)
#[test]
fn every_motion_and_layout_token_is_read_by_the_app() {
    let src = app_sources();
    let mut unread = Vec::new();
    for (path, _) in Style::base(&Ctx::default()).values() {
        if (path.starts_with("motion.") || path.starts_with("layout."))
            && !src.contains(&format!("\"{path}\""))
        {
            unread.push(path);
        }
    }
    assert!(unread.is_empty(), "the app never reads {unread:?}");
}

/// `visible` hides an option only by returning `false`: an error, a
/// timeout or any other answer leaves it shown, so that a slip in a theme
/// never takes a setting away (docs/theming.md, "Options").
#[test]
fn an_option_is_hidden_only_by_visible_returning_false() {
    let (t, _d) = load(
        "visible",
        r#"
return { api = 1, name = "T", options = {
  { id = "no", type = "toggle", visible = function() return false end },
  { id = "yes", type = "toggle", visible = function() return true end },
  { id = "nothing", type = "toggle", visible = function() end },
  { id = "broken", type = "toggle", visible = function() error("no") end },
  { id = "text", type = "toggle", visible = function() return "false" end },
  { id = "plain", type = "toggle" },
} }
"#,
    );
    let shown: Vec<bool> = t
        .def
        .options
        .iter()
        .map(|o| t.visible(o, &Ctx::default()))
        .collect();
    assert_eq!(shown, [false, true, true, true, true, true]);
}

/// A background layer may thin itself on a see-through window instead
/// of being faded by the app: `fade = false` (docs/theming.md,
/// "See-through windows"). The other layers are never faded, so on them
/// the key says nothing, and that is noted. The built-in themes say so;
/// a theme that doesn't is faded as ever.
#[test]
fn a_background_layer_may_thin_itself() {
    let (t, _d) = load(
        "fade",
        r#"
return { api = 1, name = "T", layers = {
  background = { draw = function() end, fade = false },
  overlay = { draw = function() end, fade = false },
  display = { draw = function() end, fade = true },
  keypad = { draw = function() end, fade = "no" },
} }
"#,
    );
    let notes: Vec<String> = t.notes.list.iter().map(|n| n.to_string()).collect();
    assert_eq!(notes.len(), 2, "{notes:#?}");
    assert!(
        notes
            .iter()
            .any(|n| n.contains("layers.overlay.fade") && n.contains("only the background")),
        "{notes:#?}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("layers.keypad.fade") && n.contains("expected true or false")),
        "{notes:#?}"
    );
    assert!(t.def.layers["background"].unfaded);
    assert!(!t.def.layers["display"].unfaded);
    assert!(!t.def.layers["keypad"].unfaded);
    let built_in =
        |id: &str| Theme::load(Source::built_in(id).unwrap(), Limits::default()).unwrap();
    for id in ["aurora", "sprinkles", "swirls"] {
        assert!(built_in(id).def.layers["background"].unfaded, "{id}");
    }
}

/// A layer's keys are checked like every other key (docs/theming.md,
/// "Limits, and what happens when a theme fails": a value of the wrong
/// type, or a number out of range, is noted).
#[test]
fn a_layers_odd_keys_are_noted() {
    let (t, _d) = load(
        "layer",
        r#"
return { api = 1, name = "T", layers = {
  background = { events = { press = 99, show = 2 }, pointer = "yes", place = function() end },
  display = { events = 5 },
  keypad = { events = { press = 5 }, place = function() end },
} }
"#,
    );
    let notes: Vec<String> = t.notes.list.iter().map(|n| n.to_string()).collect();
    let has = |path: &str, part: &str| {
        assert!(
            notes.iter().any(|n| n.contains(path) && n.contains(part)),
            "{path} / {part} in {notes:#?}"
        );
    };
    has("layers.background.events.press", "using 5");
    has("layers.background.pointer", "expected true or false");
    has("layers.background.place", "keypad");
    has("layers.display.events", "expected a table");
    // What is fine is not noted; the cap is applied.
    assert_eq!(notes.len(), 4, "{notes:#?}");
    assert_eq!(t.def.layers["background"].events["press"], 5.0);
    assert_eq!(t.def.layers["background"].events["show"], 2.0);
}
