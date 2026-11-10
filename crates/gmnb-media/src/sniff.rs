//! What a file is, and how big its picture: read from its first bytes,
//! with nothing decoded.

use std::fmt;

use crate::Error;

/// The four kinds of file taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Png,
    Jpeg,
    Webp,
    Gif,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Webp => "WebP",
            Format::Gif => "GIF",
        })
    }
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// The kind of a file, by its magic bytes: `89 50 4E 47 0D 0A 1A 0A` PNG;
/// `FF D8 FF` JPEG; `RIFF....WEBP` WebP; `GIF87a` / `GIF89a` GIF. Anything
/// else is none of them, whatever its name.
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(PNG) {
        Some(Format::Png)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(Format::Jpeg)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(Format::Webp)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(Format::Gif)
    } else {
        None
    }
}

/// What a file's header says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub format: Format,
    /// The picture's size (an animation's canvas), in pixels.
    pub width: u32,
    pub height: u32,
    /// The header says it moves: a PNG with an `acTL` chunk before its
    /// first `IDAT`, a WebP with the animation flag. (A GIF doesn't say:
    /// its frames have to be counted.)
    pub animated: bool,
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 4)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 2)?;
    Some(u32::from(u16::from_le_bytes([b[0], b[1]])))
}

fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 3)?;
    Some(u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16)
}

/// A PNG's chunks from `at`, as (kind, where its data starts, its
/// length), for as long as they are whole.
pub(crate) fn png_chunks(bytes: &[u8]) -> impl Iterator<Item = ([u8; 4], usize, usize)> + '_ {
    let mut at = PNG.len();
    std::iter::from_fn(move || {
        let len = be32(bytes, at)? as usize;
        let kind = bytes.get(at + 4..at + 8)?;
        let kind = [kind[0], kind[1], kind[2], kind[3]];
        let body = at + 8;
        // The data and its checksum must be there.
        let end = body.checked_add(len)?.checked_add(4)?;
        if end > bytes.len() {
            return None;
        }
        at = end;
        Some((kind, body, len))
    })
}

fn png(bytes: &[u8]) -> Option<Info> {
    // The IHDR chunk comes first: length, "IHDR", width, height.
    if bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let (width, height) = (be32(bytes, 16)?, be32(bytes, 20)?);
    let mut animated = false;
    for (kind, _, _) in png_chunks(bytes) {
        match &kind {
            b"acTL" => animated = true,
            b"IDAT" => break,
            _ => {}
        }
    }
    Some(Info {
        format: Format::Png,
        width,
        height,
        animated,
    })
}

fn jpeg(bytes: &[u8]) -> Option<Info> {
    // Walk the segments to the frame header (SOF0..SOF15, but the tables
    // DHT, JPG and DAC that share the range).
    let mut at = 2;
    while at + 4 <= bytes.len() {
        if bytes[at] != 0xff {
            return None;
        }
        let marker = bytes[at + 1];
        if marker == 0xff {
            at += 1;
            continue;
        }
        if (0xd0..=0xd9).contains(&marker) || marker == 0x01 {
            at += 2;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
        if len < 2 {
            return None;
        }
        let frame = (0xc0..=0xcf).contains(&marker) && ![0xc4, 0xc8, 0xcc].contains(&marker);
        if frame {
            let b = bytes.get(at + 5..at + 9)?;
            let height = u32::from(u16::from_be_bytes([b[0], b[1]]));
            let width = u32::from(u16::from_be_bytes([b[2], b[3]]));
            return Some(Info {
                format: Format::Jpeg,
                width,
                height,
                animated: false,
            });
        }
        at += 2 + len;
    }
    None
}

fn webp(bytes: &[u8]) -> Option<Info> {
    // A RIFF file says how long it is: one that is shorter was cut.
    let said = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
    if said.checked_add(8)? > bytes.len() {
        return None;
    }
    let chunk = bytes.get(12..16)?;
    let (width, height, animated) = match chunk {
        // The extended header: flags, then the canvas less one, 24 bits
        // each.
        b"VP8X" => {
            let flags = *bytes.get(20)?;
            (
                le24(bytes, 24)? + 1,
                le24(bytes, 27)? + 1,
                flags & 0x02 != 0,
            )
        }
        // Lossy: a 3-byte frame tag, the start code, then 14 bits each.
        b"VP8 " => {
            if bytes.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            (le16(bytes, 26)? & 0x3fff, le16(bytes, 28)? & 0x3fff, false)
        }
        // Lossless: a signature byte, then 14 bits each, less one.
        b"VP8L" => {
            if *bytes.get(20)? != 0x2f {
                return None;
            }
            let b = bytes.get(21..25)?;
            let bits = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1, false)
        }
        _ => return None,
    };
    Some(Info {
        format: Format::Webp,
        width,
        height,
        animated,
    })
}

fn gif(bytes: &[u8]) -> Option<Info> {
    Some(Info {
        format: Format::Gif,
        width: le16(bytes, 6)?,
        height: le16(bytes, 8)?,
        animated: false,
    })
}

/// A file's kind and its picture's size, from its header alone (a
/// microsecond's work). `Err`: none of the four kinds, or a header cut
/// short (a WebP: a file shorter than it says it is).
pub fn probe(bytes: &[u8]) -> Result<Info, Error> {
    let format = sniff(bytes).ok_or(Error::Unknown)?;
    let info = match format {
        Format::Png => png(bytes),
        Format::Jpeg => jpeg(bytes),
        Format::Webp => webp(bytes),
        Format::Gif => gif(bytes),
    };
    info.ok_or_else(|| crate::broken(format_args!("the {format} file is cut short")))
}

/// Refuses a picture of no size, or wider or taller than `max`.
pub(crate) fn check_size(info: &Info, max: u32) -> Result<(), Error> {
    if info.width == 0 || info.height == 0 || info.width > max || info.height > max {
        return Err(Error::Size {
            width: info.width,
            height: info.height,
            max,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_what_its_first_bytes_say() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nrest"), Some(Format::Png));
        assert_eq!(sniff(&[0xff, 0xd8, 0xff, 0xe0]), Some(Format::Jpeg));
        assert_eq!(sniff(b"RIFF\x10\0\0\0WEBPVP8 "), Some(Format::Webp));
        assert_eq!(sniff(b"GIF89a\x01\0\x01\0"), Some(Format::Gif));
        assert_eq!(sniff(b"GIF87a"), Some(Format::Gif));
        // Close, and not it.
        for other in [
            &b"GIF88a"[..],
            b"RIFF\x10\0\0\0WAVE",
            b"\x89PNG\n",
            &[0xff, 0xd8],
            b"<svg xmlns='http://www.w3.org/2000/svg'/>",
            b"BM",
            b"",
        ] {
            assert_eq!(sniff(other), None, "{other:?}");
            assert_eq!(probe(other), Err(Error::Unknown));
        }
    }

    #[test]
    fn a_header_gives_the_size_without_decoding() {
        // A GIF's logical screen.
        let info = probe(b"GIF89a\x40\x01\xf0\x00\x00\x00\x00").unwrap();
        assert_eq!((info.width, info.height), (320, 240));
        // A lossless WebP: 14 bits each, less one.
        let mut lossless = b"RIFF\x11\0\0\0WEBPVP8L\x05\0\0\0\x2f".to_vec();
        lossless.extend(((99u32) | (49 << 14)).to_le_bytes());
        let info = probe(&lossless).unwrap();
        assert_eq!((info.width, info.height, info.animated), (100, 50, false));
        // An extended WebP with the animation flag.
        let mut extended = b"RIFF\x16\0\0\0WEBPVP8X\x0a\0\0\0\x02\0\0\0".to_vec();
        extended.extend([0xdf, 0x01, 0x00, 0x0d, 0x01, 0x00]);
        let info = probe(&extended).unwrap();
        assert_eq!((info.width, info.height, info.animated), (480, 270, true));
        // A WebP shorter than it says it is was cut.
        extended[4] += 1;
        assert!(matches!(probe(&extended), Err(Error::Broken(_))));
        // A header cut short is refused, not guessed at.
        assert!(matches!(probe(b"GIF89a\x40"), Err(Error::Broken(_))));
        assert!(matches!(
            probe(b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR\0\0"),
            Err(Error::Broken(_))
        ));
        assert!(matches!(
            probe(&[0xff, 0xd8, 0xff, 0xe0, 0x00, 0x02]),
            Err(Error::Broken(_))
        ));
    }
}
