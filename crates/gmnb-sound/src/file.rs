//! Sound files (docs/theming-media.md, "Sound files"): WAV through
//! `hound`, Ogg Vorbis through `lewton`, both safe Rust. What a file is, is
//! read from its first bytes, never from its name. The limits are checked
//! before the work they bound: the file's size before it is looked at, its
//! channels, rate and length from its header before a sample is decoded
//! (and, for Vorbis, whose header does not say how long it is, as the
//! samples come).

use std::io::Cursor;

use crate::recipe::{RATE, limits};
use crate::resample::{resample, resampled_len};

/// What a file's first bytes say it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Wav,
    OggVorbis,
}

/// `RIFF….WAVE` is WAV, `OggS` is Ogg; anything else is refused.
pub fn sniff(bytes: &[u8]) -> Option<Kind> {
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        Some(Kind::Wav)
    } else if bytes.len() >= 4 && &bytes[..4] == b"OggS" {
        Some(Kind::OggVorbis)
    } else {
        None
    }
}

/// A decoded file: samples from -1 to 1, one list per channel.
struct Decoded {
    rate: u32,
    channels: Vec<Vec<f64>>,
}

fn check_format(channels: u32, rate: u32) -> Result<(), String> {
    if channels == 0 || channels > u32::from(limits::FILE_CHANNELS) {
        return Err(format!("{channels} channels (a sound has 1 or 2)"));
    }
    if rate < limits::FILE_RATE.0 || rate > limits::FILE_RATE.1 {
        return Err(format!(
            "{rate} samples a second (a sound has 8000 to 96000)"
        ));
    }
    Ok(())
}

/// The most frames a file of this rate may hold: 4 s.
fn most_frames(rate: u32) -> usize {
    (limits::SECONDS * f64::from(rate)) as usize
}

/// The longest block a Vorbis stream has, in frames.
const LAST_BLOCK: usize = 8192;

fn too_long() -> String {
    "longer than 4 s".into()
}

fn wav(bytes: &[u8]) -> Result<Decoded, String> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    check_format(u32::from(spec.channels), spec.sample_rate)?;
    let float = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 8 | 16 | 24 | 32) => false,
        (hound::SampleFormat::Float, 32) => true,
        (format, bits) => {
            return Err(format!(
                "{bits}-bit {format:?} samples (a WAV sound is 8, 16, 24 or 32-bit, or 32-bit float)"
            ));
        }
    };
    // The header says how much is to come; the samples are only read if
    // that is within the limit.
    let frames = reader.duration() as usize;
    if frames > most_frames(spec.sample_rate) {
        return Err(too_long());
    }
    let count = spec.channels as usize;
    let mut channels = vec![Vec::with_capacity(frames); count];
    if float {
        for (i, s) in reader.samples::<f32>().enumerate() {
            let s = s.map_err(|e| e.to_string())?;
            let s = if s.is_finite() {
                s.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            channels[i % count].push(f64::from(s));
        }
    } else {
        let full = f64::from(1u32 << (spec.bits_per_sample - 1));
        for (i, s) in reader.samples::<i32>().enumerate() {
            let s = s.map_err(|e| e.to_string())?;
            channels[i % count].push(f64::from(s) / full);
        }
    }
    if channels.iter().any(|c| c.len() != frames) {
        return Err("the file ends before its samples do".into());
    }
    Ok(Decoded {
        rate: spec.sample_rate,
        channels,
    })
}

fn ogg(bytes: &[u8]) -> Result<Decoded, String> {
    let describe = |e: lewton::VorbisError| format!("{e}");
    let mut reader =
        lewton::inside_ogg::OggStreamReader::new(Cursor::new(bytes)).map_err(describe)?;
    let count = u32::from(reader.ident_hdr.audio_channels);
    let rate = reader.ident_hdr.audio_sample_rate;
    check_format(count, rate)?;
    let most = most_frames(rate);
    let count = count as usize;
    let mut channels = vec![Vec::new(); count];
    while let Some(packet) = reader.read_dec_packet_itl().map_err(describe)? {
        // A chained file may start another stream of another shape.
        if usize::from(reader.ident_hdr.audio_channels) != count
            || reader.ident_hdr.audio_sample_rate != rate
        {
            return Err("the file changes its format part way".into());
        }
        // The last block may reach past the sound's end (below): that
        // much more than 4 s is let through here.
        if channels[0].len() + packet.len() / count > most + LAST_BLOCK {
            return Err(too_long());
        }
        for (i, s) in packet.iter().enumerate() {
            channels[i % count].push(f64::from(*s) / 32768.0);
        }
    }
    // Vorbis ends in a whole block; where the sound itself ends is the
    // last page's position.
    if let Some(end) = reader.get_last_absgp() {
        let end = usize::try_from(end).unwrap_or(usize::MAX);
        for c in &mut channels {
            c.truncate(end);
        }
    }
    if channels[0].len() > most {
        return Err(too_long());
    }
    Ok(Decoded { rate, channels })
}

/// Decodes a WAV or Ogg Vorbis file to interleaved stereo 16-bit samples
/// at 48 kHz.
///
/// Refused, with the reason: a file over 1 MiB, of another kind than its
/// first bytes promise, with more than 2 channels, a rate outside 8 to
/// 96 kHz, or longer than 4 s; and one the decoder cannot read. Mono is
/// copied to both channels; another rate is resampled. With `gain` the
/// result is scaled so that its loudest sample is that (0 to 1); without,
/// it plays as recorded.
///
/// A decoder that panics is caught and the file refused.
pub fn decode(bytes: &[u8], gain: Option<f64>) -> Result<Vec<i16>, String> {
    if bytes.len() as u64 > limits::FILE_BYTES {
        return Err(format!(
            "{} bytes (a sound file is at most 1 MiB)",
            bytes.len()
        ));
    }
    let kind = sniff(bytes).ok_or("neither a WAV nor an Ogg Vorbis file")?;
    let decoded = std::panic::catch_unwind(|| match kind {
        Kind::Wav => wav(bytes),
        Kind::OggVorbis => ogg(bytes),
    })
    .map_err(|_| "the decoder failed on this file".to_string())??;

    let mut channels: Vec<Vec<f64>> = decoded
        .channels
        .iter()
        .map(|c| resample(c, decoded.rate, RATE))
        .collect();
    // Resampling 4 s can round a frame over.
    let frames = resampled_len(decoded.channels[0].len(), decoded.rate, RATE).min(limits::FRAMES);
    for c in &mut channels {
        c.truncate(frames);
    }
    let (left, right) = (&channels[0], channels.get(1).unwrap_or(&channels[0]));
    let scale = match gain {
        Some(gain) => {
            let peak = left
                .iter()
                .chain(right.iter())
                .fold(0.0f64, |m, v| m.max(v.abs()));
            if peak > 0.0 && gain.is_finite() {
                gain.clamp(0.0, 1.0) * 32767.0 / peak
            } else {
                0.0
            }
        }
        None => 32768.0,
    };
    let mut out = Vec::with_capacity(frames * 2);
    for (l, r) in left.iter().zip(right) {
        out.push((l * scale).round() as i16);
        out.push((r * scale).round() as i16);
    }
    Ok(out)
}
