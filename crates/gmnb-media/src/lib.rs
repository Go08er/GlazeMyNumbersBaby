//! A theme's pictures (docs/theming-media.md, "Pictures" and "For
//! implementers"): what a file is, by its first bytes; stills decoded to
//! RGBA; animations (GIF, APNG, animated WebP) streamed frame by frame on
//! one thread that takes a quarter of a core at most; and the field pass,
//! which colours a coarse grid of numbers through a ramp.
//!
//! No toolkit and no script engine: bytes in, RGBA bytes out. The app
//! turns them into textures.
//!
//! # A theme's file is a stranger's
//!
//! - A file is what its first bytes say ([`sniff`]), never what its name
//!   says, and only four kinds are taken: PNG, JPEG, WebP, GIF.
//! - Its size is read from its header ([`probe`]) and checked before
//!   anything is decoded; each decoder also runs under its own limit.
//! - A decoder that panics is caught ([`shielded`]) and the file refused.
//! - Nothing here starts a thread until an animation needs a frame after
//!   its first ([`Player`]).

mod anim;
pub mod bucket;
mod cpu;
pub mod field;
mod player;
mod sniff;
mod still;
mod stream;

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};

pub use anim::{AnimLimits, MIN_SHOWN, Timeline, open};
pub use bucket::Bucket;
pub use cpu::thread_time;
pub use field::{Field, Ramp};
pub use player::{Frame, Player, SHARE, Source, Stats, Watch, threads_started};
pub use sniff::{Format, Info, probe, sniff};
pub use still::{Rgba, decode_still, premultiply};
pub use stream::Stream;

/// Why a file gave no picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Its first bytes are none of PNG, JPEG, WebP or GIF.
    Unknown,
    /// A kind that can't be an animation (a JPEG).
    NotAnimation(Format),
    /// Wider or taller than allowed, or of no size.
    Size { width: u32, height: u32, max: u32 },
    /// More frames than allowed.
    Frames { max: u32 },
    /// The file is cut short or malformed: what the decoder said.
    Broken(String),
    /// The decoder panicked.
    Panicked(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unknown => f.write_str("not a PNG, JPEG, WebP or GIF file (by its first bytes)"),
            Error::NotAnimation(format) => {
                write!(
                    f,
                    "a {format} file; an animation is a GIF, PNG or WebP file"
                )
            }
            Error::Size { width, height, max } => {
                write!(f, "{width}×{height}; at most {max}×{max} is taken")
            }
            Error::Frames { max } => write!(f, "more than {max} frames"),
            Error::Broken(why) => write!(f, "can't be decoded: {why}"),
            Error::Panicked(why) => write!(f, "can't be decoded: the decoder failed ({why})"),
        }
    }
}

impl std::error::Error for Error {}

/// The longest message kept of what a decoder said.
const MESSAGE: usize = 160;

pub(crate) fn broken(why: impl fmt::Display) -> Error {
    let mut text = why.to_string();
    if text.len() > MESSAGE {
        let mut end = MESSAGE;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    Error::Broken(text)
}

/// Runs a decoder: a panic inside it is an [`Error::Panicked`], not the
/// app's. (The decoders are other people's code reading a stranger's
/// file; whatever state a panic left behind is dropped with the closure.)
pub fn shielded<T>(f: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(panic) => {
            let what = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "a panic".into());
            let Error::Broken(text) = broken(what) else {
                unreachable!()
            };
            Err(Error::Panicked(text))
        }
    }
}

/// Bytes shared between the threads and with the textures made of them.
/// (`Arc<Vec<u8>>` is not `AsRef<[u8]>`; this is.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bytes(pub std::sync::Arc<Vec<u8>>);

impl Bytes {
    pub fn new(bytes: Vec<u8>) -> Bytes {
        Bytes(std::sync::Arc::new(bytes))
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl std::ops::Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}
