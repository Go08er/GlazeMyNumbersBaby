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

// ---- Vorbis headers, checked before lewton reads them --------------------------------
//
// lewton sets memory aside for what a header says before it reads what
// the header holds: a comment header that claims four billion comments,
// or a codebook of 2^24 entries of 65535 dimensions (a few bytes each),
// asks for hundreds of gigabytes, and a failed allocation aborts the
// process (no `catch_unwind` catches it). So every comment and setup
// header in the file, as lewton's own Ogg reader gives them (a chained
// stream's too), is read here first, bounded by its own size, and the
// file is refused if a header promises more than its bytes can hold or
// more than any sound needs.

/// Codebook entries, all of a file's setup headers together: lewton
/// builds a tree of them. (libvorbis's largest books have a few thousand.)
const VORBIS_ENTRIES: u64 = 1 << 20;
/// Numbers in codebooks' vector tables, all together: lewton spreads
/// every table out to `entries × dimensions` of them.
const VORBIS_VALUES: u64 = 1 << 22;

/// Reads bits least significant first, as Vorbis packs them.
struct BitReader<'a> {
    bytes: &'a [u8],
    at: u64,
}

impl BitReader<'_> {
    fn left(&self) -> u64 {
        (self.bytes.len() as u64 * 8).saturating_sub(self.at)
    }

    fn read(&mut self, bits: u32) -> Option<u64> {
        if u64::from(bits) > self.left() {
            return None;
        }
        let mut v = 0u64;
        for i in 0..u64::from(bits) {
            let at = self.at + i;
            let bit = (self.bytes[(at / 8) as usize] >> (at % 8)) & 1;
            v |= u64::from(bit) << i;
        }
        self.at += u64::from(bits);
        Some(v)
    }

    fn skip(&mut self, bits: u64) -> Option<()> {
        (bits <= self.left()).then(|| self.at += bits)
    }
}

/// The largest `r` with `r ^ dimensions ≤ entries`, as lewton works it
/// out (with its answer for no dimensions).
fn lookup1_values(entries: u64, dimensions: u64) -> u64 {
    if entries == 0 {
        return 0;
    }
    if dimensions == 0 {
        return u64::from(u32::MAX);
    }
    let fits = |base: u64| {
        let mut acc = 1u64;
        for _ in 0..dimensions {
            acc = acc.saturating_mul(base);
            if acc > entries {
                return false;
            }
        }
        true
    };
    let (mut low, mut high) = (1u64, entries);
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if fits(mid) { low = mid } else { high = mid - 1 }
    }
    low
}

/// What a file's setup headers have asked for so far.
#[derive(Default)]
struct Asked {
    entries: u64,
    values: u64,
}

const GREEDY: &str = "a Vorbis header asks for more than a sound needs";
const DAMAGED: &str = "a Vorbis setup header is damaged";

/// A comment header (`\x03vorbis…`): every length within the packet.
fn comments_fit(packet: &[u8]) -> Result<(), String> {
    let mut rest = &packet[7..];
    // A length, and that many bytes after it.
    let field = |rest: &mut &[u8], bytes: bool| -> Result<usize, String> {
        let Some((n, tail)) = rest.split_first_chunk::<4>() else {
            return Err(GREEDY.into());
        };
        let n = u32::from_le_bytes(*n) as usize;
        *rest = tail;
        if bytes {
            *rest = rest.get(n..).ok_or(GREEDY)?;
        }
        Ok(n)
    };
    field(&mut rest, true)?;
    let count = field(&mut rest, false)?;
    // Each comment takes 4 bytes at least.
    if count > rest.len() / 4 {
        return Err(GREEDY.into());
    }
    for _ in 0..count {
        field(&mut rest, true)?;
    }
    Ok(())
}

/// A setup header's codebooks (`\x05vorbis…`), read as lewton reads them:
/// none may describe more than the packet holds, and all of the file's
/// together stay within [`VORBIS_ENTRIES`] and [`VORBIS_VALUES`].
fn codebooks_fit(packet: &[u8], asked: &mut Asked) -> Result<(), String> {
    let mut r = BitReader {
        bytes: &packet[7..],
        at: 0,
    };
    // What lewton refuses by itself (the packet ends, a bad sync pattern,
    // an unknown kind of table), at the same place and before it sets
    // anything aside for it, is refused here as damaged.
    let read = |r: &mut BitReader, bits| r.read(bits).ok_or(DAMAGED);
    let count = read(&mut r, 8)? + 1;
    for _ in 0..count {
        if read(&mut r, 24)? != 0x56_4342 {
            return Err(DAMAGED.into());
        }
        let dimensions = read(&mut r, 16)?;
        let entries = read(&mut r, 24)?;
        asked.entries += entries;
        if asked.entries > VORBIS_ENTRIES {
            return Err(GREEDY.into());
        }
        if read(&mut r, 1)? == 0 {
            let sparse = read(&mut r, 1)? == 1;
            for _ in 0..entries {
                if !sparse || read(&mut r, 1)? == 1 {
                    read(&mut r, 5)?;
                }
            }
        } else {
            read(&mut r, 5)?;
            let mut current = 0u64;
            while current < entries {
                let bits = 64 - (entries - current).leading_zeros();
                current += read(&mut r, bits)?;
            }
            if current > entries {
                return Err(DAMAGED.into());
            }
        }
        let lookup = read(&mut r, 4)?;
        match lookup {
            0 => {}
            1 | 2 => {
                read(&mut r, 32)?;
                read(&mut r, 32)?;
                let value_bits = read(&mut r, 4)? + 1;
                read(&mut r, 1)?;
                let values = if lookup == 1 {
                    lookup1_values(entries, dimensions)
                } else {
                    entries * dimensions
                };
                // lewton sets aside room for the values before it reads
                // them: each must be in the packet. Then it spreads the
                // table out to one number per entry and dimension.
                if values > r.left() / value_bits {
                    return Err(GREEDY.into());
                }
                r.skip(values * value_bits).ok_or(DAMAGED)?;
                asked.values += entries * dimensions;
                if asked.values > VORBIS_VALUES {
                    return Err(GREEDY.into());
                }
            }
            _ => return Err(DAMAGED.into()),
        }
    }
    Ok(())
}

/// Every comment and setup header in an Ogg file, checked before lewton
/// reads them ([`comments_fit`], [`codebooks_fit`]).
fn vorbis_headers_fit(bytes: &[u8]) -> Result<(), String> {
    let mut packets = ogg::PacketReader::new(Cursor::new(bytes));
    let mut asked = Asked::default();
    // Where the reader stops, lewton stops too.
    while let Ok(Some(packet)) = packets.read_packet() {
        if packet.data.starts_with(b"\x03vorbis") {
            comments_fit(&packet.data)?;
        } else if packet.data.starts_with(b"\x05vorbis") {
            codebooks_fit(&packet.data, &mut asked)?;
        }
    }
    Ok(())
}

fn ogg(bytes: &[u8]) -> Result<Decoded, String> {
    vorbis_headers_fit(bytes)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// What a real encoder's files ask for, against the caps: far
    /// under them.
    #[test]
    fn real_vorbis_files_are_far_inside_the_caps() {
        for (name, bytes) in [
            (
                "tone-mono-44100",
                &include_bytes!("../tests/data/tone-mono-44100.ogg")[..],
            ),
            (
                "tone-stereo-48000",
                include_bytes!("../tests/data/tone-stereo-48000.ogg"),
            ),
            (
                "long-mono-8000",
                include_bytes!("../tests/data/long-mono-8000.ogg"),
            ),
            (
                "three-channels",
                include_bytes!("../tests/data/three-channels.ogg"),
            ),
        ] {
            let mut packets = ogg::PacketReader::new(Cursor::new(bytes));
            let mut asked = Asked::default();
            let mut setups = 0;
            while let Ok(Some(p)) = packets.read_packet() {
                if p.data.starts_with(b"\x05vorbis") {
                    codebooks_fit(&p.data, &mut asked).unwrap();
                    setups += 1;
                } else if p.data.starts_with(b"\x03vorbis") {
                    comments_fit(&p.data).unwrap();
                }
            }
            eprintln!("{name}: {} entries, {} values", asked.entries, asked.values);
            assert_eq!(setups, 1, "{name}");
            assert!(
                asked.entries > 0 && asked.entries * 16 < VORBIS_ENTRIES,
                "{name}"
            );
            assert!(
                asked.values > 0 && asked.values * 16 < VORBIS_VALUES,
                "{name}"
            );
        }
    }

    #[test]
    fn lookup1_values_is_the_whole_root() {
        assert_eq!(lookup1_values(1025, 10), 2);
        assert_eq!(lookup1_values(1024, 10), 2);
        assert_eq!(lookup1_values(1023, 10), 1);
        assert_eq!(lookup1_values(3125, 5), 5);
        assert_eq!(lookup1_values(3124, 5), 4);
        assert_eq!(lookup1_values(6561, 8), 3);
        assert_eq!(lookup1_values(1, 1), 1);
        assert_eq!(lookup1_values(0, 15), 0);
        assert_eq!(lookup1_values(0, 0), 0);
        assert_eq!(lookup1_values(1, 0), u64::from(u32::MAX));
        assert_eq!(lookup1_values(0xff_ffff, 65535), 1);
        assert_eq!(lookup1_values(0xff_ffff, 1), 0xff_ffff);
    }
}
