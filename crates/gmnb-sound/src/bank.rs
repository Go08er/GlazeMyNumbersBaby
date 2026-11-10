//! A theme's sounds, from what it declared to samples ready to mix: each
//! recipe rendered, each file read and decoded, within the theme-wide
//! limits (docs/theming-media.md, "Limits").

use std::borrow::Cow;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::recipe::{Fnv, Recipe, limits};
use crate::table::SoundTable;

/// Where a sound file's bytes are.
#[derive(Clone, Debug, PartialEq)]
pub enum FileData {
    /// A file on disk. (That it lies inside the theme's folder is checked
    /// by whoever names it.)
    Path(PathBuf),
    /// The bytes themselves: a built-in theme's file.
    Bytes(Cow<'static, [u8]>),
    /// It was not found, or may not be read; why.
    Missing(String),
}

/// One sound a theme declared.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundSource {
    /// Declared wrongly (a recipe with no layers, say): it plays nothing.
    Silent,
    /// `gmnb.synth(recipe)`.
    Synth(Recipe),
    /// `gmnb.sound(file, { gain })`.
    File {
        /// As the theme named it.
        name: String,
        data: FileData,
        gain: Option<f64>,
    },
}

/// Everything about sound a theme declared: what the app hands the engine
/// ([`crate::SoundEngine::load`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeSounds {
    /// By [`crate::SoundId`], from 1.
    pub sounds: Vec<SoundSource>,
    /// Which sound for which event.
    pub table: SoundTable,
    /// Whether the theme has a `sound` function. (It may then make its
    /// sounds later, inside the function.)
    pub hook: bool,
}

impl ThemeSounds {
    /// Whether the theme can make a sound at all: it declared one, or has
    /// a `sound` function.
    pub fn any(&self) -> bool {
        !self.sounds.is_empty() || self.hook
    }
}

/// What a theme's sounds have used of the theme-wide limits so far.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Budget {
    pub sounds: usize,
    /// Of the rendered samples.
    pub bytes: usize,
    pub layer_seconds: f64,
}

/// Reads a sound file of at most 1 MiB; the size is checked before the
/// file is read, and again as it is.
fn read(path: &Path) -> Result<Vec<u8>, String> {
    let too_big = |n: u64| format!("{n} bytes (a sound file is at most 1 MiB)");
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a file".into());
    }
    if meta.len() > limits::FILE_BYTES {
        return Err(too_big(meta.len()));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(limits::FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limits::FILE_BYTES {
        return Err(too_big(bytes.len() as u64));
    }
    Ok(bytes)
}

/// Makes one sound's samples (interleaved stereo, 48 kHz), counting it
/// against `budget`. A sound that is refused still counts as one of the
/// theme's 32, so a theme can make the app try at most that many, good or
/// bad.
///
/// `Err` is the note for the theme's author: the sound then plays nothing.
pub fn prepare(source: &SoundSource, budget: &mut Budget) -> Result<Vec<i16>, String> {
    budget.sounds += 1;
    if budget.sounds > limits::SOUNDS {
        return Err(format!(
            "more than {} sounds; this one is left out",
            limits::SOUNDS
        ));
    }
    let samples = match source {
        SoundSource::Silent => Vec::new(),
        SoundSource::Synth(recipe) => {
            if !recipe.within_limits() {
                return Err(format!(
                    "a recipe has 1 to {} layers and at most {} effects",
                    limits::LAYERS,
                    limits::EFFECTS
                ));
            }
            let seconds = recipe.layer_seconds();
            if budget.layer_seconds + seconds > limits::LAYER_SECONDS {
                return Err(format!(
                    "the theme's recipes are over {} layer-seconds together",
                    limits::LAYER_SECONDS
                ));
            }
            budget.layer_seconds += seconds;
            crate::synth::render(recipe)
        }
        SoundSource::File { data, gain, .. } => {
            let bytes = match data {
                FileData::Path(path) => Cow::Owned(read(path)?),
                FileData::Bytes(bytes) => Cow::Borrowed(bytes.as_ref()),
                FileData::Missing(why) => return Err(why.clone()),
            };
            crate::file::decode(&bytes, *gain)?
        }
    };
    let bytes = samples.len() * 2;
    if budget.bytes + bytes > limits::TOTAL_BYTES {
        return Err("the theme's sounds are over 8 MiB together; this one is left out".into());
    }
    budget.bytes += bytes;
    Ok(samples)
}

/// Makes all of a theme's sounds, in order, on the calling thread: for
/// tools and tests. (The app's sounds are made on the sound thread.)
pub fn prepare_all(sounds: &[SoundSource]) -> Vec<Result<Vec<i16>, String>> {
    let mut budget = Budget::default();
    sounds.iter().map(|s| prepare(s, &mut budget)).collect()
}

/// The file name a sound is dumped under (`GMNB_SOUND_DUMP`): its order
/// among the theme's sounds, from 1, and a hash of its recipe or of its
/// file's name.
pub fn dump_name(order: usize, source: &SoundSource) -> String {
    let hash = match source {
        SoundSource::Silent => 0,
        SoundSource::Synth(recipe) => recipe.fingerprint(),
        SoundSource::File { name, .. } => {
            let mut h = Fnv::new();
            h.bytes(name.as_bytes());
            h.finish()
        }
    };
    format!("{order:02}-{hash:016x}.wav")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{Env, Layer};

    fn beep(seconds: f64) -> Recipe {
        Recipe {
            layers: vec![Layer {
                env: Env {
                    attack: 0.0,
                    decay: seconds,
                    ..Env::default()
                },
                ..Layer::default()
            }],
            ..Recipe::default()
        }
    }

    #[test]
    fn the_thirty_third_sound_is_left_out() {
        let sounds = vec![SoundSource::Synth(beep(0.01)); 34];
        let made = prepare_all(&sounds);
        assert!(
            made[..32]
                .iter()
                .all(|m| m.as_ref().is_ok_and(|s| s.len() == 960))
        );
        assert!(made[32].as_ref().is_err_and(|e| e.contains("more than 32")));
        assert!(made[33].is_err());
        // A sound that is refused has used one of the 32 too.
        let mut sounds = vec![SoundSource::Silent; 20];
        sounds.extend(vec![
            SoundSource::File {
                name: "gone.wav".into(),
                data: FileData::Missing("gone.wav: no such file".into()),
                gain: None,
            };
            12
        ]);
        sounds.push(SoundSource::Synth(beep(0.01)));
        let made = prepare_all(&sounds);
        assert!(
            made[..20]
                .iter()
                .all(|m| m.as_ref().is_ok_and(Vec::is_empty))
        );
        assert!(
            made[20..32]
                .iter()
                .all(|m| m.as_ref().is_err_and(|e| e.contains("no such file")))
        );
        assert!(made[32].as_ref().is_err_and(|e| e.contains("more than 32")));
    }

    #[test]
    fn a_sound_is_at_most_four_seconds() {
        let made = prepare_all(&[SoundSource::Synth(beep(60.0))]);
        assert_eq!(made[0].as_ref().unwrap().len(), limits::FRAMES * 2);
    }

    #[test]
    fn all_together_stay_under_eight_mebibytes() {
        // Each is 4 s: 768 000 bytes. Ten fit in 8 MiB; the eleventh
        // doesn't. A short one after it still does.
        let mut sounds = vec![SoundSource::Synth(beep(4.0)); 11];
        sounds.push(SoundSource::Synth(beep(0.5)));
        let made = prepare_all(&sounds);
        assert!(made[..10].iter().all(Result::is_ok));
        assert!(made[10].as_ref().is_err_and(|e| e.contains("8 MiB")));
        assert!(made[11].is_ok());
        let total: usize = made.iter().flatten().map(|s| s.len() * 2).sum();
        assert!(total <= limits::TOTAL_BYTES);
    }

    #[test]
    fn layer_seconds_are_counted_over_the_whole_theme() {
        // Eight 4 s layers are 32 layer-seconds: two such recipes use the
        // 64 there are, and a third is refused.
        let big = Recipe {
            layers: vec![beep(4.0).layers[0].clone(); 8],
            ..Recipe::default()
        };
        assert_eq!(big.layer_seconds(), 32.0);
        let mut budget = Budget::default();
        assert!(prepare(&SoundSource::Synth(big.clone()), &mut budget).is_ok());
        assert!(prepare(&SoundSource::Synth(big.clone()), &mut budget).is_ok());
        assert_eq!(budget.layer_seconds, 64.0);
        let third = prepare(&SoundSource::Synth(beep(0.1)), &mut budget);
        assert!(third.is_err_and(|e| e.contains("layer-seconds")));
        // A layer longer than a sound counts as 4 s, not more.
        assert_eq!(beep(1000.0).layer_seconds(), 4.0);
    }

    #[test]
    fn a_recipe_past_its_own_limits_is_refused() {
        let nine = Recipe {
            layers: vec![beep(0.01).layers[0].clone(); 9],
            ..Recipe::default()
        };
        let none = Recipe::default();
        let five = Recipe {
            effects: vec![crate::recipe::Effect::Drive { amount: 0.1 }; 5],
            ..beep(0.01)
        };
        for recipe in [nine, none, five] {
            assert!(crate::synth::render(&recipe).is_empty());
            let made = prepare_all(&[SoundSource::Synth(recipe)]);
            assert!(made[0].as_ref().is_err_and(|e| e.contains("layers")));
        }
    }

    #[test]
    fn dump_names_say_order_and_what() {
        let a = dump_name(1, &SoundSource::Synth(beep(0.01)));
        let b = dump_name(2, &SoundSource::Synth(beep(0.02)));
        assert!(a.starts_with("01-") && a.ends_with(".wav") && a.len() == 23);
        assert_ne!(a[3..], b[3..]);
        assert_eq!(a, dump_name(1, &SoundSource::Synth(beep(0.01))));
        let file = |name: &str| SoundSource::File {
            name: name.into(),
            data: FileData::Missing(String::new()),
            gain: None,
        };
        assert_ne!(dump_name(3, &file("a.ogg")), dump_name(3, &file("b.ogg")));
    }
}
