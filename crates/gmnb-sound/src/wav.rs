//! Writing sounds out as WAV files: `GMNB_SOUND_DUMP` and the render
//! example.

use std::path::Path;

use crate::recipe::RATE;

/// A 16-bit stereo 48 kHz WAV file of interleaved `samples`.
pub fn encode(samples: &[i16]) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut v = Vec::with_capacity(44 + data as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    // PCM, two channels.
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&RATE.to_le_bytes());
    v.extend_from_slice(&(RATE * 4).to_le_bytes());
    v.extend_from_slice(&4u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

/// Writes `samples` to `path` as a WAV file.
pub fn write(path: &Path, samples: &[i16]) -> std::io::Result<()> {
    std::fs::write(path, encode(samples))
}
