//! Sound in a theme: recipes written in Luau come out as the samples the
//! chapter promises; what a theme gets wrong is noted and made harmless;
//! the `sounds` table and what `sound` returns are checked.

#[path = "../../gmnb-sound/tests/common/mod.rs"]
mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use gmnb_sound::{
    Effect, Entry, EventName, FileData, Pan, Pick, Recipe, Slot, SoundId, SoundSource, Varied,
    Wave, limits, prepare_all, render,
};
use gmnb_theme::sound::{Choice, parse_choice};
use gmnb_theme::theme::{Source, Theme};
use gmnb_theme::{Limits, Notes, ThemeValue};

/// A theme folder of this test's own, with `source` as its theme.luau.
struct Folder(PathBuf);

impl Folder {
    fn new(source: &str) -> Folder {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gmnb-theme-sound-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        std::fs::write(dir.join("theme.luau"), source).unwrap();
        Folder(dir)
    }

    fn load(&self) -> Theme {
        Theme::load(Source::folder("test", &self.0), Limits::default())
            .unwrap_or_else(|e| panic!("the theme didn't load: {e}"))
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Loads a theme whose file is `body` followed by a plain theme table.
fn theme(body: &str) -> (Theme, Folder) {
    let folder = Folder::new(&format!("{body}\nreturn {{ api = 1, name = \"T\" }}"));
    (folder.load(), folder)
}

fn recipes(theme: &Theme) -> Vec<Recipe> {
    theme
        .sounds()
        .sounds
        .into_iter()
        .map(|s| match s {
            SoundSource::Synth(r) => r,
            other => panic!("not a recipe: {other:?}"),
        })
        .collect()
}

fn notes(theme: &Theme) -> Vec<String> {
    theme
        .take_sound_notes()
        .iter()
        .map(|n| n.to_string())
        .collect()
}

// ---- The chapter's recipes, as it writes them -----------------------------------

/// docs/theming-media.md, "Test vectors", the Luau as printed there (it
/// was transcribed by hand from the reference's Rust and not run).
const CHAPTER: &str = r#"
local click = { gain = 0.5, layers = {
	{ freq = { 950, 420, time = 0.03 }, env = { attack = 0.0005, decay = 0.045 } },
	{ freq = { 2400, 1800, time = 0.012 }, env = { attack = 0, decay = 0.015 }, gain = 0.15 },
	{ wave = "noise", seed = 1, filter = { type = "bandpass", freq = 3800, q = 1.2 },
	  env = { attack = 0, decay = 0.012 }, gain = 0.35 },
} }

local function bell(note, at, pan, gain)
	return { freq = note, fm = { ratio = 3.5, index = { 2.6, 0.05, time = 0.35 } },
	         env = { attack = 0.002, decay = 0.9 }, at = at, pan = pan, gain = gain }
end
local chime = { gain = 0.4, length = 1.6,
	layers = { bell("E6", 0, -0.3, 1), bell("G#6", 0.075, 0.3, 0.85),
	           { freq = "E7", detune = 7, env = { attack = 0.004, decay = 0.6 }, gain = 0.2 } },
	effects = { { type = "reverb", size = 0.55, damp = 0.3, mix = 0.3 } } }

local function growl(freq, at)
	local env = { attack = 0.004, decay = 0.02, sustain = 0.8, hold = 0.07, release = 0.04 }
	local fall = { freq, freq * 0.94, time = 0.12, curve = "linear" }
	return { wave = "square", width = 0.35, freq = fall, env = env, at = at,
	         filter = { type = "lowpass", freq = 900, q = 0.8 } },
	       { wave = "saw", freq = fall, detune = 14, env = env, at = at, gain = 0.5,
	         filter = { type = "lowpass", freq = 700, q = 0.7 } }
end
local a1, a2 = growl(116.5, 0)
local b1, b2 = growl(98, 0.15)
local buzz = { gain = 0.42, layers = { a1, a2, b1, b2 },
	effects = { { type = "drive", amount = 0.25 } } }
"#;

#[test]
fn the_chapters_luau_recipes_give_the_chapters_hashes() {
    let (theme, _folder) = theme(&format!(
        "{CHAPTER}\nlocal s = {{ gmnb.synth(click), gmnb.synth(chime), gmnb.synth(buzz) }}"
    ));
    assert_eq!(notes(&theme), Vec::<String>::new());
    let made = recipes(&theme);
    assert_eq!(made.len(), 3);
    // The recipes read from Luau are the reference's own values...
    assert_eq!(made[0], common::click_soft());
    assert_eq!(made[1], common::chime_glass());
    assert_eq!(made[2], common::error_buzz());
    // ...and render to the chapter's test vectors.
    for (recipe, frames, sha) in [
        (
            &made[0],
            2184,
            "21c869ae7b1c53cb7049330a055fc4e7be80f991af3e04719b7ceaa69b5ce816",
        ),
        (
            &made[1],
            65_624,
            "767b821cea4900be32d7dac2bc441c0f6d000eabe1753ed477e1064f6a118dd1",
        ),
        (
            &made[2],
            13_632,
            "02c74414c9ac4e7a4180a032de47c27634d14f41983af170d484e2d9bbcd1579",
        ),
    ] {
        let samples = render(recipe);
        assert_eq!(samples.len(), frames * 2);
        assert_eq!(common::hash(&samples), sha);
    }
}

/// The ten example sounds as the example theme writes them
/// (crates/gmnb-sound/examples/example-sounds/theme.luau).
#[test]
fn the_example_themes_sounds_are_the_designs_ten() {
    let folder = Folder::new(include_str!(
        "../../gmnb-sound/examples/example-sounds/theme.luau"
    ));
    let theme = folder.load();
    assert_eq!(notes(&theme), Vec::<String>::new());
    assert!(theme.notes.is_empty(), "{:?}", theme.notes);
    let made = recipes(&theme);
    let expected = common::sounds();
    assert_eq!(made.len(), expected.len());
    for (got, (name, want)) in made.iter().zip(&expected) {
        assert_eq!(got, want, "{name}");
    }
    let hashes: Vec<String> = prepare_all(&theme.sounds().sounds)
        .iter()
        .map(|s| common::hash(s.as_ref().unwrap()))
        .collect();
    assert_eq!(
        hashes,
        [
            "21c869ae7b1c53cb7049330a055fc4e7be80f991af3e04719b7ceaa69b5ce816",
            "028a27b50a1d472fbf2256664b35312c3dd892ea5c6feab1647ced516fe21d6d",
            "767b821cea4900be32d7dac2bc441c0f6d000eabe1753ed477e1064f6a118dd1",
            "02c74414c9ac4e7a4180a032de47c27634d14f41983af170d484e2d9bbcd1579",
            "5cfc54bfc7502b15cad28a86a657821eb084c3942f95112c601e253745ce65b8",
            "d3574b812dabd00fb4a1d7499c3b210f444806dd018c347e612c77b5d400aa72",
            "89d243d184fa4500c362abe06bf109f24407aaa1f61e254f17d9003126494821",
            "945360c5e814fbb417c06289e9872e5b46265e775ae8737c45e053f9c601c8c8",
            "2da49fbc9e754d97ed782346f8f94169348ebe587defeee4d926ef7dce804038",
            "f3c7d902b905bd29936bf9e96f4299678d9db7bf2449372a2f12849606317e3e",
        ]
    );
    // Its table: every kind of entry.
    let sounds = theme.sounds();
    assert!(sounds.any() && !sounds.hook);
    let table = &sounds.table;
    let (found, key) = table.find(EventName::Digit).unwrap();
    assert_eq!(found, EventName::Key);
    assert_eq!(key.pick, Pick::Key);
    assert_eq!(key.pan, Pan::Position);
    assert_eq!(key.rate, Varied::Range(0.94, 1.06));
    assert_eq!(
        table.find(EventName::Equals).unwrap().1.sounds,
        [SoundId(5)]
    );
    assert_eq!(
        table.find(EventName::Toggle).unwrap().1.sounds,
        [SoundId(7), SoundId(8)]
    );
    assert_eq!(table.find(EventName::Result).unwrap().1.delay, 0.06);
    assert_eq!(table.find(EventName::Memory).unwrap().0, EventName::Button);
}

/// The chapter's opening example ("A living background and key sounds"):
/// a two-layer click and a two-bell chime. Their samples are pinned here.
#[test]
fn the_chapters_opening_example() {
    let (theme, _folder) = theme(
        r#"
local click = gmnb.synth({
	gain = 0.5,
	layers = {
		{ freq = { 950, 420, time = 0.03 }, env = { attack = 0.0005, decay = 0.045 } },
		{ wave = "noise", seed = 1, filter = { type = "bandpass", freq = 3800, q = 1.2 },
		  env = { attack = 0, decay = 0.012 }, gain = 0.35 },
	},
})
local function bell(note, at, pan)
	return { freq = note, fm = { ratio = 3.5, index = { 2.6, 0.05, time = 0.35 } },
	         env = { attack = 0.002, decay = 0.9 }, at = at, pan = pan }
end
local chime = gmnb.synth({
	gain = 0.4, length = 1.6,
	layers = { bell("E6", 0, -0.3), bell("G#6", 0.075, 0.3) },
	effects = { { type = "reverb", size = 0.55, damp = 0.3, mix = 0.3 } },
})
"#,
    );
    assert_eq!(notes(&theme), Vec::<String>::new());
    let made = prepare_all(&theme.sounds().sounds);
    let got: Vec<(usize, String)> = made
        .iter()
        .map(|s| {
            let s = s.as_ref().unwrap();
            (s.len() / 2, common::hash(s))
        })
        .collect();
    assert_eq!(
        got,
        [
            (
                2184,
                "bcb5fc8d40bd3c4c30635406a15139085b6885373ec5b2aea28a77a2ec9e2be3".to_string()
            ),
            (
                65_914,
                "50fff1b3296d0b7de2026c01160bac13fb0717a3c4ccde259bfa84ae4cd32fc7".to_string()
            ),
        ]
    );
}

// ---- gmnb.synth: what it makes of a recipe ----------------------------------------

#[test]
fn defaults_are_the_chapters() {
    let (theme, _folder) = theme(
        r#"
local s = {
	gmnb.synth({ layers = { {} } }),
	gmnb.synth({ layers = { { wave = "noise" } } }),
	gmnb.synth({ layers = { { wave = "noise", freq = 800 } } }),
	gmnb.synth({ layers = { { freq = "A4" } }, effects = {
		{ type = "delay" }, { type = "reverb" }, { type = "drive" }, { type = "crush" } } }),
}"#,
    );
    assert_eq!(notes(&theme), Vec::<String>::new());
    let made = recipes(&theme);
    // A layer of nothing is a 440 Hz sine pluck, 0.102 s long, at gain 0.5.
    let plain = Recipe {
        layers: vec![gmnb_sound::Layer::default()],
        ..Recipe::default()
    };
    assert_eq!(made[0], plain);
    assert_eq!(made[0].gain, 0.5);
    assert_eq!(render(&made[0]).len(), 4896 * 2);
    // Noise without a pitch is white; with one, held.
    assert_eq!(
        (made[1].layers[0].wave, made[1].layers[0].freq),
        (Wave::Noise, None)
    );
    assert_eq!(made[2].layers[0].freq.unwrap().from, 800.0);
    // "A4" is 440: the same sound as the first, plus its effects.
    assert_eq!(made[3].layers, plain.layers);
    assert_eq!(
        made[3].effects,
        [
            Effect::Delay {
                time: 0.25,
                feedback: 0.35,
                mix: 0.3,
                ping_pong: false
            },
            Effect::Reverb {
                size: 0.5,
                damp: 0.5,
                mix: 0.3
            },
            Effect::Drive { amount: 0.5 },
            Effect::Crush {
                bits: 8.0,
                rate: 12_000.0
            },
        ]
    );
}

#[test]
fn the_same_recipe_is_the_same_sound() {
    let (theme, _folder) = theme(
        r#"
local function click() return { layers = { { freq = { 900, 400, time = 0.03 } } } } end
local a, b = gmnb.synth(click()), gmnb.synth(click())
assert(rawequal(a, b), "the same recipe gave another sound")
assert(a.kind == "sound" and a.id == 1)
assert(not pcall(function() a.id = 5 end), "a sound can be written to")
local c = gmnb.synth({ layers = { { freq = { 900, 401, time = 0.03 } } } })
assert(c.id == 2 and not rawequal(a, c))
-- Asked for a hundred times (a hook that makes its sound each call): one.
for _ = 1, 100 do assert(rawequal(gmnb.synth(click()), a)) end
"#,
    );
    assert_eq!(theme.sounds_declared(), 2);
    assert_eq!(notes(&theme), Vec::<String>::new());
}

#[test]
fn what_is_wrong_in_a_recipe_is_noted_and_defaulted() {
    let (theme, _folder) = theme(
        r#"
gmnb.synth({
	gain = 3, length = 9, volume = 1,
	layers = {
		{ wave = "sinus", freq = "H9", detune = "flat", width = 2, phase = -1, seed = 1.5,
		  fm = 7, vibrato = { rate = -3, depth = 20, speed = 1 }, tremolo = { rate = 4, depth = 5 },
		  filter = { type = "lowpass", freq = 5, q = 100 },
		  env = { attack = -1, decay = "long", sustain = 2, curve = "log" },
		  gain = 0/0, pan = -7, at = -2, loud = true },
		{ freq = { 100, 0, time = 0.1 }, filter = { freq = 1000 } },
		{ freq = { 100 } },
		"layer",
	},
	effects = {
		{ type = "delay", time = 5, feedback = 2, mix = -1, ping_pong = "yes" },
		{ type = "echo" },
		{ size = 1 },
		{ type = "crush", bits = 0, rate = 5, amount = 1 },
	},
})"#,
    );
    let said = notes(&theme);
    let has = |path: &str, part: &str| {
        assert!(
            said.iter()
                .any(|n| n.starts_with(&format!("sound[1]{path}: ")) && n.contains(part)),
            "no note at {path} with {part:?} in:\n{}",
            said.join("\n")
        );
    };
    has(".gain", "out of range; using 1");
    has(".length", "out of range; using 4");
    has(".volume", "unknown key");
    has(".layers[1].wave", "one of \"sine\"");
    has(".layers[1].freq", "note name");
    has(".layers[1].detune", "expected a number");
    has(".layers[1].width", "using 0.95");
    has(".layers[1].phase", "using 0");
    has(".layers[1].seed", "whole number");
    has(".layers[1].fm", "{ ratio, index }");
    has(".layers[1].vibrato.rate", "using 0");
    has(".layers[1].vibrato.speed", "unknown key");
    has(".layers[1].tremolo.depth", "using 1");
    has(".layers[1].filter.freq", "using 20");
    has(".layers[1].filter.q", "using 30");
    has(".layers[1].env.attack", "using 0");
    has(".layers[1].env.decay", "expected a number");
    has(".layers[1].env.sustain", "using 1");
    has(".layers[1].env.curve", "\"exp\"");
    has(".layers[1].gain", "expected a number");
    has(".layers[1].pan", "using -1");
    has(".layers[1].at", "using 0");
    has(".layers[1].loud", "unknown key");
    has(".layers[2].freq", "needs both values above zero");
    has(".layers[2].filter.type", "needs a type");
    has(".layers[3].freq", "a glide is");
    has(".layers[4]", "expected a layer table");
    has(".effects[1].time", "using 1");
    has(".effects[1].feedback", "using 0.9");
    has(".effects[1].mix", "using 0");
    has(".effects[1].ping_pong", "true or false");
    has(".effects[2].type", "one of \"delay\"");
    has(".effects[3].type", "needs a type");
    has(".effects[4].bits", "using 1");
    has(".effects[4].rate", "using 100");
    has(".effects[4].amount", "unknown key");

    // And what is left is a recipe that plays: every default in place.
    let r = &recipes(&theme)[0];
    assert_eq!((r.gain, r.length), (1.0, Some(4.0)));
    assert_eq!(r.layers.len(), 3);
    let l = &r.layers[0];
    let base = gmnb_sound::Layer::default();
    assert_eq!((l.wave, l.freq, l.detune), (base.wave, base.freq, 0.0));
    assert_eq!((l.width, l.phase, l.seed), (0.95, 0.0, base.seed));
    assert_eq!(l.fm, None);
    assert_eq!(l.vibrato.unwrap().rate, 0.0);
    assert_eq!(l.tremolo.unwrap().depth, 1.0);
    let f = l.filter.unwrap();
    assert_eq!((f.freq.from, f.q), (20.0, 30.0));
    assert_eq!((l.env.attack, l.env.decay, l.env.sustain), (0.0, 0.1, 1.0));
    assert_eq!((l.gain, l.pan, l.at), (1.0, -1.0, 0.0));
    // The glide to 0 can't move by equal steps: it goes in a straight line.
    assert_eq!(r.layers[1].freq.unwrap().curve, gmnb_sound::Curve::Linear);
    assert_eq!(r.layers[1].filter, None);
    assert_eq!(r.layers[2].freq, base.freq);
    assert_eq!(
        r.effects,
        [
            Effect::Delay {
                time: 1.0,
                feedback: 0.9,
                mix: 0.0,
                ping_pong: false
            },
            Effect::Crush {
                bits: 1.0,
                rate: 100.0
            },
        ]
    );
    assert!(!render(r).is_empty());
}

#[test]
fn a_recipe_past_the_limits_is_silent() {
    let (theme, _folder) = theme(
        r#"
local layer = { freq = 440 }
local sounds = {
	gmnb.synth({ layers = {} }),
	gmnb.synth({ layers = { layer, layer, layer, layer, layer, layer, layer, layer, layer } }),
	gmnb.synth({ layers = { layer }, effects = {
		{ type = "drive" }, { type = "drive" }, { type = "drive" }, { type = "drive" }, { type = "drive" } } }),
	gmnb.synth({}),
	gmnb.synth("click"),
	gmnb.synth(),
	gmnb.synth({ layers = { 1, 2 } }),
	gmnb.synth({ layers = { layer, layer, layer, layer, layer, layer, layer, layer } }),
}
for i, s in sounds do assert(s.kind == "sound" and s.id == i) end
"#,
    );
    let sounds = theme.sounds().sounds;
    assert_eq!(sounds.len(), 8);
    assert!(sounds[..7].iter().all(|s| *s == SoundSource::Silent));
    assert!(matches!(&sounds[7], SoundSource::Synth(r) if r.layers.len() == 8));
    let said = notes(&theme).join("\n");
    for part in [
        "sound[1].layers: 0 layers; a recipe has 1 to 8. The sound is silent",
        "sound[2].layers: 9 layers",
        "sound[3].effects: 5 effects; a recipe has at most 4. The sound is silent",
        "sound[4].layers: expected a list of 1 to 8 layers, got nil",
        "sound[5]: expected a recipe",
        "sound[6]: expected a recipe",
        "sound[7].layers[1]: expected a layer table, got 1",
    ] {
        assert!(said.contains(part), "no {part:?} in:\n{said}");
    }
    assert!(!said.contains("sound[8]"));
    // Silent sounds are made, and play nothing.
    let made = prepare_all(&sounds);
    assert!(
        made[..7]
            .iter()
            .all(|m| m.as_ref().is_ok_and(Vec::is_empty))
    );
    assert!(made[7].as_ref().is_ok_and(|s| !s.is_empty()));
}

#[test]
fn a_thirty_third_sound_plays_nothing() {
    let (theme, _folder) = theme(
        r#"
local last
for i = 1, 40 do
	last = gmnb.synth({ layers = { { freq = 200 + i } } })
	assert(last.id == (if i <= 32 then i else 0), "sound " .. i .. " has id " .. last.id)
end
-- One already made is still found.
assert(gmnb.synth({ layers = { { freq = 201 } } }).id == 1)
"#,
    );
    assert_eq!(theme.sounds_declared(), 32);
    let said = notes(&theme);
    assert_eq!(said, ["sound: more than 32 sounds; the rest play nothing"]);
    assert_eq!(theme.sounds().sounds.len(), limits::SOUNDS);
}

#[test]
fn note_names() {
    let (_theme, _folder) = theme(
        r#"
assert(gmnb.note("A4") == 440)
assert(gmnb.note("A5") == 880)
assert(math.abs(gmnb.note("C4") - 261.6255653005986) < 1e-9)
assert(gmnb.note("C#5") == gmnb.note("Db5"))
assert(math.abs(gmnb.note("C-1") - 8.175798915643707) < 1e-12)
for _, bad in { "H4", "a4", "A", "A10", "", "A4 " } do
	local ok, err = pcall(gmnb.note, bad)
	assert(not ok and string.find(tostring(err), "gmnb.note"), bad)
end
assert(not pcall(gmnb.note, 440))
assert(not pcall(gmnb.note))
-- A pitch in a recipe may be the number it gives.
gmnb.synth({ layers = { { freq = gmnb.note("E6") * 2 } } })
"#,
    );
}

// ---- gmnb.sound ------------------------------------------------------------------

#[test]
fn sound_files_are_named_not_read() {
    let folder = Folder::new(
        r#"
local tap = gmnb.sound("sounds/tap.wav")
local loud = gmnb.sound("sounds/tap.wav", { gain = 0.5 })
assert(tap.id == 1 and loud.id == 2)
assert(rawequal(gmnb.sound("sounds/tap.wav"), tap))
assert(rawequal(gmnb.sound("sounds/tap.wav", { gain = 0.5 }), loud))
local gone = gmnb.sound("sounds/gone.ogg", { gain = 7, speed = 2 })
local out = gmnb.sound("../other/tap.wav")
local abs = gmnb.sound("/etc/passwd")
local odd = gmnb.sound(12)
local link = gmnb.sound("sounds/link.wav")
local junk = gmnb.sound("sounds/junk.wav", "loud")
assert(gone.id == 3 and out.id == 4 and abs.id == 5 and odd.id == 6 and link.id == 7 and junk.id == 8)
return { api = 1, name = "Files", sounds = { key = tap, error = gone } }
"#,
    );
    // A real WAV, a file that is not a sound, and a link leading outside.
    let tap = gmnb_sound::wav::encode(&render(&common::click_soft()));
    std::fs::write(folder.0.join("sounds/tap.wav"), &tap).unwrap();
    std::fs::write(folder.0.join("sounds/junk.wav"), b"not a sound at all").unwrap();
    let outside = folder.0.with_extension("outside.wav");
    std::fs::write(&outside, &tap).unwrap();
    std::os::unix::fs::symlink(&outside, folder.0.join("sounds/link.wav")).unwrap();

    let theme = folder.load();
    let said = notes(&theme).join("\n");
    for part in [
        "sound[3].gain: 7 is out of range; using 1",
        "sound[3].speed: unknown key",
        "sound[4]: expected a file inside the theme's folder",
        "sound[5]: expected a file inside the theme's folder",
        "sound[6]: expected a file inside the theme's folder, got 12",
        "sound[8]: expected options",
    ] {
        assert!(said.contains(part), "no {part:?} in:\n{said}");
    }
    let sounds = theme.sounds().sounds;
    assert_eq!(sounds.len(), 8);
    let file = |i: usize| match &sounds[i] {
        SoundSource::File { name, data, gain } => (name.as_str(), data.clone(), *gain),
        other => panic!("{other:?}"),
    };
    let (name, data, gain) = file(0);
    assert_eq!((name, gain), ("sounds/tap.wav", None));
    assert!(matches!(&data, FileData::Path(p) if p.ends_with("sounds/tap.wav")));
    assert_eq!(file(1).2, Some(0.5));
    assert!(matches!(file(2).1, FileData::Missing(_)));
    assert_eq!(file(2).2, Some(1.0));
    // Paths that lead outside the folder name nothing.
    assert_eq!(sounds[3], SoundSource::Silent);
    assert_eq!(sounds[4], SoundSource::Silent);
    assert_eq!(sounds[5], SoundSource::Silent);
    assert!(matches!(file(6).1, FileData::Missing(why) if why.contains("outside")));

    // The engine's side: the good file plays as recorded or at its gain;
    // the others are refused with a reason.
    let made = prepare_all(&sounds);
    assert_eq!(made[0].as_ref().unwrap(), &render(&common::click_soft()));
    let peak = |s: &[i16]| s.iter().map(|v| v.unsigned_abs()).max().unwrap();
    assert_eq!(peak(made[1].as_ref().unwrap()), 16_384);
    assert!(made[2].is_err() && made[6].is_err());
    assert!(
        made[7]
            .as_ref()
            .is_err_and(|e| e.contains("neither a WAV nor an Ogg Vorbis"))
    );
    std::fs::remove_file(outside).unwrap();
}

// ---- The `sounds` table ------------------------------------------------------------

fn table_of(body: &str) -> (gmnb_sound::SoundTable, Vec<String>) {
    let folder = Folder::new(&format!(
        "local a = gmnb.synth({{ layers = {{ {{ freq = 300 }} }} }})\n\
         local b = gmnb.synth({{ layers = {{ {{ freq = 400 }} }} }})\n\
         local c = gmnb.synth({{ layers = {{ {{ freq = 500 }} }} }})\n\
         return {{ api = 1, name = \"T\", sounds = {body} }}"
    ));
    let theme = folder.load();
    let notes = theme.notes.list.iter().map(|n| n.to_string()).collect();
    (theme.def.sounds.clone(), notes)
}

#[test]
fn the_sounds_table_as_the_chapter_writes_it() {
    let (table, notes) = table_of(
        r#"{
	press = a,
	digit = { sound = { a, b, c }, pick = "cycle" },
	operator = { sound = a, rate = 0.8 },
	equals = false,
	backspace = { sound = a, rate = 1.5, volume = 0.6 },
	result = { sound = b, delay = 0.04 },
	error = c,
	key = { sound = a, rate = { 0.96, 1.04 }, pan = "position", pick = "key" },
	mode = { sound = c, pan = { 0.5, -0.5 }, volume = { 0.2, 0.4 }, pick = "random" },
}"#,
    );
    assert_eq!(notes, Vec::<String>::new());
    let entry = |name| match table.get(name) {
        Some(Slot::Entry(e)) => e.clone(),
        other => panic!("{name:?}: {other:?}"),
    };
    assert_eq!(entry(EventName::Press), Entry::plain(SoundId(1)));
    assert_eq!(entry(EventName::Error), Entry::plain(SoundId(3)));
    let digit = entry(EventName::Digit);
    assert_eq!(digit.sounds, [SoundId(1), SoundId(2), SoundId(3)]);
    assert_eq!(digit.pick, Pick::Cycle);
    assert_eq!(entry(EventName::Operator).rate, Varied::One(0.8));
    assert_eq!(table.get(EventName::Equals), Some(&Slot::Silent));
    let back = entry(EventName::Backspace);
    assert_eq!(
        (back.rate, back.volume),
        (Varied::One(1.5), Varied::One(0.6))
    );
    assert_eq!(entry(EventName::Result).delay, 0.04);
    let key = entry(EventName::Key);
    assert_eq!(key.rate, Varied::Range(0.96, 1.04));
    assert_eq!((key.pan, key.pick), (Pan::Position, Pick::Key));
    let mode = entry(EventName::Mode);
    // A range is low to high, whichever way it was written.
    assert_eq!(mode.pan, Pan::At(Varied::Range(-0.5, 0.5)));
    assert_eq!(mode.volume, Varied::Range(0.2, 0.4));
    assert_eq!(mode.pick, Pick::Random);
    assert_eq!(table.get(EventName::Func), None);
    // Names with no entry use the next of their row; `false` stops.
    assert_eq!(table.find(EventName::Func).unwrap().0, EventName::Key);
    assert_eq!(table.find(EventName::Memory).unwrap().0, EventName::Press);
    assert_eq!(table.find(EventName::Equals), None);
}

#[test]
fn what_is_wrong_in_the_sounds_table_is_noted() {
    let (table, notes) = table_of(
        r#"{
	keypress = a,
	digit = true,
	operator = "click",
	func = { rate = 2 },
	equals = { sound = {} },
	toggle = { sound = { a, 5, b }, pick = "shuffle", volume = 3, rate = { 0.1, 9 }, pan = "left",
	           delay = 5, loop = true },
	clear = { sound = { a, a, a, a, a, a, a, a, a, a } },
	backspace = { sound = b, rate = { 1, 2, 3 } },
	memory = { kind = "sound", id = 99 },
	button = { sound = a, volume = { "quiet", 1 } },
}"#,
    );
    let said = notes.join("\n");
    for part in [
        "sounds.keypress: unknown event",
        "sounds.digit: expected a sound, false, or",
        "sounds.operator: expected a sound, false, or",
        "sounds.func: expected a sound, false, or",
        "sounds.equals.sound: expected a sound or a list of sounds",
        "sounds.toggle.sound[2]: expected a sound",
        "sounds.toggle.pick: expected one of \"cycle\", \"random\", \"key\"",
        "sounds.toggle.volume: 3 is out of range; using 1",
        "sounds.toggle.rate[1]: 0.1 is out of range; using 0.5",
        "sounds.toggle.rate[2]: 9 is out of range; using 2",
        "sounds.toggle.pan: expected a number, a range or \"position\"",
        "sounds.toggle.delay: 5 is out of range; using 1",
        "sounds.toggle.loop: unknown key",
        "sounds.clear.sound: 10 sounds; an entry chooses from at most 8",
        "sounds.backspace.rate: expected a number or a range { low, high }",
        "sounds.memory: expected a sound, false, or",
        "sounds.button.volume[1]: expected a number",
    ] {
        assert!(said.contains(part), "no {part:?} in:\n{said}");
    }
    for gone in [
        EventName::Digit,
        EventName::Operator,
        EventName::Func,
        EventName::Equals,
        EventName::Memory,
    ] {
        assert_eq!(table.get(gone), None, "{gone:?}");
    }
    let entry = |name| match table.get(name) {
        Some(Slot::Entry(e)) => e.clone(),
        other => panic!("{name:?}: {other:?}"),
    };
    let toggle = entry(EventName::Toggle);
    assert_eq!(toggle.sounds, [SoundId(1), SoundId(2)]);
    assert_eq!(toggle.pick, Pick::Cycle);
    assert_eq!(toggle.volume, Varied::One(1.0));
    assert_eq!(toggle.rate, Varied::Range(0.5, 2.0));
    assert_eq!(toggle.pan, Pan::At(Varied::One(0.0)));
    assert_eq!(toggle.delay, 1.0);
    assert_eq!(entry(EventName::Clear).sounds.len(), limits::ENTRY_SOUNDS);
    assert_eq!(entry(EventName::Backspace).rate, Varied::One(1.0));
    assert_eq!(entry(EventName::Button).volume, Varied::One(1.0));

    // Not a table at all.
    let (table, notes) = table_of("5");
    assert!(table.is_empty());
    assert_eq!(notes, ["sounds: expected a table of sounds by event"]);
}

// ---- The `sound` function ----------------------------------------------------------

#[test]
fn a_theme_may_have_a_sound_function() {
    let folder = Folder::new(
        r#"
local chime = gmnb.synth({ layers = { { freq = "E6" } } })
local streak = 0
return {
	api = 1, name = "Hook",
	sound = function(event, ctx)
		if event.kind == "result" then
			streak += 1
			return { sound = chime, rate = math.min(1 + streak * 0.06, 1.5) }
		elseif event.kind == "late" then
			-- A sound made inside the function: it joins the theme's sounds.
			return gmnb.synth({ layers = { { freq = "G6" } } })
		elseif event.kind == "many" then
			return { chime, { sound = chime, delay = 0.08, pan = 0.5 } }
		elseif event.kind == "press" then
			return false
		end
		return nil
	end,
}"#,
    );
    let theme = folder.load();
    assert!(theme.notes.is_empty(), "{:?}", theme.notes);
    assert_eq!(gmnb_theme::sound::HOOK_TIME.as_millis(), 2);
    let sounds = theme.sounds();
    assert!(sounds.hook && sounds.any() && sounds.table.is_empty());
    assert_eq!(theme.sounds_declared(), 1);
    let hook = theme.def.sound.clone().unwrap();

    let call = |kind: &str| {
        let event = theme.engine().lua().create_table().unwrap();
        event.raw_set("kind", kind).unwrap();
        // (A real call has `sound::HOOK_TIME`; a test on a busy machine
        // gets more.)
        let budget = gmnb_theme::Budget::Custom(std::time::Duration::from_millis(250));
        let value = theme.engine().call(&hook, budget, event).unwrap();
        let mut notes = Notes::default();
        let choice = parse_choice(&value, &mut notes);
        assert!(notes.is_empty(), "{notes:?}");
        choice
    };
    assert_eq!(call("mode"), Choice::Table);
    assert_eq!(call("press"), Choice::Silence);
    let Choice::Plays(first) = call("result") else {
        panic!()
    };
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].sound, SoundId(1));
    assert!((first[0].params.rate - 1.06).abs() < 1e-6);
    let Choice::Plays(second) = call("result") else {
        panic!()
    };
    assert!((second[0].params.rate - 1.12).abs() < 1e-6);
    let Choice::Plays(many) = call("many") else {
        panic!()
    };
    assert_eq!(many.len(), 2);
    assert_eq!(many[0].params, gmnb_sound::PlayParams::default());
    assert_eq!((many[1].params.delay, many[1].params.pan), (0.08, 0.5));

    // A sound made in the function: there is one more now, for the engine
    // to be told of.
    let Choice::Plays(late) = call("late") else {
        panic!()
    };
    assert_eq!(late[0].sound, SoundId(2));
    assert_eq!(theme.sounds_declared(), 2);
    assert_eq!(theme.sounds().sounds.len(), 2);
    call("late");
    assert_eq!(theme.sounds_declared(), 2);
}

#[test]
fn what_sound_returns_is_checked() {
    let sound = |id: i64| {
        ThemeValue::Map(
            [
                ("kind".to_string(), ThemeValue::String("sound".into())),
                ("id".to_string(), ThemeValue::Int(id)),
            ]
            .into(),
        )
    };
    let map = |pairs: Vec<(&str, ThemeValue)>| {
        ThemeValue::Map(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    };
    let check = |v: &ThemeValue| {
        let mut notes = Notes::default();
        let choice = parse_choice(v, &mut notes);
        let notes: Vec<String> = notes.list.iter().map(|n| n.to_string()).collect();
        (choice, notes)
    };
    assert_eq!(check(&ThemeValue::Nil), (Choice::Table, vec![]));
    assert_eq!(check(&ThemeValue::Bool(false)), (Choice::Silence, vec![]));
    assert_eq!(check(&ThemeValue::Map(Default::default())).0, Choice::Table);
    // Numbers out of range are clamped; ranges and "position" are the
    // table's, not a play's.
    let (choice, notes) = check(&map(vec![
        ("sound", sound(2)),
        ("volume", ThemeValue::Number(1.5)),
        ("rate", ThemeValue::Int(4)),
        ("pan", ThemeValue::String("position".into())),
        ("delay", ThemeValue::Number(-1.0)),
        ("pick", ThemeValue::String("key".into())),
    ]));
    let Choice::Plays(plays) = choice else {
        panic!()
    };
    assert_eq!(plays[0].sound, SoundId(2));
    let p = plays[0].params;
    assert_eq!((p.volume, p.rate, p.pan, p.delay), (1.0, 2.0, 0.0, 0.0));
    assert_eq!(notes.len(), 5, "{notes:?}");
    // Up to four at once.
    let six = ThemeValue::Array((1..=6).map(sound).collect());
    let (choice, notes) = check(&six);
    assert!(matches!(choice, Choice::Plays(p) if p.len() == 4));
    assert!(notes[0].contains("6 plays; an event starts at most 4"));
    // What is no play leaves the table to decide, with a note.
    for odd in [
        ThemeValue::Bool(true),
        ThemeValue::Int(3),
        ThemeValue::String("click".into()),
        map(vec![("rate", ThemeValue::Int(1))]),
        ThemeValue::Array(vec![ThemeValue::Int(1)]),
        sound(99),
    ] {
        let (choice, notes) = check(&odd);
        assert_eq!(choice, Choice::Table, "{odd:?}");
        assert!(!notes.is_empty(), "{odd:?}");
    }
}

// ---- Events ------------------------------------------------------------------------

#[test]
fn an_event_gets_its_name() {
    use gmnb_theme::events::{Action, Event, EventKind, KeyKind, Page, Source as From, Via};
    use gmnb_theme::sound::{across, answers, event_name, trigger};
    let rect = [100.0, 50.0, 40.0, 20.0];
    let at = |kind, page| Event::at(kind, page, rect);
    let key = |kind| Event {
        source: Some(From::Key),
        via: Some(Via::Pointer),
        key: Some((Some(7), kind)),
        ..at(EventKind::Press, Page::Calculator)
    };
    for (kind, name) in [
        (KeyKind::Number, EventName::Digit),
        (KeyKind::Operator, EventName::Operator),
        (KeyKind::Func, EventName::Func),
        (KeyKind::Equals, EventName::Equals),
        (KeyKind::Toggle, EventName::Toggle),
        (KeyKind::Other, EventName::Func),
    ] {
        assert_eq!(event_name(&key(kind)), Some(name));
    }
    // What a control does comes before what it is: C is a function key.
    let with = |action, kind| Event {
        action: Some(action),
        ..key(kind)
    };
    assert_eq!(
        event_name(&with(Action::Clear, KeyKind::Func)),
        Some(EventName::Clear)
    );
    assert_eq!(
        event_name(&with(Action::Backspace, KeyKind::Other)),
        Some(EventName::Backspace)
    );
    let button = Event {
        source: Some(From::Button),
        ..at(EventKind::Press, Page::Converter)
    };
    assert_eq!(event_name(&button), Some(EventName::Button));
    let memory = Event {
        action: Some(Action::Memory),
        ..button.clone()
    };
    assert_eq!(event_name(&memory), Some(EventName::Memory));
    // Results and errors are the Calculator page's; a mode is a mode.
    assert_eq!(
        event_name(&at(EventKind::Result, Page::Calculator)),
        Some(EventName::Result)
    );
    assert_eq!(event_name(&at(EventKind::Result, Page::Converter)), None);
    assert_eq!(
        event_name(&at(EventKind::Error, Page::Calculator)),
        Some(EventName::Error)
    );
    assert_eq!(event_name(&at(EventKind::Error, Page::Date)), None);
    assert_eq!(
        event_name(&at(EventKind::Mode, Page::Graphing)),
        Some(EventName::Mode)
    );
    assert_eq!(event_name(&at(EventKind::Option, Page::Settings)), None);
    assert_eq!(event_name(&at(EventKind::Show, Page::Calculator)), None);

    // Sound answers these five kinds, however the control was used,
    // except through the accessibility interface.
    assert!(answers(&key(KeyKind::Number)));
    assert!(answers(&at(EventKind::Option, Page::Settings)));
    assert!(answers(&at(EventKind::Result, Page::Converter)));
    assert!(!answers(&at(EventKind::Show, Page::Calculator)));
    let keyboard = Event {
        via: Some(Via::Keyboard),
        ..key(KeyKind::Number)
    };
    assert!(answers(&keyboard));
    let assistive = Event {
        via: Some(Via::Assistive),
        ..key(KeyKind::Number)
    };
    assert!(!answers(&assistive));

    // Where it happened: the middle of its control, across a window.
    let e = key(KeyKind::Number);
    assert_eq!(e.x, 120.0);
    assert_eq!(across(&e, 240.0), 0.0);
    assert_eq!(across(&e, 480.0), -0.5);
    assert_eq!(across(&e, 60.0), 1.0);
    assert_eq!(across(&e, 0.0), 0.0);
    let t = trigger(&e, 480.0);
    assert_eq!((t.key, t.across), (Some(7), Some(0.25)));
    assert_eq!(trigger(&button, 240.0).key, None);
}

// ---- Nothing a recipe holds can do harm --------------------------------------------

/// A small generator for the random recipes.
struct Rng(u32);

impl Rng {
    /// A number below `n`.
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as usize % n
    }

    fn of<'a>(&mut self, list: &[&'a str]) -> &'a str {
        list[self.below(list.len())]
    }
}

const NUMBERS: [&str; 22] = [
    "0",
    "1",
    "-1",
    "0.5",
    "0.001",
    "0.02",
    "440",
    "3800",
    "20000",
    "1e9",
    "-1e9",
    "1e308",
    "-1e308",
    "1e-300",
    "0/0",
    "math.huge",
    "-math.huge",
    "4294967296",
    "2",
    "7",
    "0.9",
    "48000",
];
const OTHERS: [&str; 16] = [
    "\"sine\"",
    "\"noise\"",
    "\"square\"",
    "\"saw\"",
    "\"triangle\"",
    "\"exp\"",
    "\"linear\"",
    "\"lowpass\"",
    "\"A4\"",
    "\"C#9\"",
    "\"\"",
    "\"zz\"",
    "true",
    "false",
    "function() end",
    "{}",
];
const KEYS: [&str; 40] = [
    "gain",
    "length",
    "layers",
    "effects",
    "wave",
    "freq",
    "detune",
    "width",
    "phase",
    "seed",
    "fm",
    "vibrato",
    "tremolo",
    "filter",
    "env",
    "pan",
    "at",
    "ratio",
    "index",
    "rate",
    "depth",
    "type",
    "q",
    "attack",
    "decay",
    "sustain",
    "hold",
    "release",
    "curve",
    "time",
    "feedback",
    "mix",
    "ping_pong",
    "size",
    "damp",
    "amount",
    "bits",
    "bogus",
    "kind",
    "id",
];
const TYPES: [&str; 8] = [
    "delay", "reverb", "drive", "crush", "lowpass", "highpass", "bandpass", "phaser",
];

/// Any value: a number (of any size, or none at all), a word, a glide or
/// something like one, a table.
fn random_value(rng: &mut Rng, depth: usize) -> String {
    match rng.below(10) {
        0..=4 => rng.of(&NUMBERS).to_string(),
        5 | 6 => rng.of(&OTHERS).to_string(),
        7 if depth < 4 => {
            let mut parts: Vec<String> = (0..rng.below(4))
                .map(|_| random_value(rng, depth + 1))
                .collect();
            if rng.below(2) == 0 {
                parts.push(format!("time = {}", random_value(rng, depth + 1)));
            }
            if rng.below(3) == 0 {
                parts.push(format!("curve = {}", random_value(rng, depth + 1)));
            }
            format!("{{ {} }}", parts.join(", "))
        }
        _ if depth < 4 => random_table(rng, depth + 1),
        _ => "1".to_string(),
    }
}

/// A table with a few of the keys a recipe has anywhere, each holding
/// anything.
fn random_table(rng: &mut Rng, depth: usize) -> String {
    let mut parts = Vec::new();
    for _ in 0..rng.below(7) {
        let key = rng.of(&KEYS);
        let v = match key {
            "type" if rng.below(4) > 0 => format!("\"{}\"", rng.of(&TYPES)),
            // Mostly short sounds, so that many can be rendered.
            "attack" | "decay" | "hold" | "release" | "length" | "at" if rng.below(4) > 0 => {
                rng.of(&["0", "0.001", "0.01", "0.03"]).to_string()
            }
            _ => random_value(rng, depth),
        };
        parts.push(format!("{key} = {v}"));
    }
    format!("{{ {} }}", parts.join(", "))
}

fn random_list(rng: &mut Rng, most: usize) -> String {
    let items: Vec<String> = (0..rng.below(most + 1))
        .map(|_| {
            if rng.below(8) == 0 {
                random_value(rng, 2)
            } else {
                random_table(rng, 2)
            }
        })
        .collect();
    format!("{{ {} }}", items.join(", "))
}

fn random_recipe(rng: &mut Rng) -> String {
    if rng.below(12) == 0 {
        return random_value(rng, 0);
    }
    let layers = random_list(rng, 9);
    let effects = random_list(rng, 5);
    let gain = random_value(rng, 3);
    let length = if rng.below(3) == 0 {
        random_value(rng, 3)
    } else {
        "nil".into()
    };
    let extra = random_table(rng, 3);
    format!(
        "{{ layers = {layers}, effects = {effects}, gain = {gain}, length = {length}, extra = {extra} }}"
    )
}

/// Random recipe tables, every kind of value in every place: `gmnb.synth`
/// never fails, and what it makes of each renders, within a sound's
/// limits, without a panic.
#[test]
fn random_recipes_never_fail() {
    let mut rng = Rng(0x9e37_79b9);
    let (mut made, mut playable, mut noted, mut rendered) = (0, 0, 0, 0);
    let started = std::time::Instant::now();
    for _round in 0..40 {
        let mut body = String::from("local made = 0\n");
        for _ in 0..32 {
            body.push_str(&format!(
                "do local s = gmnb.synth({}); assert(type(s) == \"table\" and s.kind == \"sound\"); made += 1 end\n",
                random_recipe(&mut rng)
            ));
        }
        body.push_str("assert(made == 32)\n");
        let (theme, _folder) = theme(&body);
        noted += theme.take_sound_notes().len();
        for source in theme.sounds().sounds {
            made += 1;
            let SoundSource::Synth(r) = source else {
                continue;
            };
            playable += 1;
            assert!(r.within_limits());
            // Every number that reaches the renderer is one.
            let text = format!("{r:?}");
            assert!(!text.contains("NaN") && !text.contains("inf"), "{text}");
            // Render the short ones (and some long ones, while there is
            // time).
            if r.layer_seconds() < 0.5 || started.elapsed().as_secs() < 5 {
                let samples = render(&r);
                assert!(samples.len() <= limits::FRAMES * 2);
                rendered += 1;
            }
        }
    }
    println!("{made} sounds, {playable} playable, {rendered} rendered, {noted} notes");
    assert!(
        made > 800 && playable > 150 && rendered > 100,
        "{made} {playable} {rendered}"
    );
    assert!(noted > 1000);
}

#[test]
fn the_built_in_themes_load_with_no_sound_made() {
    for id in ["aurora", "mono"] {
        let theme = Theme::load(Source::built_in(id).unwrap(), Limits::default()).unwrap();
        let sounds = theme.sounds();
        assert!(!sounds.any(), "{id}");
        assert!(theme.take_sound_notes().is_empty());
        assert!(
            !theme.notes.list.iter().any(|n| n.path.starts_with("sound")),
            "{id}: {:?}",
            theme.notes
        );
    }
}
