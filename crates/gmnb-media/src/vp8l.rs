//! What a lossless WebP bitstream (a `VP8L` chunk, or the picture inside
//! an `ALPH` chunk) makes `image-webp` 0.2.4 hold for its Huffman codes,
//! read before the decoder is given it (docs/theming-media.md, "A WebP
//! says its size twice").
//!
//! A lossless picture's pixels are coded by groups of five Huffman codes,
//! and a small picture at its start (the entropy image) says which group
//! each block of pixels uses. How many groups there are is the largest
//! number that picture holds, plus one: up to 65 536, whatever the
//! picture's size. The decoder builds every group's codes before it
//! decodes a pixel, a table of up to 1024 entries for each. Measured: a
//! file of 3.8 MB, one pixel, made it hold 1301 MiB for two seconds.
//!
//! So the bitstream is walked first, as the decoder will read it: its
//! transforms and the small pictures they bring are decoded (each at most
//! a sixteenth of the picture), then the entropy image, and then every
//! group's codes are read and what the decoder will make of them added
//! up, without building a table. The decoder is given a bitstream only if
//! that is within [`ROOM`]. A file made by libwebp holds a few hundred
//! groups at most (its encoder's cap is 2600 blocks), each well under
//! 16 KiB.
//!
//! The walk follows `image-webp` 0.2.4's `lossless.rs` step for step, so
//! that the groups counted are the ones it will build: the same order of
//! fields, the same checks, the same colour cache (which, there, is not
//! told of pixels copied from one place back, nor of pixels it gave).

/// The most the codes of one bitstream may make the decoder hold.
pub(crate) const ROOM: u64 = 16 << 20;

const CODE_LENGTH_CODES: usize = 19;
const CODE_LENGTH_CODE_ORDER: [usize; CODE_LENGTH_CODES] = [
    17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
];
const ALPHABET_SIZE: [u16; 5] = [256 + 24, 256, 256, 256, 40];
/// Codes up to this long are looked up in the decoder's table; longer
/// ones go on into a tree.
const TABLE_BITS: u16 = 10;

#[rustfmt::skip]
const DISTANCE_MAP: [(i8, i8); 120] = [
    (0, 1),  (1, 0),  (1, 1),  (-1, 1), (0, 2),  (2, 0),  (1, 2),  (-1, 2),
    (2, 1),  (-2, 1), (2, 2),  (-2, 2), (0, 3),  (3, 0),  (1, 3),  (-1, 3),
    (3, 1),  (-3, 1), (2, 3),  (-2, 3), (3, 2),  (-3, 2), (0, 4),  (4, 0),
    (1, 4),  (-1, 4), (4, 1),  (-4, 1), (3, 3),  (-3, 3), (2, 4),  (-2, 4),
    (4, 2),  (-4, 2), (0, 5),  (3, 4),  (-3, 4), (4, 3),  (-4, 3), (5, 0),
    (1, 5),  (-1, 5), (5, 1),  (-5, 1), (2, 5),  (-2, 5), (5, 2),  (-5, 2),
    (4, 4),  (-4, 4), (3, 5),  (-3, 5), (5, 3),  (-5, 3), (0, 6),  (6, 0),
    (1, 6),  (-1, 6), (6, 1),  (-6, 1), (2, 6),  (-2, 6), (6, 2),  (-6, 2),
    (4, 5),  (-4, 5), (5, 4),  (-5, 4), (3, 6),  (-3, 6), (6, 3),  (-6, 3),
    (0, 7),  (7, 0),  (1, 7),  (-1, 7), (5, 5),  (-5, 5), (7, 1),  (-7, 1),
    (4, 6),  (-4, 6), (6, 4),  (-6, 4), (2, 7),  (-2, 7), (7, 2),  (-7, 2),
    (3, 7),  (-3, 7), (7, 3),  (-7, 3), (5, 6),  (-5, 6), (6, 5),  (-6, 5),
    (8, 0),  (4, 7),  (-4, 7), (7, 4),  (-7, 4), (8, 1),  (8, 2),  (6, 6),
    (-6, 6), (8, 3),  (5, 7),  (-5, 7), (7, 5),  (-7, 5), (8, 4),  (6, 7),
    (-6, 7), (7, 6),  (-7, 6), (8, 5),  (7, 7),  (-7, 7), (8, 6),  (8, 7)
];

/// Why a bitstream isn't handed on: it can't be read (the decoder would
/// refuse it too), or its codes would take more than [`ROOM`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    Unreadable,
    TooBig,
}

type Walk<T> = Result<T, Refused>;

/// Bits from the front, least significant first.
struct Bits<'a> {
    data: &'a [u8],
    /// In bits.
    at: usize,
}

impl Bits<'_> {
    /// The next `n` bits (at most 32), zeros past the end.
    fn peek(&self, n: u32) -> u32 {
        let byte = self.at / 8;
        let mut word = [0u8; 8];
        let end = (byte + 8).min(self.data.len());
        if byte < end {
            word[..end - byte].copy_from_slice(&self.data[byte..end]);
        }
        let v = u64::from_le_bytes(word) >> (self.at % 8);
        (v & ((1u64 << n) - 1)) as u32
    }

    fn consume(&mut self, n: u32) -> Walk<()> {
        if self.at + n as usize > self.data.len() * 8 {
            return Err(Refused::Unreadable);
        }
        self.at += n as usize;
        Ok(())
    }

    fn read(&mut self, n: u32) -> Walk<u32> {
        let v = self.peek(n);
        self.consume(n)?;
        Ok(v)
    }
}

/// A Huffman code as the walk reads with it.
enum Code {
    /// One symbol: read in no bits at all.
    Single(u16),
    /// `(length << 16) | symbol`, by the next `bits` bits.
    Table { table: Vec<u32>, bits: u32 },
}

impl Code {
    fn read(&self, b: &mut Bits) -> Walk<u16> {
        match self {
            Code::Single(symbol) => Ok(*symbol),
            Code::Table { table, bits } => {
                let entry = table[b.peek(*bits) as usize];
                b.consume(entry >> 16)?;
                Ok(entry as u16)
            }
        }
    }
}

/// What the decoder holds for a code of these lengths, and the code: as
/// `HuffmanTree::build_implicit` builds and checks it (none at all, or
/// not complete: refused).
fn build(lengths: &[u16], decode: bool) -> Walk<(Code, u64)> {
    let mut hist = [0u16; 16];
    let mut symbols = 0;
    let mut root = 0;
    for (symbol, &len) in lengths.iter().enumerate() {
        if len != 0 {
            hist[usize::from(len)] += 1;
            if symbols == 0 {
                root = symbol as u16;
            }
            symbols += 1;
        }
    }
    match symbols {
        0 => return Err(Refused::Unreadable),
        1 => return Ok((Code::Single(root), 0)),
        _ => {}
    }
    let max = hist.iter().rposition(|&n| n != 0).unwrap_or(0) as u16;
    let mut next = [0u32; 16];
    let mut code = 0u32;
    for len in 1..=usize::from(max) {
        next[len] = code;
        code = (code + u32::from(hist[len])) << 1;
    }
    if code != 2 << max {
        return Err(Refused::Unreadable);
    }
    // A table of 2^min(max, 10) entries of four bytes, and a tree of
    // nodes of sixteen for the longer codes (twice as many as there are,
    // and room for it to have grown).
    let table_bits = max.min(TABLE_BITS);
    let longer: u64 = hist[usize::from(table_bits) + 1..=usize::from(max)]
        .iter()
        .map(|&n| u64::from(n))
        .sum();
    let held = (4u64 << table_bits) + 16 * (4 * longer + 4);
    if !decode {
        return Ok((Code::Single(0), held));
    }
    let bits = u32::from(max);
    let mut table = vec![0u32; 1 << bits];
    for (symbol, &len) in lengths.iter().enumerate() {
        if len == 0 {
            continue;
        }
        let c = next[usize::from(len)];
        next[usize::from(len)] += 1;
        // The code's first bit is the first read.
        let reversed = c.reverse_bits() >> (32 - u32::from(len));
        let entry = (u32::from(len) << 16) | symbol as u32;
        let mut i = reversed as usize;
        while i < table.len() {
            table[i] = entry;
            i += 1 << len;
        }
    }
    Ok((Code::Table { table, bits }, held))
}

/// The pixels of a picture the stream codes (`[r, g, b, a]`), and what
/// its codes make the decoder hold.
struct Walker<'a> {
    bits: Bits<'a>,
    /// Bytes the decoder will hold for codes, so far.
    held: u64,
    /// The code lengths of the code being read: one buffer for all.
    lengths: Vec<u16>,
}

fn subsample(size: u16, bits: u32) -> u16 {
    ((u32::from(size) + (1 << bits) - 1) >> bits) as u16
}

impl Walker<'_> {
    fn charge(&mut self, bytes: u64) -> Walk<()> {
        self.held += bytes;
        if self.held > ROOM {
            return Err(Refused::TooBig);
        }
        Ok(())
    }

    /// `read_huffman_code`: one code of `alphabet` symbols.
    fn code(&mut self, alphabet: u16, decode: bool) -> Walk<Code> {
        // What the decoder holds for any code: the code itself, in a
        // list that may have grown to twice what it holds.
        self.charge(128)?;
        let b = &mut self.bits;
        if b.read(1)? == 1 {
            let symbols = b.read(1)? + 1;
            let first_8bits = b.read(1)?;
            let zero = b.read(1 + 7 * first_8bits)? as u16;
            if zero >= alphabet {
                return Err(Refused::Unreadable);
            }
            if symbols == 1 {
                return Ok(Code::Single(zero));
            }
            let one = b.read(8)? as u16;
            if one >= alphabet {
                return Err(Refused::Unreadable);
            }
            self.charge(64)?;
            let table = vec![(1 << 16) | u32::from(zero), (1 << 16) | u32::from(one)];
            return Ok(Code::Table { table, bits: 1 });
        }
        let mut code_length_lengths = [0u16; CODE_LENGTH_CODES];
        let count = 4 + b.read(4)? as usize;
        for &at in &CODE_LENGTH_CODE_ORDER[..count] {
            code_length_lengths[at] = b.read(3)? as u16;
        }
        // `read_huffman_code_lengths`.
        let (lengths_code, _) = build(&code_length_lengths, true)?;
        let mut max_symbol = if b.read(1)? == 1 {
            let nbits = 2 + 2 * b.read(3)?;
            let max_minus_two = b.read(nbits)? as u16;
            if max_minus_two > alphabet - 2 {
                return Err(Refused::Unreadable);
            }
            2 + max_minus_two
        } else {
            alphabet
        };
        let mut lengths = std::mem::take(&mut self.lengths);
        lengths.clear();
        lengths.resize(usize::from(alphabet), 0);
        let b = &mut self.bits;
        let mut previous = 8;
        let mut symbol = 0;
        let read = (|| {
            while symbol < alphabet {
                if max_symbol == 0 {
                    break;
                }
                max_symbol -= 1;
                let len = lengths_code.read(b)?;
                if len < 16 {
                    lengths[usize::from(symbol)] = len;
                    symbol += 1;
                    if len != 0 {
                        previous = len;
                    }
                } else {
                    let (extra, offset) = match len - 16 {
                        0 => (2, 3),
                        1 => (3, 3),
                        2 => (7, 11),
                        _ => return Err(Refused::Unreadable),
                    };
                    let repeat = b.read(extra)? as u16 + offset;
                    if symbol + repeat > alphabet {
                        return Err(Refused::Unreadable);
                    }
                    let fill = if len == 16 { previous } else { 0 };
                    lengths[usize::from(symbol)..usize::from(symbol + repeat)].fill(fill);
                    symbol += repeat;
                }
            }
            build(&lengths, decode)
        })();
        self.lengths = lengths;
        let (code, held) = read?;
        self.charge(held)?;
        Ok(code)
    }

    /// `decode_image_stream` of a picture that is not the main one (a
    /// transform's, the entropy image): its pixels, decoded.
    fn picture(&mut self, width: u16, height: u16) -> Walk<Vec<[u8; 4]>> {
        let cache = self.color_cache()?;
        let mut alphabet = ALPHABET_SIZE;
        if let Some(bits) = cache {
            alphabet[0] += 1 << bits;
        }
        let mut group = Vec::with_capacity(5);
        for size in alphabet {
            group.push(self.code(size, true)?);
        }
        self.pixels(width, height, &group, cache)
    }

    /// `read_color_cache`: its size in bits, if there is one.
    fn color_cache(&mut self) -> Walk<Option<u32>> {
        if self.bits.read(1)? == 1 {
            let bits = self.bits.read(4)?;
            if !(1..=11).contains(&bits) {
                return Err(Refused::Unreadable);
            }
            return Ok(Some(bits));
        }
        Ok(None)
    }

    /// `get_copy_distance`.
    fn copy_distance(&mut self, prefix: u16) -> Walk<usize> {
        if prefix < 4 {
            return Ok(usize::from(prefix + 1));
        }
        let extra = u32::from((prefix - 2) >> 1);
        let offset = (2 + (usize::from(prefix) & 1)) << extra;
        Ok(offset + self.bits.read(extra)? as usize + 1)
    }

    /// `decode_image_data` for a picture of one group of codes.
    fn pixels(
        &mut self,
        width: u16,
        height: u16,
        group: &[Code],
        cache_bits: Option<u32>,
    ) -> Walk<Vec<[u8; 4]>> {
        let total = usize::from(width) * usize::from(height);
        let mut data = vec![[0u8; 4]; total];
        let mut cache = cache_bits.map(|bits| (bits, vec![[0u8; 4]; 1 << bits]));
        let insert = |cache: &mut Option<(u32, Vec<[u8; 4]>)>, c: [u8; 4]| {
            if let Some((bits, cache)) = cache {
                let argb = (u32::from(c[0]) << 16)
                    | (u32::from(c[1]) << 8)
                    | u32::from(c[2])
                    | (u32::from(c[3]) << 24);
                cache[(0x1e35_a7bd_u32.wrapping_mul(argb) >> (32 - *bits)) as usize] = c;
            }
        };
        let single = group[..4].iter().all(|c| matches!(c, Code::Single(_)));
        let mut index = 0;
        while index < total {
            // One group throughout: every code single, the first pixel
            // is every pixel.
            if index == 0 && single {
                let green = group[0].read(&mut self.bits)?;
                if green < 256 {
                    let red = group[1].read(&mut self.bits)?;
                    let blue = group[2].read(&mut self.bits)?;
                    let alpha = group[3].read(&mut self.bits)?;
                    let value = [red as u8, green as u8, blue as u8, alpha as u8];
                    data.fill(value);
                    insert(&mut cache, value);
                    break;
                }
            }
            let code = group[0].read(&mut self.bits)?;
            if code < 256 {
                let red = group[1].read(&mut self.bits)? as u8;
                let blue = group[2].read(&mut self.bits)? as u8;
                let alpha = group[3].read(&mut self.bits)? as u8;
                let value = [red, code as u8, blue, alpha];
                data[index] = value;
                insert(&mut cache, value);
                index += 1;
            } else if code < 256 + 24 {
                let length = self.copy_distance(code - 256)?;
                let symbol = group[4].read(&mut self.bits)?;
                let plane = self.copy_distance(symbol)?;
                let dist = if plane > 120 {
                    plane - 120
                } else {
                    let (dx, dy) = DISTANCE_MAP[plane - 1];
                    let d = i32::from(dx) + i32::from(dy) * i32::from(width);
                    d.max(1) as usize
                };
                if index < dist || total - index < length {
                    return Err(Refused::Unreadable);
                }
                for i in index..index + length {
                    data[i] = data[i - dist];
                }
                if dist != 1 {
                    for pixel in &data[index..index + length] {
                        insert(&mut cache, *pixel);
                    }
                }
                index += length;
            } else {
                let Some((_, colors)) = &cache else {
                    return Err(Refused::Unreadable);
                };
                data[index] = colors[usize::from(code - 280)];
                index += 1;
            }
        }
        Ok(data)
    }

    /// `read_transforms`: the width the main picture is coded at.
    fn transforms(&mut self, width: u16, height: u16) -> Walk<u16> {
        let mut xsize = width;
        let mut seen = [false; 4];
        while self.bits.read(1)? == 1 {
            let kind = self.bits.read(2)? as usize;
            if std::mem::replace(&mut seen[kind], true) {
                return Err(Refused::Unreadable);
            }
            match kind {
                0 | 1 => {
                    let size_bits = self.bits.read(3)? + 2;
                    let (w, h) = (subsample(xsize, size_bits), subsample(height, size_bits));
                    self.picture(w, h)?;
                }
                2 => {}
                _ => {
                    let colors = self.bits.read(8)? as u16 + 1;
                    self.picture(colors, 1)?;
                    let bits = match colors {
                        0..=2 => 3,
                        3..=4 => 2,
                        5..=16 => 1,
                        _ => 0,
                    };
                    xsize = subsample(xsize, bits);
                }
            }
        }
        Ok(xsize)
    }

    /// `decode_image_stream` of the main picture, as far as its codes:
    /// every group's, counted.
    fn main(&mut self, xsize: u16, height: u16) -> Walk<()> {
        let cache = self.color_cache()?;
        let mut groups = 1u32;
        if self.bits.read(1)? == 1 {
            let bits = self.bits.read(3)? + 2;
            let (w, h) = (subsample(xsize, bits), subsample(height, bits));
            let entropy = self.picture(w, h)?;
            for pixel in entropy {
                let meta = u32::from(pixel[0]) << 8 | u32::from(pixel[1]);
                groups = groups.max(meta + 1);
            }
            // The entropy image itself, as numbers.
            self.charge(u64::from(w) * u64::from(h) * 2)?;
        }
        let mut alphabet = ALPHABET_SIZE;
        if let Some(bits) = cache {
            alphabet[0] += 1 << bits;
        }
        for _ in 0..groups {
            for size in alphabet {
                self.code(size, false)?;
            }
        }
        Ok(())
    }
}

/// Walks a lossless bitstream: `data` is a `VP8L` chunk's (header first),
/// or with `implicit` the picture inside an `ALPH` chunk, of that size
/// and with no header. `Ok`: the bytes its codes will make the decoder
/// hold, within [`ROOM`].
pub(crate) fn codes_held(data: &[u8], implicit: Option<(u16, u16)>) -> Result<u64, Refused> {
    let mut w = Walker {
        bits: Bits { data, at: 0 },
        held: 0,
        lengths: Vec::new(),
    };
    let (width, height) = match implicit {
        Some(size) => size,
        None => {
            if w.bits.read(8)? != 0x2f {
                return Err(Refused::Unreadable);
            }
            let width = w.bits.read(14)? as u16 + 1;
            let height = w.bits.read(14)? as u16 + 1;
            w.bits.read(1)?;
            if w.bits.read(3)? != 0 {
                return Err(Refused::Unreadable);
            }
            (width, height)
        }
    };
    let xsize = w.transforms(width, height)?;
    w.main(xsize, height)?;
    Ok(w.held)
}
