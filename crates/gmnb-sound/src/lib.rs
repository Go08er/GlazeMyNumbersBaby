//! GMNB's theme sounds (docs/theming-media.md, "Sound" and "The mixer and
//! the stream"): a synthesiser that renders a recipe to the same samples
//! everywhere, WAV and Ogg Vorbis files, a small mixer, and an output that
//! speaks PulseAudio's protocol.
//!
//! GMNB only: DGMNB links none of this (CI checks its dependency tree).

pub mod recipe;
pub mod synth;
pub mod wav;

pub use recipe::{
    Curve, Effect, Env, Filter, FilterKind, Fm, Glide, Layer, Lfo, Recipe, Wave, limits, note,
    note_name,
};
pub use synth::render;
