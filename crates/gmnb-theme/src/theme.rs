//! Themes: where they are found, what `theme.luau` returns, and running
//! `style` (docs/theming.md, "Files and where they go", "When a theme
//! runs", "Options").

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use std::cell::RefCell;

use mlua::{Lua, Table, Value};

use crate::check::{Note, Notes, parse_hex};
use crate::draw::{self, DisplayList};
use crate::layer::{self, Drawn, Failure, Frame, Frames, LayerKind};
use crate::library::{self, API};
use crate::picture::Pictures;
use crate::style::{Resolved, resolve};
use crate::tokens::{Colors, Ctx, OptionValue, Tokens};
use crate::{Budget, Engine, FunctionHandle, HostTable, Limits, ThemeError, ThemeValue};

// ---- Where themes are ------------------------------------------------------

/// A theme compiled into the app.
pub struct BuiltIn {
    pub id: &'static str,
    pub source: &'static str,
    /// The other files of its folder, by their path in it.
    pub files: &'static [(&'static str, &'static [u8])],
}

/// The themes built in, from `apps/gmnb/themes/`.
pub static BUILT_IN: [BuiltIn; 4] = [
    BuiltIn {
        id: "aurora",
        source: include_str!("../../../apps/gmnb/themes/aurora/theme.luau"),
        files: &[
            (
                "fonts/Outfit-Variable.ttf",
                include_bytes!("../../../apps/gmnb/themes/aurora/fonts/Outfit-Variable.ttf"),
            ),
            (
                "fonts/OFL-Outfit.txt",
                include_bytes!("../../../apps/gmnb/themes/aurora/fonts/OFL-Outfit.txt"),
            ),
        ],
    },
    BuiltIn {
        id: "mono",
        source: include_str!("../../../apps/gmnb/themes/mono/theme.luau"),
        files: &[],
    },
    BuiltIn {
        id: "glazed",
        source: include_str!("../../../apps/gmnb/themes/glazed/theme.luau"),
        files: &[
            (
                "fonts/Jersey10-Regular.ttf",
                include_bytes!("../../../apps/gmnb/themes/glazed/fonts/Jersey10-Regular.ttf"),
            ),
            (
                "fonts/OFL-Jersey10.txt",
                include_bytes!("../../../apps/gmnb/themes/glazed/fonts/OFL-Jersey10.txt"),
            ),
            (
                "images/wall-light.png",
                include_bytes!("../../../apps/gmnb/themes/glazed/images/wall-light.png"),
            ),
            (
                "images/wall-dark.png",
                include_bytes!("../../../apps/gmnb/themes/glazed/images/wall-dark.png"),
            ),
            (
                "images/scallop-odd.png",
                include_bytes!("../../../apps/gmnb/themes/glazed/images/scallop-odd.png"),
            ),
            (
                "images/scallop-even.png",
                include_bytes!("../../../apps/gmnb/themes/glazed/images/scallop-even.png"),
            ),
            (
                "images/scallop-line.png",
                include_bytes!("../../../apps/gmnb/themes/glazed/images/scallop-line.png"),
            ),
            (
                "images/sprinkles.gif",
                include_bytes!("../../../apps/gmnb/themes/glazed/images/sprinkles.gif"),
            ),
        ],
    },
    BuiltIn {
        id: "swirls",
        source: include_str!("../../../apps/gmnb/themes/swirls/theme.luau"),
        files: &[(
            "credits/ColorMix.txt",
            include_bytes!("../../../apps/gmnb/themes/swirls/credits/ColorMix.txt"),
        )],
    },
];

/// The id of the theme loaded from `GMNB_THEME_DIR`.
pub const DEV_ID: &str = "dev";
/// The name that stands for the base look where a theme id is given
/// (`GMNB_THEME=base`); no theme can have it.
pub const BASE_ID: &str = "base";

pub const THEME_FILE: &str = "theme.luau";
pub const MAX_FONTS: usize = 8;
pub const MAX_FONT_BYTES: u64 = 16 << 20;
pub const MAX_FONTS_BYTES: u64 = 32 << 20;
pub const MAX_CREDIT_BYTES: u64 = 64 << 10;

/// Where a theme comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    BuiltIn,
    /// A folder of the user's.
    User,
    /// A folder of the user's with a built-in theme's id: it replaces it
    /// ("your copy").
    UserCopy,
    /// `GMNB_THEME_DIR`.
    Dev,
}

/// A theme that can be loaded: its id and where its files are.
#[derive(Clone, Debug)]
pub struct Source {
    pub id: String,
    pub origin: Origin,
    dir: Option<PathBuf>,
}

/// A folder name that is a theme id: lower-case letters, digits, `-` and
/// `_`, at most 32 characters, starting with a letter or digit.
pub fn valid_id(id: &str) -> bool {
    let ok = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    !id.is_empty()
        && id.len() <= 32
        && id != BASE_ID
        && ok(id.as_bytes()[0])
        && id.bytes().all(|b| ok(b) || b == b'-' || b == b'_')
}

/// A path a theme may name inside its own folder: relative, with no `..`
/// (nor `.`, an empty part, a backslash or a control character).
pub fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 255
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// The themes installed: the built-in ones first (each replaced by the
/// user's folder of the same id, if there is one), then the user's by id,
/// then `dev_dir`'s (`GMNB_THEME_DIR`). `user_dir` is
/// `$XDG_DATA_HOME/gmnb/themes`.
pub fn discover(user_dir: Option<&Path>, dev_dir: Option<&Path>) -> Vec<Source> {
    let mut user: Vec<(String, PathBuf)> = Vec::new();
    if let Some(entries) = user_dir.and_then(|d| std::fs::read_dir(d).ok()) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(id) = name.to_str().filter(|n| valid_id(n) && *n != DEV_ID) else {
                continue;
            };
            let dir = entry.path();
            if dir.join(THEME_FILE).is_file() {
                user.push((id.to_string(), dir));
            }
        }
    }
    user.sort();
    let mut out = Vec::new();
    for b in &BUILT_IN {
        let copy = user.iter().position(|(id, _)| id == b.id);
        out.push(match copy {
            Some(at) => Source {
                id: b.id.to_string(),
                origin: Origin::UserCopy,
                dir: Some(user.remove(at).1),
            },
            None => Source {
                id: b.id.to_string(),
                origin: Origin::BuiltIn,
                dir: None,
            },
        });
    }
    out.extend(user.into_iter().map(|(id, dir)| Source {
        id,
        origin: Origin::User,
        dir: Some(dir),
    }));
    if let Some(dir) = dev_dir {
        out.push(Source {
            id: DEV_ID.to_string(),
            origin: Origin::Dev,
            dir: Some(dir.to_path_buf()),
        });
    }
    out
}

impl Source {
    /// A built-in theme, whatever the user has installed (for tests).
    pub fn built_in(id: &str) -> Option<Source> {
        BUILT_IN.iter().find(|b| b.id == id).map(|b| Source {
            id: b.id.to_string(),
            origin: Origin::BuiltIn,
            dir: None,
        })
    }

    /// A theme in a folder, under `id`.
    pub fn folder(id: &str, dir: &Path) -> Source {
        Source {
            id: id.to_string(),
            origin: Origin::Dev,
            dir: Some(dir.to_path_buf()),
        }
    }

    fn built(&self) -> Option<&'static BuiltIn> {
        match self.dir {
            None => BUILT_IN.iter().find(|b| b.id == self.id),
            Some(_) => None,
        }
    }

    /// The theme's folder, for a theme that has one on disk.
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// The real path of `rel` inside the theme's folder: refused if it
    /// isn't [`safe_relative`], isn't a file, or leads outside the folder
    /// (through a link).
    pub fn path(&self, rel: &str) -> Result<PathBuf, String> {
        if !safe_relative(rel) {
            return Err(format!("{rel:?} is not a path inside the theme's folder"));
        }
        let Some(dir) = &self.dir else {
            return Err("a built-in theme has no folder".into());
        };
        let dir = std::fs::canonicalize(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = std::fs::canonicalize(dir.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
        if !path.starts_with(&dir) {
            return Err(format!("{rel} leads outside the theme's folder"));
        }
        if !path.is_file() {
            return Err(format!("{rel} is not a file"));
        }
        Ok(path)
    }

    /// How big `rel` is, in bytes.
    pub fn size(&self, rel: &str) -> Result<u64, String> {
        match self.built() {
            Some(b) => b
                .files
                .iter()
                .find(|(name, _)| *name == rel)
                .map(|(_, bytes)| bytes.len() as u64)
                .ok_or_else(|| format!("{rel}: no such file")),
            None => {
                let path = self.path(rel)?;
                std::fs::metadata(&path)
                    .map(|m| m.len())
                    .map_err(|e| format!("{rel}: {e}"))
            }
        }
    }

    /// The bytes of `rel`, a file of the theme's folder of at most `max`
    /// bytes.
    pub fn read(&self, rel: &str, max: u64) -> Result<Vec<u8>, String> {
        let size = self.size(rel)?;
        if size > max {
            return Err(format!("{rel} is {size} bytes (the cap is {max})"));
        }
        match self.built() {
            Some(b) => Ok(b
                .files
                .iter()
                .find(|(name, _)| *name == rel)
                .map(|(_, bytes)| bytes.to_vec())
                .unwrap_or_default()),
            None => std::fs::read(self.path(rel)?).map_err(|e| format!("{rel}: {e}")),
        }
    }

    /// `theme.luau`'s text.
    fn text(&self, max: usize) -> Result<Vec<u8>, LoadError> {
        match self.built() {
            Some(b) => Ok(b.source.as_bytes().to_vec()),
            None => self.read(THEME_FILE, max as u64).map_err(LoadError::Read),
        }
    }
}

// ---- What theme.luau returns --------------------------------------------------

/// Why a theme didn't load: the user gets the base look and is told.
#[derive(Clone, Debug, PartialEq)]
pub enum LoadError {
    /// The file can't be read (or is over 512 KiB).
    Read(String),
    /// A syntax error, an error while running, the time or memory limit.
    Script(ThemeError),
    /// The file didn't return a table.
    NotATable,
    NoApi,
    /// Written for a later API than this app speaks.
    NewerApi(i64),
    NoName,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Read(m) => write!(f, "{m}"),
            LoadError::Script(e) => write!(f, "{e}"),
            LoadError::NotATable => write!(f, "theme.luau must return a table"),
            LoadError::NoApi => write!(f, "the theme doesn't say which API it is for (api = 1)"),
            LoadError::NewerApi(n) => write!(
                f,
                "needs a newer GMNB (the theme is for API {n}; this GMNB speaks {API})"
            ),
            LoadError::NoName => write!(f, "the theme has no name"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub license: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug)]
pub enum OptionKind {
    /// One of the choices' ids. `preview` draws a tile for one.
    Choice {
        choices: Vec<Choice>,
        preview: Option<FunctionHandle>,
    },
    /// `"#rrggbb"`.
    Color,
    Toggle,
    Slider {
        min: f64,
        max: f64,
        step: f64,
        /// Shown as a percentage.
        percent: bool,
    },
    /// No value: the label as an explanation.
    Note,
}

/// A small button at the end of an option's row.
#[derive(Clone, Debug)]
pub struct Action {
    pub label: String,
    pub tooltip: Option<String>,
    pub run: FunctionHandle,
}

#[derive(Clone, Debug)]
pub struct OptionDef {
    pub id: String,
    pub kind: OptionKind,
    pub label: String,
    pub description: Option<String>,
    pub group: Option<String>,
    /// `None` for a note.
    pub default: Option<OptionValue>,
    pub visible: Option<FunctionHandle>,
    pub actions: Vec<Action>,
}

pub const MAX_OPTIONS: usize = 24;
pub const MAX_CHOICES: usize = 16;
pub const MAX_LABEL: usize = 80;
pub const MAX_DESCRIPTION: usize = 300;

impl OptionDef {
    /// `value` as this option takes it: `None` if it doesn't fit (the
    /// wrong type, a choice that is gone, a colour that isn't one). A
    /// slider's number is brought into range and onto a step.
    pub fn accept(&self, value: &OptionValue) -> Option<OptionValue> {
        match (&self.kind, value) {
            (OptionKind::Choice { choices, .. }, OptionValue::Text(s)) => {
                choices.iter().any(|c| c.id == *s).then(|| value.clone())
            }
            (OptionKind::Color, OptionValue::Text(s)) => {
                let c = parse_hex(s).filter(|_| s.len() == 7)?;
                Some(OptionValue::Text(appcore::color::to_hex([
                    c[0], c[1], c[2],
                ])))
            }
            (OptionKind::Toggle, OptionValue::Bool(_)) => Some(value.clone()),
            (OptionKind::Slider { min, max, step, .. }, OptionValue::Number(n)) => {
                if !n.is_finite() {
                    return None;
                }
                let n = n.clamp(*min, *max);
                let n = if *step > 0.0 {
                    (min + ((n - min) / step).round() * step).clamp(*min, *max)
                } else {
                    n
                };
                Some(OptionValue::Number(n))
            }
            _ => None,
        }
    }
}

/// The values a theme is told (`ctx.options`): the saved one where it
/// fits its option, else the default. Always complete.
pub fn complete_options(
    options: &[OptionDef],
    saved: &BTreeMap<String, OptionValue>,
) -> BTreeMap<String, OptionValue> {
    options
        .iter()
        .filter_map(|o| {
            let default = o.default.clone()?;
            let value = saved
                .get(&o.id)
                .and_then(|v| o.accept(v))
                .unwrap_or(default);
            Some((o.id.clone(), value))
        })
        .collect()
}

/// A draw layer's hooks ([`Theme::draw`] runs them).
#[derive(Clone, Debug, Default)]
pub struct Layer {
    pub draw: Option<FunctionHandle>,
    /// Keypad only.
    pub place: Option<FunctionHandle>,
    /// Which events it is told about, and for how many seconds each is
    /// kept (at most 5).
    pub events: BTreeMap<String, f64>,
    pub pointer: bool,
}

/// The longest an event is kept for, in seconds (the contract's 5).
const MAX_EVENT_SECONDS: f64 = crate::events::MAX_LIFE;

pub const LAYERS: [&str; 4] = ["background", "keypad", "display", "overlay"];
pub const EVENTS: [&str; 6] = ["press", "result", "error", "show", "option", "mode"];

#[derive(Clone, Debug, PartialEq)]
pub struct Credit {
    pub title: String,
    /// A text file in the theme's folder.
    pub file: String,
}

/// The theme table, checked.
#[derive(Clone, Debug, Default)]
pub struct ThemeDef {
    pub meta: Meta,
    /// Font files in the theme's folder.
    pub fonts: Vec<String>,
    pub credits: Vec<Credit>,
    pub options: Vec<OptionDef>,
    pub style: Option<FunctionHandle>,
    /// By name ([`LAYERS`]).
    pub layers: BTreeMap<&'static str, Layer>,
    /// Which sound for which event ([`crate::sound`]).
    pub sounds: gmnb_sound::SoundTable,
    /// `sound(event, ctx)`: what to play for one event.
    pub sound: Option<FunctionHandle>,
}

fn text_field(
    m: &ThemeValue,
    key: &str,
    path: &str,
    max: usize,
    notes: &mut Notes,
) -> Option<String> {
    match m.get(key)? {
        ThemeValue::String(s) if s.chars().count() <= max => Some(s.clone()),
        ThemeValue::String(s) => {
            notes.push(
                &format!("{path}{key}"),
                format!("longer than {max} characters; cut short"),
            );
            Some(s.chars().take(max).collect())
        }
        other => {
            notes.push(
                &format!("{path}{key}"),
                format!("expected text, got {}", other.type_name()),
            );
            None
        }
    }
}

fn function_field(
    m: &ThemeValue,
    key: &str,
    path: &str,
    notes: &mut Notes,
) -> Option<FunctionHandle> {
    match m.get(key)? {
        ThemeValue::Function(f) => Some(f.clone()),
        other => {
            notes.push(
                &format!("{path}{key}"),
                format!("expected a function, got {}", other.type_name()),
            );
            None
        }
    }
}

fn option_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn option_value(v: &ThemeValue) -> Option<OptionValue> {
    match v {
        ThemeValue::Bool(b) => Some(OptionValue::Bool(*b)),
        ThemeValue::String(s) => Some(OptionValue::Text(s.clone())),
        other => other.as_f64().map(OptionValue::Number),
    }
}

fn parse_option(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<OptionDef> {
    const KEYS: [&str; 14] = [
        "id",
        "type",
        "label",
        "description",
        "group",
        "default",
        "choices",
        "preview",
        "min",
        "max",
        "step",
        "format",
        "visible",
        "actions",
    ];
    let ThemeValue::Map(m) = v else {
        notes.push(path, "expected an option table");
        return None;
    };
    for key in m.keys() {
        if !KEYS.contains(&key.as_str()) {
            notes.push(&format!("{path}.{key}"), "unknown key");
        }
    }
    let dot = format!("{path}.");
    let id = match v.get("id").and_then(ThemeValue::as_str) {
        Some(id) if option_id(id) => id.to_string(),
        _ => {
            notes.push(
                &format!("{dot}id"),
                "an option needs an id of letters, digits and _",
            );
            return None;
        }
    };
    let number =
        |key: &str, default: f64| v.get(key).and_then(ThemeValue::as_f64).unwrap_or(default);
    let kind = match v.get("type").and_then(ThemeValue::as_str) {
        Some("choice") => {
            let mut choices = Vec::new();
            let listed = v.get("choices").and_then(crate::check::list).unwrap_or(&[]);
            for (i, c) in listed.iter().enumerate() {
                let cpath = format!("{dot}choices[{}].", i + 1);
                let cid = c.get("id").and_then(ThemeValue::as_str);
                match cid {
                    Some(cid) if !cid.is_empty() && cid.len() <= 64 => {
                        if choices.len() >= MAX_CHOICES {
                            notes.push(
                                &format!("{dot}choices"),
                                format!("more than {MAX_CHOICES} choices; the rest are left out"),
                            );
                            break;
                        }
                        // Unnamed (absent or blank) it is named by its id: a tile
                        // nothing says the name of can't be heard.
                        let label = text_field(c, "label", &cpath, MAX_LABEL, notes)
                            .filter(|l| !l.trim().is_empty())
                            .unwrap_or_else(|| cid.to_string());
                        choices.push(Choice {
                            id: cid.to_string(),
                            label,
                        });
                    }
                    _ => notes.push(&format!("{cpath}id"), "a choice needs an id"),
                }
            }
            if choices.is_empty() {
                notes.push(&format!("{dot}choices"), "a choice option needs choices");
                return None;
            }
            OptionKind::Choice {
                choices,
                preview: function_field(v, "preview", &dot, notes),
            }
        }
        Some("color") => OptionKind::Color,
        Some("toggle") => OptionKind::Toggle,
        Some("slider") => {
            let (min, max) = (number("min", 0.0), number("max", 1.0));
            if max <= min || !max.is_finite() || !min.is_finite() {
                notes.push(&format!("{dot}max"), "a slider needs max above min");
                return None;
            }
            let step = number("step", 0.0);
            let percent = match v.get("format").and_then(ThemeValue::as_str) {
                None | Some("number") => false,
                Some("percent") => true,
                Some(_) => {
                    notes.push(
                        &format!("{dot}format"),
                        "expected \"number\" or \"percent\"",
                    );
                    false
                }
            };
            OptionKind::Slider {
                min,
                max,
                step: if step > 0.0 && step <= max - min {
                    step
                } else {
                    0.0
                },
                percent,
            }
        }
        Some("note") => OptionKind::Note,
        _ => {
            notes.push(
                &format!("{dot}type"),
                "expected \"choice\", \"color\", \"toggle\", \"slider\" or \"note\"",
            );
            return None;
        }
    };
    let mut actions = Vec::new();
    for (i, a) in v
        .get("actions")
        .and_then(crate::check::list)
        .unwrap_or(&[])
        .iter()
        .enumerate()
    {
        let apath = format!("{dot}actions[{}].", i + 1);
        let label = text_field(a, "label", &apath, MAX_LABEL, notes);
        let run = function_field(a, "run", &apath, notes);
        match (label, run) {
            (Some(label), Some(run)) if actions.len() < 4 => actions.push(Action {
                label,
                tooltip: text_field(a, "tooltip", &apath, MAX_DESCRIPTION, notes),
                run,
            }),
            _ => notes.push(&apath, "an action needs a label and a run function"),
        }
    }
    let note = matches!(kind, OptionKind::Note);
    // A control with no name (absent or blank) is named by its id: its row,
    // switch, slider or tile would otherwise be read as nothing.
    let label = text_field(
        v,
        "label",
        &dot,
        if note { MAX_DESCRIPTION } else { MAX_LABEL },
        notes,
    )
    .filter(|l| note || !l.trim().is_empty())
    .unwrap_or_else(|| if note { String::new() } else { id.clone() });
    let mut option = OptionDef {
        id,
        label,
        description: text_field(v, "description", &dot, MAX_DESCRIPTION, notes),
        group: text_field(v, "group", &dot, MAX_LABEL, notes),
        default: None,
        visible: function_field(v, "visible", &dot, notes),
        actions,
        kind,
    };
    // The default: the theme's if it fits, else the plainest value.
    let fallback = match &option.kind {
        OptionKind::Choice { choices, .. } => Some(OptionValue::Text(choices[0].id.clone())),
        OptionKind::Color => Some(OptionValue::Text("#000000".into())),
        OptionKind::Toggle => Some(OptionValue::Bool(false)),
        OptionKind::Slider { min, .. } => Some(OptionValue::Number(*min)),
        OptionKind::Note => None,
    };
    option.default = match (v.get("default"), &fallback) {
        (_, None) => None,
        (None, fallback) => fallback.clone(),
        (Some(given), fallback) => {
            let fits = option_value(given).and_then(|d| option.accept(&d));
            if fits.is_none() {
                notes.push(&format!("{dot}default"), "doesn't fit the option; ignored");
            }
            fits.or_else(|| fallback.clone())
        }
    };
    Some(option)
}

fn parse_layer(v: &ThemeValue, path: &str, notes: &mut Notes) -> Layer {
    let mut layer = Layer::default();
    let ThemeValue::Map(m) = v else {
        notes.push(path, "expected a layer table");
        return layer;
    };
    let dot = format!("{path}.");
    for key in m.keys() {
        if !["draw", "events", "pointer", "place"].contains(&key.as_str()) {
            notes.push(&format!("{dot}{key}"), "unknown key");
        }
    }
    layer.draw = function_field(v, "draw", &dot, notes);
    layer.place = function_field(v, "place", &dot, notes);
    match v.get("pointer") {
        None => {}
        Some(ThemeValue::Bool(b)) => layer.pointer = *b,
        Some(other) => notes.push(
            &format!("{dot}pointer"),
            format!("expected true or false, got {}", other.type_name()),
        ),
    }
    match v.get("events") {
        None => {}
        Some(ThemeValue::Map(events)) => {
            for (kind, seconds) in events {
                match (EVENTS.contains(&kind.as_str()), seconds.as_f64()) {
                    (true, Some(s)) if s > 0.0 => {
                        let kept = s.min(MAX_EVENT_SECONDS);
                        if kept < s {
                            notes.push(
                                &format!("{dot}events.{kind}"),
                                format!("{s} is out of range; using {kept}"),
                            );
                        }
                        layer.events.insert(kind.clone(), kept);
                    }
                    (true, _) => notes.push(&format!("{dot}events.{kind}"), "expected seconds"),
                    (false, _) => notes.push(&format!("{dot}events.{kind}"), "unknown event"),
                }
            }
        }
        Some(other) => notes.push(
            &format!("{dot}events"),
            format!(
                "expected a table of seconds by event, got {}",
                other.type_name()
            ),
        ),
    }
    layer
}

/// Checks what `theme.luau` returned. Only what makes a theme unusable is
/// an error; the rest is noted and left out.
pub fn parse(v: &ThemeValue, notes: &mut Notes) -> Result<ThemeDef, LoadError> {
    const KEYS: [&str; 13] = [
        "api",
        "name",
        "description",
        "author",
        "version",
        "license",
        "fonts",
        "credits",
        "options",
        "style",
        "layers",
        "sounds",
        "sound",
    ];
    let ThemeValue::Map(m) = v else {
        return Err(LoadError::NotATable);
    };
    match v.get("api").and_then(ThemeValue::as_f64) {
        None => return Err(LoadError::NoApi),
        Some(n) if n > API as f64 => return Err(LoadError::NewerApi(n as i64)),
        Some(n) if n < 1.0 => return Err(LoadError::NoApi),
        Some(_) => {}
    }
    for key in m.keys() {
        if !KEYS.contains(&key.as_str()) {
            notes.push(key, "unknown key");
        }
    }
    let mut def = ThemeDef::default();
    def.meta.name = text_field(v, "name", "", MAX_LABEL, notes)
        .filter(|n| !n.trim().is_empty())
        .ok_or(LoadError::NoName)?;
    def.meta.description = text_field(v, "description", "", MAX_DESCRIPTION, notes);
    def.meta.author = text_field(v, "author", "", MAX_LABEL, notes);
    def.meta.version = text_field(v, "version", "", MAX_LABEL, notes);
    def.meta.license = text_field(v, "license", "", MAX_LABEL, notes);

    for (i, f) in v
        .get("fonts")
        .and_then(crate::check::list)
        .unwrap_or(&[])
        .iter()
        .enumerate()
    {
        let path = format!("fonts[{}]", i + 1);
        match f.as_str() {
            Some(file) if safe_relative(file) => {
                if def.fonts.len() >= MAX_FONTS {
                    notes.push(&path, format!("more than {MAX_FONTS} fonts; left out"));
                } else {
                    def.fonts.push(file.to_string());
                }
            }
            _ => notes.push(&path, "expected a path inside the theme's folder"),
        }
    }
    for (i, c) in v
        .get("credits")
        .and_then(crate::check::list)
        .unwrap_or(&[])
        .iter()
        .enumerate()
    {
        let path = format!("credits[{}].", i + 1);
        let title = text_field(c, "title", &path, MAX_LABEL, notes);
        let file = c
            .get("file")
            .and_then(ThemeValue::as_str)
            .filter(|f| safe_relative(f));
        match (title, file) {
            (Some(title), Some(file)) if def.credits.len() < 8 => def.credits.push(Credit {
                title,
                file: file.to_string(),
            }),
            _ => notes.push(
                &path,
                "a credit needs a title and a file inside the theme's folder",
            ),
        }
    }
    if let Some(options) = v.get("options") {
        match crate::check::list(options) {
            Some(listed) => {
                for (i, o) in listed.iter().enumerate() {
                    let path = format!("options[{}]", i + 1);
                    if def.options.len() >= MAX_OPTIONS {
                        notes.push(&path, format!("more than {MAX_OPTIONS} options; left out"));
                        break;
                    }
                    if let Some(option) = parse_option(o, &path, notes) {
                        if def.options.iter().any(|d| d.id == option.id) {
                            notes.push(&path, format!("a second option \"{}\"", option.id));
                        } else {
                            def.options.push(option);
                        }
                    }
                }
            }
            None => notes.push("options", "expected a list of options"),
        }
    }
    def.style = function_field(v, "style", "", notes);
    match v.get("layers") {
        None => {}
        Some(ThemeValue::Map(layers)) => {
            for (name, layer) in layers {
                match LAYERS.iter().find(|l| *l == name) {
                    Some(known) => {
                        let path = format!("layers.{name}");
                        let parsed = parse_layer(layer, &path, notes);
                        if parsed.place.is_some() && *known != "keypad" {
                            notes.push(
                                &format!("{path}.place"),
                                "only the keypad layer has `place`; it is never called here",
                            );
                        }
                        def.layers.insert(known, parsed);
                    }
                    None => notes.push(&format!("layers.{name}"), "unknown layer"),
                }
            }
        }
        Some(_) => notes.push("layers", "expected a table of layers"),
    }
    def.sounds = crate::sound::parse_table(v.get("sounds"), notes);
    def.sound = function_field(v, "sound", "", notes);
    Ok(def)
}

// ---- A loaded theme ---------------------------------------------------------------

/// A font file a theme brings.
pub enum FontFile {
    /// A built-in theme's: its name and bytes.
    Bytes(&'static str, &'static [u8]),
    /// A file in the theme's folder.
    Path(PathBuf),
}

/// A theme, loaded: its own Luau machine, what its file returned, and its
/// `style`. Dropping it unloads it.
pub struct Theme {
    pub source: Source,
    pub def: ThemeDef,
    /// What loading it noted.
    pub notes: Notes,
    engine: Engine,
    ctx: HostTable,
    /// What the `ctx` table holds now: it is refilled only for another.
    ctx_filled: RefCell<Option<Ctx>>,
    /// The `ctx` the style was last worked out for, and what came of it.
    last: Option<(Ctx, Resolved)>,
    /// The draw API: the `draw` table, the recorder and the pictures.
    api: draw::Api,
    /// The tables frames are told through.
    frames: RefCell<Frames>,
    /// What a preview is told.
    preview: HostTable,
}

/// One theme of the Settings list.
#[derive(Clone, Debug)]
pub struct Listing {
    pub id: String,
    pub origin: Origin,
    /// Its name and description, or why it doesn't load.
    pub meta: Result<Meta, String>,
}

/// What Settings lists: each theme's file is run in a throwaway sandbox
/// for its name and description, which is then discarded.
pub fn list(sources: &[Source], limits: &Limits) -> Vec<Listing> {
    sources
        .iter()
        .map(|source| Listing {
            id: source.id.clone(),
            origin: source.origin,
            meta: Theme::load(source.clone(), limits.clone())
                .map(|t| t.def.meta.clone())
                .map_err(|e| e.to_string()),
        })
        .collect()
}

impl Theme {
    /// Runs the theme's file, once, in a sandbox of its own, and checks
    /// the table it returns.
    pub fn load(source: Source, limits: Limits) -> Result<Theme, LoadError> {
        let text = source.text(limits.source_bytes)?;
        let pictures = Pictures::new(source.clone(), limits.draw.clone());
        let draw_limits = limits.draw.clone();
        let mut api = None;
        let engine = Engine::with_globals(limits, |lua| {
            library::register(lua, |lua, gmnb| {
                api = Some(draw::Api::register(lua, gmnb, draw_limits, pictures)?);
                Ok(())
            })
        })
        .map_err(LoadError::Script)?;
        let Some(api) = api else {
            return Err(LoadError::Script(ThemeError::Internal(
                "the draw API wasn't registered".into(),
            )));
        };
        let name = format!("@{}/{THEME_FILE}", source.id);
        let value = engine.load(&name, &text).map_err(LoadError::Script)?;
        let mut notes = Notes::default();
        let def = parse(&value, &mut notes)?;
        let ctx = engine.host_table(0, 8).map_err(LoadError::Script)?;
        let frames = Frames::new(&engine).map_err(LoadError::Script)?;
        let preview = engine.host_table(0, 3).map_err(LoadError::Script)?;
        let mut theme = Theme {
            source,
            def,
            notes,
            engine,
            ctx,
            ctx_filled: RefCell::new(None),
            last: None,
            api,
            frames: RefCell::new(frames),
            preview,
        };
        theme.check_files();
        Ok(theme)
    }

    /// Drops the fonts and credits that can't be read or are too big: the
    /// theme loads without them, with a note.
    fn check_files(&mut self) {
        let mut total = 0;
        let (source, notes) = (&self.source, &mut self.notes);
        self.def.fonts.retain(|file| match source.size(file) {
            Ok(size) if size > MAX_FONT_BYTES => {
                notes.push(file, format!("{size} bytes; a font file is at most 16 MiB"));
                false
            }
            Ok(size) if total + size > MAX_FONTS_BYTES => {
                notes.push(file, "the theme's fonts are over 32 MiB together; left out");
                false
            }
            Ok(size) => {
                total += size;
                true
            }
            Err(e) => {
                notes.push(file, e);
                false
            }
        });
        self.def
            .credits
            .retain(|credit| match source.size(&credit.file) {
                Ok(size) if size > MAX_CREDIT_BYTES => {
                    notes.push(&credit.file, "a credits file is at most 64 KiB; left out");
                    false
                }
                Ok(_) => true,
                Err(e) => {
                    notes.push(&credit.file, e);
                    false
                }
            });
    }

    /// The theme's font files, to add to the app's font map.
    pub fn font_files(&self) -> Vec<FontFile> {
        self.def
            .fonts
            .iter()
            .filter_map(|file| match self.source.built() {
                Some(b) => b
                    .files
                    .iter()
                    .find(|(name, _)| name == file)
                    .map(|(name, bytes)| FontFile::Bytes(name, bytes)),
                None => self.source.path(file).ok().map(FontFile::Path),
            })
            .collect()
    }

    /// The theme's credits: each title and its text.
    pub fn credits(&self) -> Vec<(String, String)> {
        self.def
            .credits
            .iter()
            .filter_map(|c| {
                let bytes = self.source.read(&c.file, MAX_CREDIT_BYTES).ok()?;
                Some((
                    c.title.clone(),
                    String::from_utf8_lossy(&bytes).into_owned(),
                ))
            })
            .collect()
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Whether the theme hit its memory limit (or mlua panicked): it is
    /// to be unloaded.
    pub fn is_dead(&self) -> bool {
        self.engine.is_dead()
    }

    /// What the theme printed or warned since the last call.
    pub fn take_log(&self) -> Vec<String> {
        self.engine.take_log().lines
    }

    /// `ctx` as the theme sees it: refilled in place when it changed,
    /// read-only.
    /// The `ctx` table hooks are given, filled for `ctx`.
    pub(crate) fn ctx_for(&self, ctx: &Ctx) -> Result<&Table, ThemeError> {
        self.fill_ctx(ctx)?;
        Ok(self.ctx.table())
    }

    fn fill_ctx(&self, ctx: &Ctx) -> Result<(), ThemeError> {
        if self.ctx_filled.borrow().as_ref() == Some(ctx) {
            return Ok(());
        }
        self.ctx_filled.replace(None);
        self.write_ctx(ctx)?;
        self.ctx_filled.replace(Some(ctx.clone()));
        Ok(())
    }

    fn write_ctx(&self, ctx: &Ctx) -> Result<(), ThemeError> {
        let lua = self.engine.lua();
        self.engine.fill(&self.ctx, |t| {
            let base = Tokens::base(ctx);
            t.raw_set("dark", ctx.dark)?;
            match ctx.accent {
                Some([r, g, b]) => t.raw_set("accent", frozen(rgba(lua, [r, g, b, 1.0])?))?,
                None => t.raw_set("accent", Value::Nil)?,
            }
            t.raw_set("opacity", f64::from(ctx.opacity()))?;
            let options = lua.create_table()?;
            for (id, value) in &ctx.options {
                match value {
                    OptionValue::Text(s) => options.raw_set(id.as_str(), s.as_str())?,
                    OptionValue::Number(n) => options.raw_set(id.as_str(), *n)?,
                    OptionValue::Bool(b) => options.raw_set(id.as_str(), *b)?,
                }
            }
            t.raw_set("options", frozen(options))?;
            t.raw_set("motion", ctx.motion)?;
            let font = lua.create_table()?;
            font.raw_set("family", ctx.font.0.as_str())?;
            font.raw_set("size", f64::from(ctx.font.1))?;
            t.raw_set("font", frozen(font))?;
            let b = lua.create_table()?;
            b.raw_set("colors", colors(lua, &base.colors)?)?;
            let text = lua.create_table()?;
            for (name, f) in base.text.named() {
                let font = lua.create_table()?;
                font.raw_set(
                    "family",
                    frozen(lua.create_sequence_from(f.family.iter().map(String::as_str))?),
                )?;
                font.raw_set("size", f64::from(f.size))?;
                font.raw_set("weight", f64::from(f.weight))?;
                text.raw_set(name, frozen(font))?;
            }
            b.raw_set("text", frozen(text))?;
            let shape = lua.create_table()?;
            shape.raw_set("radius", f64::from(base.shape.radius))?;
            shape.raw_set("row_radius", f64::from(base.shape.row_radius))?;
            shape.raw_set("card_radius", f64::from(base.shape.card_radius))?;
            b.raw_set("shape", frozen(shape))?;
            let scrim = base.colors.text_scrim(ctx.opacity());
            b.raw_set("text_scrim", frozen(rgba(lua, scrim)?))?;
            t.raw_set("base", frozen(b))?;
            Ok(())
        })
    }

    /// The style for `ctx`: `style(ctx)` is run only when `ctx` changed
    /// since the last call, and its result checked. Without a `style` the
    /// theme looks like the base. On an error the theme's last style (if
    /// any) is the caller's to keep.
    pub fn style(&mut self, ctx: &Ctx) -> Result<&Resolved, ThemeError> {
        let fresh = !matches!(&self.last, Some((last, _)) if last == ctx);
        if fresh {
            let resolved = match &self.def.style {
                None => resolve(ctx, &ThemeValue::Map(BTreeMap::new())),
                Some(style) => {
                    self.fill_ctx(ctx)?;
                    let value = self.engine.call(style, Budget::Style, self.ctx.table())?;
                    resolve(ctx, &value)
                }
            };
            self.last = Some((ctx.clone(), resolved));
        }
        match &self.last {
            Some((_, resolved)) => Ok(resolved),
            None => Err(ThemeError::Internal("no style".into())),
        }
    }

    /// Whether Settings shows `option` now: its `visible(options, ctx)`,
    /// or `true` without one (or if it fails).
    pub fn visible(&self, option: &OptionDef, ctx: &Ctx) -> bool {
        let Some(visible) = &option.visible else {
            return true;
        };
        if self.fill_ctx(ctx).is_err() {
            return true;
        }
        let Ok(options) = self.ctx.table().raw_get::<Value>("options") else {
            return true;
        };
        !matches!(
            self.engine
                .call(visible, Budget::Visible, (options, self.ctx.table())),
            Ok(ThemeValue::Bool(false))
        )
    }

    /// Runs an option's action: the options it changes, each checked
    /// against its option (what doesn't fit is left out).
    pub fn run_action(
        &self,
        action: &Action,
        ctx: &Ctx,
    ) -> Result<BTreeMap<String, OptionValue>, ThemeError> {
        self.fill_ctx(ctx)?;
        let options = self
            .ctx
            .table()
            .raw_get::<Value>("options")
            .unwrap_or(Value::Nil);
        let changed = self
            .engine
            .call(&action.run, Budget::Action, (options, self.ctx.table()))?;
        let mut out = BTreeMap::new();
        if let ThemeValue::Map(m) = changed {
            for (id, value) in m {
                let fits = self
                    .def
                    .options
                    .iter()
                    .find(|o| o.id == id)
                    .zip(option_value(&value))
                    .and_then(|(o, v)| o.accept(&v));
                if let Some(v) = fits {
                    out.insert(id, v);
                }
            }
        }
        Ok(out)
    }

    // ---- Drawing -----------------------------------------------------------------

    /// The theme's `layers` entry for `kind`, if it has one.
    pub fn layer(&self, kind: LayerKind) -> Option<&Layer> {
        self.def.layers.get(kind.name())
    }

    /// Whether `kind` has a `draw` hook that is still called.
    pub fn draws(&self, kind: LayerKind) -> bool {
        self.layer(kind)
            .and_then(|l| l.draw.as_ref())
            .is_some_and(|f| !f.is_disabled())
    }

    /// The draw API's state: the theme's pictures, and what measures
    /// text.
    pub fn api(&self) -> &draw::Api {
        &self.api
    }

    /// What `draw:text` fonts are laid onto: the last style's body text.
    fn sync_base_font(&self) {
        if let Some((_, resolved)) = &self.last {
            self.api.set_base_font(&resolved.style.tokens.text.body);
        }
    }

    /// Runs a layer's hooks for one frame: `place` for each cell of a
    /// keypad (not under reduced motion; all of them within
    /// [`Limits::place`]), then `draw` (within [`Limits::frame`]).
    ///
    /// A `draw` that fails, runs out of time or goes over a cap draws
    /// nothing and counts against the hook: after [`Limits::strikes`]
    /// failures, or one that ran four times its limit, the hook is off
    /// ([`Failure::Off`]) until the theme is loaded again.
    pub fn draw(&self, kind: LayerKind, ctx: &Ctx, frame: &Frame) -> Result<Drawn, Failure> {
        let Some(layer) = self.layer(kind) else {
            return Err(Failure::Off);
        };
        let Some(hook) = layer.draw.as_ref().filter(|f| !f.is_disabled()) else {
            return Err(Failure::Off);
        };
        self.fill_ctx(ctx).map_err(Failure::Failed)?;
        self.sync_base_font();
        let mut frames = self.frames.borrow_mut();
        frames
            .fill(&self.engine, frame, self.ctx.table())
            .map_err(Failure::Failed)?;
        let mut drawn = Drawn::default();
        if let (Some(place), Some(keys), true) = (&layer.place, &frame.keys, frame.motion)
            && !place.is_disabled()
        {
            let span = self.engine.span(Budget::Place);
            let mut places = Vec::with_capacity(keys.cells.len());
            for i in 0..keys.cells.len() {
                let Some(cell) = frames.cell(keys.pad, i) else {
                    break;
                };
                match self.engine.call_in(place, &span, (cell, frames.table())) {
                    Ok(v) => places.push(layer::placement(&v)),
                    // One failure ends this frame's placing: the cells
                    // stay where they are.
                    Err(_) => {
                        places.clear();
                        break;
                    }
                }
            }
            drawn.places = places;
        }
        let glyphs = frame.value.as_ref().map_or([0, 0], |v| {
            [
                v.glyphs.glyphs.len() as u32,
                v.previous.map_or(0, |p| p.glyphs.len() as u32),
            ]
        });
        // Which layer, where: this keypad's, that readout's.
        let place = frame
            .keys
            .as_ref()
            .map(|k| k.pad)
            .or(frame.value.as_ref().map(|v| v.readout))
            .unwrap_or(0);
        let which = LayerKind::ALL.iter().position(|k| *k == kind).unwrap_or(0) as u64;
        // Where a readout's glyphs are: what a `draw:glyph` covers.
        let glyph_box = frame.value.as_ref().map_or([0.0; 4], |v| {
            let r = v.glyphs.rect;
            match v.previous {
                Some(p) => {
                    let q = p.rect;
                    let (x0, y0) = (r[0].min(q[0]), r[1].min(q[1]));
                    let (x1, y1) = (
                        (r[0] + r[2]).max(q[0] + q[2]),
                        (r[1] + r[3]).max(q[1] + q[3]),
                    );
                    [x0, y0, x1 - x0, y1 - y0]
                }
                None => r,
            }
        });
        self.api.begin(&draw::Begin {
            glyphs,
            time: frame.time,
            motion: frame.motion,
            still: false,
            layer: which << 60 | (place & ((1 << 60) - 1)),
            size: [frame.width, frame.height],
            whole: matches!(kind, LayerKind::Background | LayerKind::Overlay),
            glyph_box,
            window: frame.window,
        });
        let result = self
            .engine
            .call(hook, Budget::Frame, (self.api.table(), frames.table()));
        let again = match result {
            Ok(v) => v,
            Err(e) => {
                self.api.abandon();
                return Err(Failure::Failed(e));
            }
        };
        match self.api.finish() {
            Ok(list) => drawn.list = list,
            Err(cap) => {
                self.engine.strike(hook);
                return Err(Failure::Failed(ThemeError::Limit(cap)));
            }
        }
        drawn.again = again.as_f64();
        // An animation drawn on the layer's clock asks for its next frame
        // itself; what `draw` returned stays what `draw` returned.
        drawn.due = self.api.take_due();
        drawn.retry = self.api.take_retry();
        Ok(drawn)
    }

    /// Draws the tile of one choice of a `choice` option with a
    /// `preview`: a still picture of [`layer::PREVIEW_SIZE`].
    pub fn preview(
        &self,
        option: &OptionDef,
        choice: &str,
        ctx: &Ctx,
    ) -> Result<DisplayList, ThemeError> {
        let OptionKind::Choice {
            preview: Some(hook),
            ..
        } = &option.kind
        else {
            return Err(ThemeError::Unsupported("the option has no preview".into()));
        };
        self.fill_ctx(ctx)?;
        self.sync_base_font();
        let (w, h) = layer::PREVIEW_SIZE;
        layer::preview_table(&self.engine, &self.preview, w, h, self.ctx.table())?;
        // A tile is a still picture: animations show their first frame.
        self.api.begin(&draw::Begin {
            still: true,
            layer: u64::MAX,
            size: [w, h],
            whole: true,
            ..draw::Begin::default()
        });
        let result = self.engine.call(
            hook,
            Budget::Preview,
            (self.api.table(), self.preview.table(), choice),
        );
        if let Err(e) = result {
            self.api.abandon();
            return Err(e);
        }
        self.api.finish().map_err(|cap| {
            self.engine.strike(hook);
            ThemeError::Limit(cap)
        })
    }

    /// What the theme's author should know of its drawing since the last
    /// call: each message once in the theme's life.
    pub fn take_draw_notes(&self) -> Vec<Note> {
        self.api
            .take_notes()
            .into_iter()
            .map(|message| Note {
                path: "draw".into(),
                message,
            })
            .collect()
    }

    /// Each hook of the theme's layers: its name, its failures so far and
    /// whether it has been switched off.
    pub fn hooks(&self) -> Vec<(String, u32, bool)> {
        let mut out = Vec::new();
        for (name, layer) in &self.def.layers {
            for (hook, f) in [("draw", &layer.draw), ("place", &layer.place)] {
                if let Some(f) = f {
                    out.push((format!("{name}.{hook}"), f.strikes(), f.is_disabled()));
                }
            }
        }
        out
    }
}

fn frozen(t: Table) -> Table {
    t.set_readonly(true);
    t
}

fn rgba(lua: &Lua, c: [f32; 4]) -> mlua::Result<Table> {
    let t = lua.create_table_with_capacity(0, 4)?;
    t.raw_set("r", c[0])?;
    t.raw_set("g", c[1])?;
    t.raw_set("b", c[2])?;
    t.raw_set("a", c[3])?;
    Ok(t)
}

/// The colour tokens as a read-only table.
fn colors(lua: &Lua, c: &Colors) -> mlua::Result<Table> {
    let t = library::colors_table(lua, c)?;
    for pair in t.pairs::<Value, Value>() {
        if let (_, Value::Table(inner)) = pair? {
            if let Ok(Value::Table(first)) = inner.raw_get::<Value>(1) {
                // `series`: a list of colours.
                first.set_readonly(true);
                for i in 2..=inner.raw_len() {
                    if let Ok(Value::Table(c)) = inner.raw_get::<Value>(i) {
                        c.set_readonly(true);
                    }
                }
            }
            inner.set_readonly(true);
        }
    }
    Ok(frozen(t))
}
