//! GMNB's theme sounds (docs/theming-media.md, "Sound" and "The mixer and
//! the stream"): a synthesiser that renders a recipe to the same samples
//! everywhere, WAV and Ogg Vorbis files, a small mixer, and an output that
//! speaks PulseAudio's protocol.
//!
//! GMNB only: DGMNB links none of this (CI checks its dependency tree).

//!
//! # For the app
//!
//! ```no_run
//! use gmnb_sound::{PlayParams, Settings, SoundEngine, SoundId, ThemeSounds};
//!
//! // Once: nothing is created yet, no thread and no socket.
//! let mut engine = SoundEngine::new(Settings::from_env(true, 0.5));
//! // A theme was loaded: its sounds are rendered on the sound thread.
//! let sounds = ThemeSounds::default(); // from gmnb_theme: `theme.sounds()`
//! let bank = engine.load(&sounds);
//! // An event: never waits.
//! engine.play(&bank, SoundId(1), PlayParams::default());
//! // The theme went away.
//! engine.unload();
//! ```
//!
//! Which sound an event plays is worked out with a [`Resolver`] over the
//! theme's [`SoundTable`].

pub mod bank;
pub mod engine;
pub mod file;
pub mod mixer;
#[doc(hidden)]
pub mod pulse;
pub mod recipe;
pub mod resample;
/// A stand-in sound server, for the app's tests.
#[cfg(feature = "stand-in")]
#[doc(hidden)]
pub mod standin;
pub mod synth;
pub mod table;
pub mod wav;

pub use bank::{Budget, FileData, SoundSource, ThemeSounds, dump_name, prepare, prepare_all};
pub use engine::{
    Bank, Log, Played, Settings, Sink, SoundEngine, SoundState, Stats, Status, StatusHook,
};
pub use mixer::{MixStats, Mixer, PlayParams};
pub use pulse::{Identity, connection_attempts};
pub use recipe::{
    Curve, Effect, Env, Filter, FilterKind, Fm, Glide, Layer, Lfo, Recipe, Wave, limits, note,
    note_name,
};
pub use synth::render;
pub use table::{
    Entry, EventName, Pan, Pick, Play, Resolver, Slot, SoundId, SoundTable, Trigger, Varied,
};
