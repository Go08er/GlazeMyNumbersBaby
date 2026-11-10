//! Still pictures: a PNG, JPEG, WebP or GIF file decoded to RGBA8 by the
//! app's own decoders (docs/theming-media.md, "Decoding stills").

use std::io::Cursor;

use crate::sniff::{Format, check_size, probe};
use crate::stream::Stream;
use crate::{Bytes, Error, broken, shielded};

/// A decoded picture: RGBA8, **premultiplied**, `width × 4` bytes a row,
/// rows from the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Multiplies each pixel's colour by its opacity, in place: `c × a / 255`,
/// rounded to the nearest.
pub fn premultiply(pixels: &mut [u8]) {
    for p in pixels.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        if a == 255 {
            continue;
        }
        for c in &mut p[..3] {
            // Exact rounding of c × a / 255 without a division.
            let t = u32::from(*c) * a + 128;
            *c = ((t + (t >> 8)) >> 8) as u8;
        }
    }
}

/// What a decoder may allocate for a picture of this size: its pixels
/// twice over, and a little for its tables.
pub(crate) fn decoder_room(width: u32, height: u32) -> usize {
    (width as usize) * (height as usize) * 8 + (1 << 20)
}

/// The PNG decoder set up as every PNG here is read: palettes and low bit
/// depths expanded, transparency as an alpha channel, 16 bits reduced to
/// 8, within `room` bytes of its own.
pub(crate) fn png_decoder(bytes: Bytes, room: usize) -> png::Decoder<Cursor<Bytes>> {
    let mut decoder =
        png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: room });
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::ALPHA | png::Transformations::STRIP_16,
    );
    decoder
}

/// One row of a PNG frame as the decoder gives it, to RGBA8 (straight).
pub(crate) fn png_row(color: png::ColorType, row: &[u8], out: &mut [u8]) -> Result<(), Error> {
    match color {
        png::ColorType::Rgba => out.copy_from_slice(row),
        png::ColorType::GrayscaleAlpha => {
            for (o, i) in out
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(row.as_chunks::<2>().0)
            {
                *o = [i[0], i[0], i[0], i[1]];
            }
        }
        png::ColorType::Rgb => {
            for (o, i) in out
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(row.as_chunks::<3>().0)
            {
                *o = [i[0], i[1], i[2], 255];
            }
        }
        png::ColorType::Grayscale => {
            for (o, i) in out.as_chunks_mut::<4>().0.iter_mut().zip(row) {
                *o = [*i, *i, *i, 255];
            }
        }
        png::ColorType::Indexed => return Err(broken("a palette the decoder left in place")),
    }
    Ok(())
}

fn png_still(bytes: Bytes, width: u32, height: u32) -> Result<Vec<u8>, Error> {
    let mut reader = png_decoder(bytes, decoder_room(width, height))
        .read_info()
        .map_err(broken)?;
    let mut buf = vec![0; reader.output_buffer_size()];
    // The first image of the file: of an animated PNG, the picture meant
    // for programs that don't animate.
    let frame = reader.next_frame(&mut buf).map_err(broken)?;
    if (frame.width, frame.height) != (width, height) || frame.bit_depth != png::BitDepth::Eight {
        return Err(broken("the picture isn't the size its header says"));
    }
    if frame.color_type == png::ColorType::Rgba {
        buf.truncate(frame.buffer_size());
        return Ok(buf);
    }
    let mut out = vec![0; width as usize * height as usize * 4];
    let rows = buf.chunks_exact(frame.line_size);
    for (o, i) in out.chunks_exact_mut(width as usize * 4).zip(rows) {
        png_row(frame.color_type, &i[..frame.line_size], o)?;
    }
    Ok(out)
}

fn jpeg_still(bytes: &[u8], width: u32, height: u32, max: u32) -> Result<Vec<u8>, Error> {
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    // zune's portable code (none of its SIMD paths is even built), its
    // default of 100 scans at most, and RGBA out.
    let options = DecoderOptions::new_safe()
        .set_max_width(max as usize)
        .set_max_height(max as usize)
        .jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(broken)?;
    let info = decoder
        .info()
        .ok_or_else(|| broken("the file has no frame header"))?;
    if (u32::from(info.width), u32::from(info.height)) != (width, height) {
        return Err(broken("the picture isn't the size its header says"));
    }
    decoder.decode().map_err(broken)
}

/// RGB at three bytes a pixel in the front of `pixels`, to RGBA in all of
/// it, in place (from the last pixel back, so nothing is overwritten
/// before it is read).
pub(crate) fn expand_rgb(pixels: &mut [u8]) {
    for i in (0..pixels.len() / 4).rev() {
        let (r, g, b) = (pixels[i * 3], pixels[i * 3 + 1], pixels[i * 3 + 2]);
        pixels[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, 255]);
    }
}

/// Decodes a still picture, at most `max_side` wide and tall, to
/// premultiplied RGBA8.
///
/// 1. The kind, by the first bytes; anything but the four is refused.
/// 2. The size, from the header: refused over `max_side`, before any
///    pixel is decoded.
/// 3. The decoder, under its own limits, any panic caught.
///
/// 16-bit PNGs are reduced to 8 bits; colour profiles and rotation tags
/// are ignored. Of a file that moves, the first picture.
pub fn decode_still(bytes: &Bytes, max_side: u32) -> Result<Rgba, Error> {
    let info = probe(bytes)?;
    check_size(&info, max_side)?;
    let (width, height) = (info.width, info.height);
    let mut pixels = shielded(|| match info.format {
        Format::Png => png_still(bytes.clone(), width, height),
        Format::Jpeg => jpeg_still(bytes, width, height, max_side),
        // A GIF's or a WebP's first frame, on its canvas.
        Format::Gif | Format::Webp => {
            let mut stream = Stream::open(bytes.clone(), info.format, width, height)?;
            stream.advance()?;
            Ok(stream.take_canvas())
        }
    })?;
    if pixels.len() != width as usize * height as usize * 4 {
        return Err(broken("the decoder gave a picture of another size"));
    }
    premultiply(&mut pixels);
    Ok(Rgba {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplying_rounds_to_the_nearest() {
        for a in 0..=255u32 {
            for c in [0u32, 1, 2, 127, 128, 254, 255] {
                let mut p = [c as u8, c as u8, c as u8, a as u8];
                premultiply(&mut p);
                let want = ((c * a) as f64 / 255.0).round() as u8;
                assert_eq!(p, [want, want, want, a as u8], "{c} at {a}");
            }
        }
    }

    #[test]
    fn rgb_expands_in_place() {
        let mut p = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 0, 0];
        expand_rgb(&mut p);
        assert_eq!(p, [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255]);
    }
}
