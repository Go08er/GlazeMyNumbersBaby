//! The family names inside a theme's font file, read before the file is
//! handed to the toolkit.
//!
//! A font added to the app's font map stays there until the app quits and
//! is known by the family names inside it. A theme's font that took the
//! name of one the app already has (the base look's "Inter Variable", the
//! desktop's interface font) is picked in its place, in the base look and
//! in Settings, whichever theme is chosen afterwards (round 3, R3-E-1). So
//! the app reads the names first ([`families`]) and leaves out a font whose
//! names are taken ([`clash`]).
//!
//! The file is hostile, and what is read must be what fontconfig will
//! read: only TrueType and OpenType files (and collections of them, which
//! the docs name) are accepted, and anything the reader can't fully
//! account for is refused rather than guessed at: a collection of more
//! than [`MOST_FONTS`] fonts, a `name` table with more than
//! [`MOST_NAMES`] records or two `name` tables in one font, a font with no
//! family name, a family name in an encoding that isn't read here.

use std::collections::HashSet;

/// The `name` table's ids that name a family: the family, the
/// typographic family and the WWS family.
const FAMILY_IDS: [u16; 3] = [1, 16, 21];

/// The most fonts of a collection, and records of a `name` table, read.
pub const MOST_FONTS: usize = 64;
pub const MOST_NAMES: usize = 4096;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        b.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(b: &[u8], at: usize) -> Option<usize> {
    let n = u32::from_be_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?);
    usize::try_from(n).ok()
}

/// How a family name is compared: fontconfig ignores case (by Unicode's
/// folding: the round trip through upper case is what a `ſ` or `ß` needs)
/// and blanks, and reads a name to its first NUL.
pub fn normal(name: &str) -> String {
    let name = name.split('\0').next().unwrap_or("");
    name.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .to_uppercase()
        .to_lowercase()
}

/// The family names of a font file's bytes, [`normal`]ised, or `None`
/// when the bytes are not a TrueType or OpenType font (or a collection of
/// them) whose names can all be read.
pub fn families(bytes: &[u8]) -> Option<Vec<String>> {
    let mut starts = Vec::new();
    match bytes.get(..4)? {
        b"ttcf" => {
            let count = u32_at(bytes, 8)?;
            if count == 0 || count > MOST_FONTS {
                return None;
            }
            for i in 0..count {
                starts.push(u32_at(bytes, 12 + 4 * i)?);
            }
        }
        [0, 1, 0, 0] | b"OTTO" | b"true" => starts.push(0),
        _ => return None,
    }
    let mut out = Vec::new();
    for start in starts {
        names_of(bytes, start, &mut out)?;
    }
    out.sort();
    out.dedup();
    Some(out)
}

/// Adds the family names of the font whose table directory starts at
/// `start`; `None` if there are none, or any can't be read.
fn names_of(b: &[u8], start: usize, out: &mut Vec<String>) -> Option<()> {
    let tables = usize::from(u16_at(b, start.checked_add(4)?)?);
    let mut name = None;
    for i in 0..tables {
        let record = start.checked_add(12 + 16 * i)?;
        if b.get(record..record.checked_add(4)?)? == b"name" {
            if name.is_some() {
                // Which of two FreeType would take isn't asked.
                return None;
            }
            name = Some((u32_at(b, record + 8)?, u32_at(b, record + 12)?));
        }
    }
    let (table, length) = name?;
    let t = b.get(table..table.checked_add(length)?)?;
    let count = usize::from(u16_at(t, 2)?);
    if count > MOST_NAMES {
        return None;
    }
    let strings = usize::from(u16_at(t, 4)?);
    let before = out.len();
    for i in 0..count {
        let r = 6 + 12 * i;
        let (platform, encoding, id) = (u16_at(t, r)?, u16_at(t, r + 2)?, u16_at(t, r + 6)?);
        if !FAMILY_IDS.contains(&id) {
            continue;
        }
        let (len, off) = (
            usize::from(u16_at(t, r + 8)?),
            usize::from(u16_at(t, r + 10)?),
        );
        let from = strings.checked_add(off)?;
        let raw = t.get(from..from.checked_add(len)?)?;
        let text = match (platform, encoding) {
            // Unicode, Windows (symbol or Unicode): UTF-16 big-endian.
            (0, _) | (3, 0 | 1) => {
                if raw.len() % 2 != 0 {
                    return None;
                }
                let units: Vec<u16> = raw
                    .chunks_exact(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16(&units).ok()?
            }
            // Macintosh Roman: its ASCII; a byte above is a letter that
            // isn't read here.
            (1, 0) if raw.is_ascii() => raw.iter().map(|&c| char::from(c)).collect(),
            _ => return None,
        };
        out.push(normal(&text));
    }
    // A font with no family name is named by something else.
    (out.len() > before).then_some(())
}

/// A name of `families` that is already `taken` (both [`normal`]ised).
pub fn clash<'a>(families: &'a [String], taken: &HashSet<String>) -> Option<&'a str> {
    families
        .iter()
        .find(|f| taken.contains(*f))
        .map(String::as_str)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A font of one table, `name`, with these records: platform,
    /// encoding, name id and the string's bytes.
    pub(crate) fn font_with(records: &[(u16, u16, u16, &[u8])]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut table = vec![0, 0];
        table.extend((records.len() as u16).to_be_bytes());
        table.extend((6 + 12 * records.len() as u16).to_be_bytes());
        for (platform, encoding, id, text) in records {
            for v in [
                *platform,
                *encoding,
                0x409,
                *id,
                text.len() as u16,
                strings.len() as u16,
            ] {
                table.extend(v.to_be_bytes());
            }
            strings.extend(*text);
        }
        table.extend(&strings);
        let mut f = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        f.extend(b"name");
        f.extend([0u8; 4]);
        f.extend(28u32.to_be_bytes());
        f.extend((table.len() as u32).to_be_bytes());
        f.extend(table);
        f
    }

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(u16::to_be_bytes).collect()
    }

    /// A font named `family` as Windows would.
    pub(crate) fn font_named(family: &str) -> Vec<u8> {
        font_with(&[(3, 1, 1, &utf16(family))])
    }

    fn taken(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| normal(n)).collect()
    }

    #[test]
    fn a_fonts_family_is_read_and_compared_as_fontconfig_does() {
        let f = font_named("Inter Variable");
        assert_eq!(families(&f), Some(vec!["intervariable".to_string()]));
        let t = taken(&["Inter Variable"]);
        assert_eq!(clash(&families(&f).unwrap(), &t), Some("intervariable"));
        assert_eq!(
            clash(&families(&font_named("Jersey 10")).unwrap(), &t),
            None
        );
        // Case, blanks, a NUL's tail, Mac and Unicode platforms, every
        // family id.
        for f in [
            font_named("INTER  variable"),
            font_named("Inter Variable\0x"),
            font_with(&[(1, 0, 1, b"Inter Variable")]),
            font_with(&[(0, 3, 16, &utf16("Inter Variable"))]),
            font_with(&[(3, 1, 21, &utf16("Inter Variable"))]),
            // Among others that don't clash.
            font_with(&[
                (3, 1, 1, &utf16("Fine")),
                (3, 1, 16, &utf16("Inter Variable")),
            ]),
        ] {
            let names = families(&f).unwrap();
            assert!(clash(&names, &t).is_some(), "{names:?}");
        }
        // A letter fontconfig folds to another ("ſ" to "s", "ß" to "ss").
        let t = taken(&["Noto Sans Math", "Strasse"]);
        for name in ["Noto Sanſ Math", "Straße"] {
            let names = families(&font_named(name)).unwrap();
            assert!(clash(&names, &t).is_some(), "{name}: {names:?}");
        }
    }

    #[test]
    fn the_built_in_fonts_are_read() {
        for b in &crate::theme::BUILT_IN {
            for (name, bytes) in b.files.iter().filter(|(n, _)| n.ends_with(".ttf")) {
                let got = families(bytes).unwrap_or_else(|| panic!("{name}"));
                assert!(!got.is_empty(), "{name}");
            }
        }
    }

    /// What can't be fully accounted for is refused, not guessed at: the
    /// clash could be in what is left unread.
    #[test]
    fn what_cannot_be_accounted_for_is_refused() {
        let ok = utf16("Fine");
        // No family name at all (fontconfig falls back to another name).
        assert_eq!(families(&font_with(&[(3, 1, 4, &ok)])), None);
        assert_eq!(families(&font_with(&[])), None);
        // An encoding that isn't read, an odd UTF-16 length, a Mac string
        // with a byte above ASCII, UTF-16 that isn't.
        assert_eq!(families(&font_with(&[(3, 10, 1, &ok)])), None);
        assert_eq!(families(&font_with(&[(3, 1, 1, &ok[..3])])), None);
        assert_eq!(families(&font_with(&[(1, 0, 1, b"Caf\x8e")])), None);
        assert_eq!(families(&font_with(&[(3, 1, 1, &[0xd8, 0, 0, 65])])), None);
        // More records than are read (one of the last is the family).
        let many: Vec<(u16, u16, u16, &[u8])> = (0..=MOST_NAMES)
            .map(|i| (3, 1, if i == MOST_NAMES { 1 } else { 4 }, &ok[..]))
            .collect();
        assert_eq!(families(&font_with(&many)), None);
        // Two `name` tables in one font.
        let mut twice = font_named("Fine");
        twice[5] = 2;
        let record = twice[12..28].to_vec();
        twice.splice(28..28, record);
        // (the table's offset is now 16 bytes on)
        twice[20..24].copy_from_slice(&44u32.to_be_bytes());
        twice[36..40].copy_from_slice(&44u32.to_be_bytes());
        assert_eq!(families(&twice), None);
        // A collection of more than 64 fonts, or none.
        for count in [0u32, MOST_FONTS as u32 + 1] {
            let mut c = b"ttcf\0\x01\0\0".to_vec();
            c.extend(count.to_be_bytes());
            c.extend(vec![0u8; 4 * (count as usize)]);
            assert_eq!(families(&c), None, "{count}");
        }
    }

    /// A collection's every font is read.
    #[test]
    fn every_font_of_a_collection_is_read() {
        let (a, b) = (font_named("Fine"), font_named("Inter Variable"));
        let mut c = b"ttcf\0\x01\0\0\0\0\0\x02".to_vec();
        let first = 20;
        c.extend((first as u32).to_be_bytes());
        // The second font starts after the first; its table offsets are
        // its own file's, so it is placed at the start's offset.
        let mut second = b.clone();
        let at = first + a.len();
        c.extend((at as u32).to_be_bytes());
        let shift = |f: &mut Vec<u8>, by: usize| {
            let old = u32::from_be_bytes(f[20..24].try_into().unwrap()) as usize;
            f[20..24].copy_from_slice(&((old + by) as u32).to_be_bytes());
        };
        let mut first_font = a.clone();
        shift(&mut first_font, first);
        shift(&mut second, at);
        c.extend(first_font);
        c.extend(second);
        let names = families(&c).unwrap();
        assert_eq!(names, vec!["fine".to_string(), "intervariable".to_string()]);
    }

    /// Whatever a hostile file says, reading it is a bounded walk over its
    /// own bytes: nothing panics, and what isn't a font isn't read.
    #[test]
    fn hostile_bytes_are_refused_not_followed() {
        assert_eq!(families(b""), None);
        assert_eq!(families(b"\0\x01\0\0"), None);
        assert_eq!(families(b"<svg xmlns=''></svg>"), None);
        assert_eq!(
            families(&[b't', b't', b'c', b'f', 0, 1, 0, 0, 255, 255, 255, 255]),
            None
        );
        let good = font_named("Evil");
        for cut in 0..good.len() {
            let _ = families(&good[..cut]);
        }
        // A table directory of 65 535 tables, a name table that lies.
        let mut f = vec![0, 1, 0, 0, 255, 255];
        f.resize(12, 0);
        f.extend(vec![0xff; 4096]);
        assert_eq!(families(&f), None);
        let mut lie = good.clone();
        let n = lie.len();
        lie[24..28].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(families(&lie), None);
        lie[24..28].copy_from_slice(&(n as u32).to_be_bytes());
        assert_eq!(families(&lie), None);
    }
}
