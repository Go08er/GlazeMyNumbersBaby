//! Sound in a theme (docs/theming-media.md, "Sound"): `gmnb.synth`,
//! `gmnb.sound` and `gmnb.note`; the theme table's `sounds`; what a
//! `sound(event, ctx)` function returns.
//!
//! Nothing here makes a sound, renders one, reads a sound file or starts a
//! thread: a theme's file is also run just to list it in Settings. What a
//! theme declares is kept as data ([`Theme::sounds`]), and the app hands
//! that to the sound engine (`gmnb_sound::SoundEngine::load`) if and when
//! sound is on.
//!
//! A value the engine can't use is never an error for the theme: its
//! default is used, a number out of range is clamped, and the author gets
//! a note ([`Theme::take_sound_notes`]).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gmnb_sound::{
    Curve, Effect, Entry, Env, EventName, FileData, Filter, FilterKind, Fm, Glide, Layer, Lfo, Pan,
    Pick, Play, PlayParams, Recipe, Slot, SoundId, SoundSource, SoundTable, ThemeSounds, Varied,
    Wave, limits, note_name,
};
use mlua::{Lua, Table, Value};

use crate::ThemeValue;
use crate::check::{Note, Notes};
use crate::theme::{Theme, safe_relative};

/// The time one `sound(event, ctx)` call has (`Budget::Custom`).
pub const HOOK_TIME: Duration = Duration::from_millis(2);

// ---- What a theme has declared ------------------------------------------------

/// One sound a theme made, as it was asked for.
#[derive(Clone, Debug, PartialEq)]
enum Declared {
    /// Asked for wrongly: it plays nothing.
    Silent,
    Synth(Recipe),
    File {
        name: String,
        gain: Option<f64>,
    },
}

/// A theme's sounds so far, by the order they were made in.
#[derive(Default)]
struct Sounds {
    list: Vec<Declared>,
    notes: Notes,
    /// Whether "more than 32 sounds" has been said.
    full: bool,
}

type Shared = Rc<RefCell<Sounds>>;

fn shared(lua: &Lua) -> Option<Shared> {
    lua.app_data_ref::<Shared>().map(|s| s.clone())
}

/// The Sound a script holds: `{ kind = "sound", id = n }`, read-only.
/// `id` 0 is the sound that plays nothing.
fn handle(lua: &Lua, id: usize) -> mlua::Result<Table> {
    let t = lua.create_table_with_capacity(0, 2)?;
    t.raw_set("kind", "sound")?;
    t.raw_set("id", id as i64)?;
    t.set_readonly(true);
    Ok(t)
}

/// Adds `gmnb.synth`, `gmnb.sound` and `gmnb.note` to the `gmnb` table.
pub fn register(lua: &Lua, gmnb: &Table) -> mlua::Result<()> {
    let sounds: Shared = Rc::default();
    lua.set_app_data(sounds.clone());
    // The handles given out, by the sound's order from 1; [0] is the
    // sound that plays nothing (a 33rd sound, say).
    let handles: Rc<RefCell<Vec<Table>>> = Rc::new(RefCell::new(vec![handle(lua, 0)?]));

    // Called again with the same arguments, both return the same sound at
    // no cost: a sound asked for inside a hook is made once.
    let declare = {
        let (sounds, handles) = (sounds.clone(), handles.clone());
        move |lua: &Lua, what: Declared, notes: Notes| -> mlua::Result<Table> {
            let mut sounds = sounds.borrow_mut();
            let mut handles = handles.borrow_mut();
            if what != Declared::Silent
                && let Some(at) = sounds.list.iter().position(|d| *d == what)
            {
                return Ok(handles[at + 1].clone());
            }
            if sounds.list.len() >= limits::SOUNDS {
                if !sounds.full {
                    sounds.full = true;
                    sounds.notes.push(
                        "sound",
                        format!("more than {} sounds; the rest play nothing", limits::SOUNDS),
                    );
                }
                return Ok(handles[0].clone());
            }
            sounds.list.push(what);
            let id = sounds.list.len();
            for note in notes.list {
                let path = format!("sound[{id}]{}", note.path);
                sounds.notes.push(&path, note.message);
            }
            let h = handle(lua, id)?;
            handles.push(h.clone());
            Ok(h)
        }
    };

    let synth = declare.clone();
    gmnb.raw_set(
        "synth",
        lua.create_function(move |lua, recipe: Value| {
            let mut notes = Notes::default();
            let what = match read_recipe(&recipe, &mut notes) {
                Some(recipe) => Declared::Synth(recipe),
                None => Declared::Silent,
            };
            synth(lua, what, notes)
        })?,
    )?;
    gmnb.raw_set(
        "sound",
        lua.create_function(move |lua, (file, options): (Value, Value)| {
            let mut notes = Notes::default();
            let what = read_file(&file, &options, &mut notes);
            declare(lua, what, notes)
        })?,
    )?;
    gmnb.raw_set(
        "note",
        lua.create_function(|_, name: Value| {
            let hertz = match &name {
                Value::String(s) => s.to_str().ok().and_then(|s| note_name(&s)),
                _ => None,
            };
            hertz.ok_or_else(|| {
                mlua::Error::runtime(
                    "gmnb.note: a note is a letter A to G, an optional # or b, and an octave -1 to 9 (\"A4\", \"C#5\", \"Eb3\")",
                )
            })
        })?,
    )?;
    Ok(())
}

impl Theme {
    /// Everything about sound the theme has declared so far: its sounds
    /// (by `SoundId`, in the order it made them), its `sounds` table, and
    /// whether it has a `sound` function. What the app hands the sound
    /// engine.
    ///
    /// A sound file is only looked up here (is it in the theme's folder?),
    /// not read: the engine reads it on the sound thread.
    pub fn sounds(&self) -> ThemeSounds {
        let declared = shared(self.engine().lua());
        let list = declared.as_ref().map(|s| s.borrow().list.clone());
        let sounds = list
            .unwrap_or_default()
            .into_iter()
            .map(|d| match d {
                Declared::Silent => SoundSource::Silent,
                Declared::Synth(recipe) => SoundSource::Synth(recipe),
                Declared::File { name, gain } => {
                    let data = match self.source.dir() {
                        Some(_) => match self.source.path(&name) {
                            Ok(path) => FileData::Path(path),
                            Err(why) => FileData::Missing(why),
                        },
                        // A built-in theme's files are in the app.
                        None => match self.source.read(&name, limits::FILE_BYTES) {
                            Ok(bytes) => FileData::Bytes(bytes.into()),
                            Err(why) => FileData::Missing(why),
                        },
                    };
                    SoundSource::File { name, data, gain }
                }
            })
            .collect();
        ThemeSounds {
            sounds,
            table: self.def.sounds.clone(),
            hook: self.def.sound.is_some(),
        }
    }

    /// How many sounds the theme has made so far. (More than at the last
    /// look: its `sound` function made one, and the engine is to be told,
    /// `SoundEngine::extend`.)
    pub fn sounds_declared(&self) -> usize {
        shared(self.engine().lua()).map_or(0, |s| s.borrow().list.len())
    }

    /// What the theme's author should know about the sounds it made,
    /// since the last call.
    pub fn take_sound_notes(&self) -> Vec<Note> {
        match shared(self.engine().lua()) {
            Some(s) => std::mem::take(&mut s.borrow_mut().notes).list,
            None => Vec::new(),
        }
    }
}

// ---- Reading a recipe ------------------------------------------------------------
//
// A recipe is read raw, straight from the script's tables: no script code
// runs (no metamethods), and a glide `{ 950, 420, time = 0.03 }`, a list
// with named fields too, is fine here (the general conversion of what a
// script returns refuses such tables).

fn get(t: &Table, key: &str) -> Value {
    t.raw_get::<Value>(key).unwrap_or(Value::Nil)
}

fn describe(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            let text = s.to_string_lossy();
            let mut short: String = text.chars().take(40).collect();
            if short.len() < text.len() {
                short.push('…');
            }
            format!("{short:?}")
        }
        Value::Table(_) => "a table".into(),
        Value::Function(_) => "a function".into(),
        other => format!("a {}", other.type_name()),
    }
}

fn wrong(path: &str, want: &str, got: &Value, notes: &mut Notes) {
    notes.push(path, format!("expected {want}, got {}", describe(got)));
}

/// Notes the keys of `t` that are not in `known` (looking at no more
/// than a table of this kind could sensibly hold). `list` allows the
/// entries 1 to n.
fn unknown_keys(t: &Table, path: &str, known: &[&str], list: bool, notes: &mut Notes) {
    let mut seen = 0;
    let _ = t.for_each::<Value, Value>(|k, _| {
        seen += 1;
        if seen > 64 {
            return Err(mlua::Error::runtime("enough"));
        }
        match &k {
            Value::String(s) => {
                let key = s.to_string_lossy();
                if !known.contains(&key.as_str()) {
                    let key: String = key.chars().take(40).collect();
                    notes.push(&format!("{path}.{key}"), "unknown key");
                }
            }
            Value::Integer(_) if list => {}
            other => notes.push(path, format!("unknown key {}", describe(other))),
        }
        Ok(())
    });
}

/// A finite number; `None` (with a note, unless it is simply absent) for
/// anything else.
fn number(v: &Value, path: &str, notes: &mut Notes) -> Option<f64> {
    match v {
        Value::Nil => None,
        Value::Integer(i) => Some(*i as f64),
        Value::Number(n) if n.is_finite() => Some(*n),
        other => {
            wrong(path, "a number", other, notes);
            None
        }
    }
}

/// A number brought into `min..=max`, with a note if it was outside.
fn ranged(v: &Value, path: &str, min: f64, max: f64, notes: &mut Notes) -> Option<f64> {
    let n = number(v, path, notes)?;
    if n < min || n > max {
        let to = n.clamp(min, max);
        notes.push(path, format!("{n} is out of range; using {to}"));
        return Some(to);
    }
    Some(n)
}

/// A number that is at least `min`.
fn at_least(v: &Value, path: &str, min: f64, notes: &mut Notes) -> Option<f64> {
    ranged(v, path, min, f64::MAX, notes)
}

fn word<T: Copy>(v: &Value, path: &str, words: &[(&str, T)], notes: &mut Notes) -> Option<T> {
    if matches!(v, Value::Nil) {
        return None;
    }
    let found = match v {
        Value::String(s) => {
            let text = s.to_string_lossy();
            words.iter().find(|(w, _)| *w == text).map(|(_, t)| *t)
        }
        _ => None,
    };
    if found.is_none() {
        let names: Vec<String> = words.iter().map(|(w, _)| format!("\"{w}\"")).collect();
        wrong(path, &format!("one of {}", names.join(", ")), v, notes);
    }
    found
}

fn table<'a>(v: &'a Value, path: &str, want: &str, notes: &mut Notes) -> Option<&'a Table> {
    match v {
        Value::Nil => None,
        Value::Table(t) => Some(t),
        other => {
            wrong(path, want, other, notes);
            None
        }
    }
}

const CURVES: [(&str, Curve); 2] = [("exp", Curve::Exp), ("linear", Curve::Linear)];

/// A pitch: hertz (not below 0), or a note name.
fn pitch(v: &Value, path: &str, notes: &mut Notes) -> Option<f64> {
    match v {
        Value::String(s) => {
            let hertz = s.to_str().ok().and_then(|s| note_name(&s));
            if hertz.is_none() {
                wrong(
                    path,
                    "hertz or a note name (\"A4\", \"C#5\", \"Eb3\")",
                    v,
                    notes,
                );
            }
            hertz
        }
        other => at_least(other, path, 0.0, notes),
    }
}

fn plain(v: &Value, path: &str, notes: &mut Notes) -> Option<f64> {
    number(v, path, notes)
}

/// A value that may glide: one value, or `{ from, to, time = seconds,
/// curve = "exp" | "linear" }`, each end read by `end`.
fn glide(
    v: &Value,
    path: &str,
    end: fn(&Value, &str, &mut Notes) -> Option<f64>,
    notes: &mut Notes,
) -> Option<Glide> {
    let Value::Table(t) = v else {
        return end(v, path, notes).map(Glide::fixed);
    };
    unknown_keys(t, path, &["time", "curve"], true, notes);
    if t.raw_len() != 2 {
        notes.push(path, "a glide is { from, to, time = seconds }");
        return None;
    }
    let from = end(
        &t.raw_get::<Value>(1).unwrap_or(Value::Nil),
        &format!("{path}[1]"),
        notes,
    )?;
    let to = end(
        &t.raw_get::<Value>(2).unwrap_or(Value::Nil),
        &format!("{path}[2]"),
        notes,
    )?;
    let time = number(&get(t, "time"), &format!("{path}.time"), notes);
    let mut curve =
        word(&get(t, "curve"), &format!("{path}.curve"), &CURVES, notes).unwrap_or(Curve::Exp);
    if curve == Curve::Exp && from != to && (from <= 0.0 || to <= 0.0) {
        notes.push(
            path,
            "an \"exp\" glide needs both values above zero; gliding in a straight line",
        );
        curve = Curve::Linear;
    }
    Some(Glide {
        from,
        to,
        time,
        curve,
    })
}

const FILTERS: [(&str, FilterKind); 3] = [
    ("lowpass", FilterKind::Lowpass),
    ("highpass", FilterKind::Highpass),
    ("bandpass", FilterKind::Bandpass),
];

fn filter_freq(v: &Value, path: &str, notes: &mut Notes) -> Option<f64> {
    ranged(v, path, 20.0, 20_000.0, notes)
}

/// `{ type, freq, q }`; `kind` is given when the type is already known
/// (an effect's). A filter with no `freq` is no filter.
fn filter(t: &Table, path: &str, kind: Option<FilterKind>, notes: &mut Notes) -> Option<Filter> {
    let kind = match kind {
        Some(kind) => kind,
        None => {
            unknown_keys(t, path, &["type", "freq", "q"], false, notes);
            let v = get(t, "type");
            let found = word(&v, &format!("{path}.type"), &FILTERS, notes);
            if matches!(v, Value::Nil) {
                notes.push(
                    &format!("{path}.type"),
                    "a filter needs a type: \"lowpass\", \"highpass\" or \"bandpass\"",
                );
            }
            found?
        }
    };
    let v = get(t, "freq");
    if matches!(v, Value::Nil) {
        notes.push(&format!("{path}.freq"), "a filter needs a freq");
    }
    let freq = glide(&v, &format!("{path}.freq"), filter_freq, notes)?;
    let q = ranged(&get(t, "q"), &format!("{path}.q"), 0.1, 30.0, notes).unwrap_or(0.707);
    Some(Filter { kind, freq, q })
}

fn lfo(t: &Table, path: &str, most: f64, notes: &mut Notes) -> Lfo {
    unknown_keys(t, path, &["rate", "depth"], false, notes);
    Lfo {
        rate: at_least(&get(t, "rate"), &format!("{path}.rate"), 0.0, notes).unwrap_or(0.0),
        depth: ranged(&get(t, "depth"), &format!("{path}.depth"), 0.0, most, notes).unwrap_or(0.0),
    }
}

fn env(t: &Table, path: &str, notes: &mut Notes) -> Env {
    unknown_keys(
        t,
        path,
        &["attack", "decay", "sustain", "hold", "release", "curve"],
        false,
        notes,
    );
    let base = Env::default();
    let seconds = |key: &str, default: f64, notes: &mut Notes| {
        at_least(&get(t, key), &format!("{path}.{key}"), 0.0, notes).unwrap_or(default)
    };
    Env {
        attack: seconds("attack", base.attack, notes),
        decay: seconds("decay", base.decay, notes),
        sustain: ranged(
            &get(t, "sustain"),
            &format!("{path}.sustain"),
            0.0,
            1.0,
            notes,
        )
        .unwrap_or(base.sustain),
        hold: seconds("hold", base.hold, notes),
        release: seconds("release", base.release, notes),
        curve: word(&get(t, "curve"), &format!("{path}.curve"), &CURVES, notes)
            .unwrap_or(base.curve),
    }
}

const WAVES: [(&str, Wave); 5] = [
    ("sine", Wave::Sine),
    ("triangle", Wave::Triangle),
    ("square", Wave::Square),
    ("saw", Wave::Saw),
    ("noise", Wave::Noise),
];

fn layer(t: &Table, path: &str, notes: &mut Notes) -> Layer {
    unknown_keys(
        t,
        path,
        &[
            "wave", "freq", "detune", "width", "phase", "seed", "fm", "vibrato", "tremolo",
            "filter", "env", "gain", "pan", "at",
        ],
        false,
        notes,
    );
    let at = |key: &str| format!("{path}.{key}");
    let mut l = Layer::default();
    if let Some(wave) = word(&get(t, "wave"), &at("wave"), &WAVES, notes) {
        l.wave = wave;
    }
    // Noise with no `freq` is white noise; anything else at no `freq` is
    // 440 Hz.
    let freq = glide(&get(t, "freq"), &at("freq"), pitch, notes);
    l.freq = match (freq, l.wave) {
        (Some(g), _) => Some(g),
        (None, Wave::Noise) => None,
        (None, _) => l.freq,
    };
    if let Some(v) = number(&get(t, "detune"), &at("detune"), notes) {
        l.detune = v;
    }
    if let Some(v) = ranged(&get(t, "width"), &at("width"), 0.05, 0.95, notes) {
        l.width = v;
    }
    if let Some(v) = ranged(&get(t, "phase"), &at("phase"), 0.0, 1.0, notes) {
        l.phase = v;
    }
    match get(t, "seed") {
        Value::Nil => {}
        Value::Integer(i) if u32::try_from(i).is_ok() => l.seed = i as u32,
        Value::Number(n) if n.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&n) => {
            l.seed = n as u32;
        }
        other => wrong(
            &at("seed"),
            "a whole number from 0 to 4294967295",
            &other,
            notes,
        ),
    }
    if let Some(fm) = table(&get(t, "fm"), &at("fm"), "{ ratio, index }", notes) {
        let path = at("fm");
        unknown_keys(fm, &path, &["ratio", "index"], false, notes);
        l.fm = Some(Fm {
            ratio: number(&get(fm, "ratio"), &format!("{path}.ratio"), notes).unwrap_or(1.0),
            index: glide(&get(fm, "index"), &format!("{path}.index"), plain, notes)
                .unwrap_or(Glide::fixed(0.0)),
        });
    }
    if let Some(v) = table(&get(t, "vibrato"), &at("vibrato"), "{ rate, depth }", notes) {
        l.vibrato = Some(lfo(v, &at("vibrato"), f64::MAX, notes));
    }
    if let Some(v) = table(&get(t, "tremolo"), &at("tremolo"), "{ rate, depth }", notes) {
        l.tremolo = Some(lfo(v, &at("tremolo"), 1.0, notes));
    }
    if let Some(f) = table(&get(t, "filter"), &at("filter"), "{ type, freq, q }", notes) {
        l.filter = filter(f, &at("filter"), None, notes);
    }
    if let Some(e) = table(&get(t, "env"), &at("env"), "an envelope table", notes) {
        l.env = env(e, &at("env"), notes);
    }
    if let Some(v) = number(&get(t, "gain"), &at("gain"), notes) {
        l.gain = v;
    }
    if let Some(v) = ranged(&get(t, "pan"), &at("pan"), -1.0, 1.0, notes) {
        l.pan = v;
    }
    if let Some(v) = at_least(&get(t, "at"), &at("at"), 0.0, notes) {
        l.at = v;
    }
    l
}

/// What an effect is without one of its numbers: as the chapter's table
/// of effects gives them.
mod effect_defaults {
    pub const DELAY_TIME: f64 = 0.25;
    pub const DELAY_FEEDBACK: f64 = 0.35;
    pub const DELAY_MIX: f64 = 0.3;
    pub const REVERB_SIZE: f64 = 0.5;
    pub const REVERB_DAMP: f64 = 0.5;
    pub const REVERB_MIX: f64 = 0.3;
    pub const DRIVE_AMOUNT: f64 = 0.5;
    pub const CRUSH_BITS: f64 = 8.0;
    pub const CRUSH_RATE: f64 = 12_000.0;
}

#[derive(Clone, Copy)]
enum EffectKind {
    Delay,
    Reverb,
    Drive,
    Crush,
    Filter(FilterKind),
}

const EFFECTS: [(&str, EffectKind); 7] = [
    ("delay", EffectKind::Delay),
    ("reverb", EffectKind::Reverb),
    ("drive", EffectKind::Drive),
    ("crush", EffectKind::Crush),
    ("lowpass", EffectKind::Filter(FilterKind::Lowpass)),
    ("highpass", EffectKind::Filter(FilterKind::Highpass)),
    ("bandpass", EffectKind::Filter(FilterKind::Bandpass)),
];

fn effect(t: &Table, path: &str, notes: &mut Notes) -> Option<Effect> {
    use effect_defaults::*;
    let v = get(t, "type");
    if matches!(v, Value::Nil) {
        notes.push(&format!("{path}.type"), "an effect needs a type");
        return None;
    }
    let kind = word(&v, &format!("{path}.type"), &EFFECTS, notes)?;
    let unit = |key: &str, min: f64, max: f64, default: f64, notes: &mut Notes| {
        ranged(&get(t, key), &format!("{path}.{key}"), min, max, notes).unwrap_or(default)
    };
    Some(match kind {
        EffectKind::Delay => {
            unknown_keys(
                t,
                path,
                &["type", "time", "feedback", "mix", "ping_pong"],
                false,
                notes,
            );
            let ping_pong = match get(t, "ping_pong") {
                Value::Nil => false,
                Value::Boolean(b) => b,
                other => {
                    wrong(&format!("{path}.ping_pong"), "true or false", &other, notes);
                    false
                }
            };
            Effect::Delay {
                time: unit("time", 0.001, 1.0, DELAY_TIME, notes),
                feedback: unit("feedback", 0.0, 0.9, DELAY_FEEDBACK, notes),
                mix: unit("mix", 0.0, 1.0, DELAY_MIX, notes),
                ping_pong,
            }
        }
        EffectKind::Reverb => {
            unknown_keys(t, path, &["type", "size", "damp", "mix"], false, notes);
            Effect::Reverb {
                size: unit("size", 0.0, 1.0, REVERB_SIZE, notes),
                damp: unit("damp", 0.0, 1.0, REVERB_DAMP, notes),
                mix: unit("mix", 0.0, 1.0, REVERB_MIX, notes),
            }
        }
        EffectKind::Drive => {
            unknown_keys(t, path, &["type", "amount"], false, notes);
            Effect::Drive {
                amount: unit("amount", 0.0, 1.0, DRIVE_AMOUNT, notes),
            }
        }
        EffectKind::Crush => {
            unknown_keys(t, path, &["type", "bits", "rate"], false, notes);
            Effect::Crush {
                bits: unit("bits", 1.0, 16.0, CRUSH_BITS, notes),
                rate: unit("rate", 100.0, 48_000.0, CRUSH_RATE, notes),
            }
        }
        EffectKind::Filter(kind) => {
            unknown_keys(t, path, &["type", "freq", "q"], false, notes);
            Effect::Filter(filter(t, path, Some(kind), notes)?)
        }
    })
}

/// A recipe as the script wrote it, checked. `None`: the sound is silent
/// (it is no recipe, or has no layers, or more layers or effects than a
/// recipe may).
fn read_recipe(v: &Value, notes: &mut Notes) -> Option<Recipe> {
    let Value::Table(t) = v else {
        wrong("", "a recipe: { layers = { … } }", v, notes);
        return None;
    };
    unknown_keys(
        t,
        "",
        &["gain", "length", "layers", "effects"],
        false,
        notes,
    );
    let mut recipe = Recipe::default();
    if let Some(v) = ranged(&get(t, "gain"), ".gain", 0.0, 1.0, notes) {
        recipe.gain = v;
    }
    recipe.length = ranged(&get(t, "length"), ".length", 0.0, limits::SECONDS, notes);

    let layers = get(t, "layers");
    let Value::Table(list) = &layers else {
        wrong(".layers", "a list of 1 to 8 layers", &layers, notes);
        return None;
    };
    let n = list.raw_len();
    if n == 0 || n > limits::LAYERS {
        notes.push(
            ".layers",
            format!(
                "{n} layers; a recipe has 1 to {}. The sound is silent",
                limits::LAYERS
            ),
        );
        return None;
    }
    for i in 1..=n {
        let path = format!(".layers[{i}]");
        let v = list.raw_get::<Value>(i).unwrap_or(Value::Nil);
        match &v {
            Value::Table(l) => recipe.layers.push(layer(l, &path, notes)),
            other => wrong(&path, "a layer table", other, notes),
        }
    }
    if recipe.layers.is_empty() {
        return None;
    }

    match get(t, "effects") {
        Value::Nil => {}
        Value::Table(list) => {
            let n = list.raw_len();
            if n > limits::EFFECTS {
                notes.push(
                    ".effects",
                    format!(
                        "{n} effects; a recipe has at most {}. The sound is silent",
                        limits::EFFECTS
                    ),
                );
                return None;
            }
            for i in 1..=n {
                let path = format!(".effects[{i}]");
                let v = list.raw_get::<Value>(i).unwrap_or(Value::Nil);
                match &v {
                    Value::Table(e) => recipe.effects.extend(effect(e, &path, notes)),
                    other => wrong(&path, "an effect table", other, notes),
                }
            }
        }
        other => wrong(".effects", "a list of effects", &other, notes),
    }
    Some(recipe)
}

/// `gmnb.sound(file, { gain })`.
fn read_file(file: &Value, options: &Value, notes: &mut Notes) -> Declared {
    let name = match file {
        Value::String(s) => s.to_str().ok().map(|s| s.to_string()),
        _ => None,
    };
    let Some(name) = name.filter(|n| safe_relative(n)) else {
        wrong("", "a file inside the theme's folder", file, notes);
        return Declared::Silent;
    };
    let gain = match options {
        Value::Nil => None,
        Value::Table(t) => {
            unknown_keys(t, "", &["gain"], false, notes);
            ranged(&get(t, "gain"), ".gain", 0.0, 1.0, notes)
        }
        other => {
            wrong("", "options: { gain = 0.5 }", other, notes);
            None
        }
    };
    Declared::File { name, gain }
}

// ---- The `sounds` table, and what `sound` returns --------------------------------
//
// These arrive converted (`ThemeValue`), like the rest of the theme table.

/// A Sound: what `gmnb.synth` and `gmnb.sound` return.
fn sound_id(v: &ThemeValue) -> Option<SoundId> {
    let ThemeValue::Map(m) = v else {
        return None;
    };
    if m.get("kind").and_then(ThemeValue::as_str) != Some("sound") {
        return None;
    }
    match m.get("id") {
        Some(ThemeValue::Int(id)) if (0..=limits::SOUNDS as i64).contains(id) => {
            Some(SoundId(*id as u32))
        }
        _ => None,
    }
}

/// A number in `min..=max`, clamped with a note.
fn bounded(v: &ThemeValue, path: &str, min: f32, max: f32, notes: &mut Notes) -> Option<f32> {
    crate::check::number(v, path, f64::from(min), f64::from(max), notes).map(|n| n as f32)
}

/// A number, or a range `{ low, high }`.
fn varied(v: &ThemeValue, path: &str, min: f32, max: f32, notes: &mut Notes) -> Option<Varied> {
    match v {
        ThemeValue::Array(pair) if pair.len() == 2 => {
            let low = bounded(&pair[0], &format!("{path}[1]"), min, max, notes)?;
            let high = bounded(&pair[1], &format!("{path}[2]"), min, max, notes)?;
            Some(Varied::Range(low.min(high), low.max(high)))
        }
        ThemeValue::Array(_) | ThemeValue::Map(_) => {
            notes.push(path, "expected a number or a range { low, high }");
            None
        }
        other => bounded(other, path, min, max, notes).map(Varied::One),
    }
}

/// The volume, rate, pan and delay of an entry or a play, onto `entry`.
/// `ranges`: whether a value may be a range (in the table) or must be one
/// number (from `sound`).
fn play_fields(v: &ThemeValue, path: &str, ranges: bool, entry: &mut Entry, notes: &mut Notes) {
    let field = |key: &str, min: f32, max: f32, notes: &mut Notes| -> Option<Varied> {
        let value = v.get(key)?;
        let path = format!("{path}.{key}");
        if ranges {
            varied(value, &path, min, max, notes)
        } else {
            bounded(value, &path, min, max, notes).map(Varied::One)
        }
    };
    if let Some(volume) = field("volume", 0.0, 1.0, notes) {
        entry.volume = volume;
    }
    let (slow, fast) = limits::PLAY_RATE;
    if let Some(rate) = field("rate", slow, fast, notes) {
        entry.rate = rate;
    }
    match v.get("pan") {
        Some(ThemeValue::String(s)) if ranges && s == "position" => entry.pan = Pan::Position,
        Some(ThemeValue::String(_)) if ranges => notes.push(
            &format!("{path}.pan"),
            "expected a number, a range or \"position\"",
        ),
        _ => {
            if let Some(pan) = field("pan", -1.0, 1.0, notes) {
                entry.pan = Pan::At(pan);
            }
        }
    }
    if let Some(delay) = v.get("delay") {
        let path = format!("{path}.delay");
        if let Some(delay) = bounded(delay, &path, 0.0, limits::PLAY_DELAY, notes) {
            entry.delay = delay;
        }
    }
}

fn unknown(v: &ThemeValue, path: &str, known: &[&str], notes: &mut Notes) {
    if let ThemeValue::Map(m) = v {
        for key in m.keys() {
            if !known.contains(&key.as_str()) {
                notes.push(&format!("{path}.{key}"), "unknown key");
            }
        }
    }
}

/// One entry of the `sounds` table.
fn slot(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Slot> {
    if let ThemeValue::Bool(false) = v {
        return Some(Slot::Silent);
    }
    if let Some(id) = sound_id(v) {
        return Some(Slot::Entry(Entry::plain(id)));
    }
    let Some(given) = v.get("sound") else {
        notes.push(
            path,
            "expected a sound, false, or { sound = …, rate = …, … }",
        );
        return None;
    };
    unknown(
        v,
        path,
        &["sound", "pick", "volume", "rate", "pan", "delay"],
        notes,
    );
    let sounds: Vec<SoundId> = match (sound_id(given), given) {
        (Some(id), _) => vec![id],
        (None, ThemeValue::Array(list)) => {
            if list.len() > limits::ENTRY_SOUNDS {
                notes.push(
                    &format!("{path}.sound"),
                    format!(
                        "{} sounds; an entry chooses from at most {}. The rest are left out",
                        list.len(),
                        limits::ENTRY_SOUNDS
                    ),
                );
            }
            let mut ids = Vec::new();
            for (i, s) in list.iter().take(limits::ENTRY_SOUNDS).enumerate() {
                match sound_id(s) {
                    Some(id) => ids.push(id),
                    None => notes.push(&format!("{path}.sound[{}]", i + 1), "expected a sound"),
                }
            }
            ids
        }
        _ => Vec::new(),
    };
    if sounds.is_empty() {
        notes.push(
            &format!("{path}.sound"),
            "expected a sound or a list of sounds",
        );
        return None;
    }
    let mut entry = Entry {
        sounds,
        ..Entry::plain(SoundId::NONE)
    };
    if let Some(pick) = v.get("pick") {
        let words = &["cycle", "random", "key"];
        entry.pick = match crate::check::word(pick, &format!("{path}.pick"), words, notes) {
            Some("random") => Pick::Random,
            Some("key") => Pick::Key,
            _ => Pick::Cycle,
        };
    }
    play_fields(v, path, true, &mut entry, notes);
    Some(Slot::Entry(entry))
}

/// The theme table's `sounds`, checked: which sound for which event.
pub fn parse_table(v: Option<&ThemeValue>, notes: &mut Notes) -> SoundTable {
    let mut table = SoundTable::default();
    let entries = match v {
        None => return table,
        Some(ThemeValue::Map(entries)) => entries,
        Some(_) => {
            notes.push("sounds", "expected a table of sounds by event");
            return table;
        }
    };
    for (name, entry) in entries {
        let path = format!("sounds.{name}");
        match EventName::parse(name) {
            Some(event) => {
                if let Some(slot) = slot(entry, &path, notes) {
                    table.set(event, slot);
                }
            }
            None => notes.push(&path, "unknown event"),
        }
    }
    table
}

/// What a `sound(event, ctx)` call decided.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    /// `nil` (or something unusable): the `sounds` table decides, as if
    /// there were no function.
    Table,
    /// `false`: play nothing.
    Silence,
    /// Play these together: 1 to 4.
    Plays(Vec<Play>),
}

/// One play: a sound, or `{ sound = …, volume = …, rate = …, pan = …,
/// delay = … }` with single numbers.
fn play(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Play> {
    let mut entry = Entry::plain(SoundId::NONE);
    let sound = match sound_id(v) {
        Some(id) => id,
        None => {
            let id = v.get("sound").and_then(sound_id);
            if id.is_none() {
                notes.push(path, "expected a sound or { sound = …, volume = …, … }");
            }
            unknown(v, path, &["sound", "volume", "rate", "pan", "delay"], notes);
            play_fields(v, path, false, &mut entry, notes);
            id?
        }
    };
    let one = |v: Varied| match v {
        Varied::One(x) | Varied::Range(x, _) => x,
    };
    let pan = match entry.pan {
        Pan::At(v) => one(v),
        Pan::Position => 0.0,
    };
    Some(Play {
        sound,
        params: PlayParams {
            volume: one(entry.volume),
            rate: one(entry.rate),
            pan,
            delay: entry.delay,
        },
    })
}

/// Checks what a theme's `sound` function returned.
pub fn parse_choice(v: &ThemeValue, notes: &mut Notes) -> Choice {
    match v {
        ThemeValue::Nil => Choice::Table,
        ThemeValue::Bool(false) => Choice::Silence,
        ThemeValue::Array(list) => {
            if list.len() > limits::PLAYS_PER_EVENT {
                notes.push(
                    "sound()",
                    format!(
                        "{} plays; an event starts at most {}. The rest are left out",
                        list.len(),
                        limits::PLAYS_PER_EVENT
                    ),
                );
            }
            let plays: Vec<Play> = list
                .iter()
                .take(limits::PLAYS_PER_EVENT)
                .enumerate()
                .filter_map(|(i, p)| play(p, &format!("sound()[{}]", i + 1), notes))
                .collect();
            if plays.is_empty() {
                Choice::Table
            } else {
                Choice::Plays(plays)
            }
        }
        // The empty table is an empty list: nothing chosen.
        ThemeValue::Map(m) if m.is_empty() => Choice::Table,
        other => match play(other, "sound()", notes) {
            Some(play) => Choice::Plays(vec![play]),
            None => Choice::Table,
        },
    }
}
