//! An animation's timeline: when each frame starts, read from the file
//! without decoding a pixel (docs/theming-media.md, "The animation
//! player", opening).

use std::io::Cursor;

use crate::sniff::{Format, check_size, png_chunks, probe};
use crate::stream::webp_frames;
use crate::{Bytes, Error, broken, shielded};

/// The shortest a frame is shown: at most 30 frames a second reach the
/// screen. A faster animation keeps its speed and skips frames.
pub const MIN_SHOWN: f64 = 1.0 / 30.0;

/// A delay shorter than this (in GIFs usually a mistake for "as fast as
/// possible") is taken as [`SHORT_BECOMES`], as browsers do.
const SHORTEST: f64 = 0.02;
const SHORT_BECOMES: f64 = 0.1;
/// The longest a frame stays.
const LONGEST: f64 = 10.0;

/// What an animation may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimLimits {
    /// The canvas's width and height.
    pub side: u32,
    pub frames: u32,
}

impl Default for AnimLimits {
    fn default() -> Self {
        AnimLimits {
            side: 1024,
            frames: 2048,
        }
    }
}

/// When each frame of an animation starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Timeline {
    pub format: Format,
    /// The canvas, in pixels.
    pub width: u32,
    pub height: u32,
    /// Each frame's start, in seconds from the start of a pass.
    starts: Vec<f64>,
    /// One pass, in seconds. 0 for a still (one frame with no delay of
    /// its own).
    pub duration: f64,
    /// Bytes the player keeps beside the frames: the largest part of the
    /// canvas a frame asks to have put back as it was (GIF disposal 3,
    /// APNG `previous`), at four bytes a pixel. 0 for most files.
    pub restore: u64,
}

/// A frame's delay as it is played.
fn delay(seconds: f64) -> f64 {
    // (Not a number is no delay at all.)
    if seconds.is_nan() || seconds < SHORTEST {
        SHORT_BECOMES
    } else {
        seconds.min(LONGEST)
    }
}

impl Timeline {
    /// A timeline of frames with these delays, in seconds as the file
    /// gives them.
    fn new(format: Format, width: u32, height: u32, delays: &[f64], restore: u64) -> Timeline {
        let mut starts = Vec::with_capacity(delays.len());
        let mut at = 0.0;
        for d in delays {
            starts.push(at);
            at += delay(*d);
        }
        Timeline {
            format,
            width,
            height,
            starts,
            duration: if delays.len() > 1 { at } else { 0.0 },
            restore,
        }
    }

    /// A still: one frame.
    fn still(format: Format, width: u32, height: u32) -> Timeline {
        Timeline {
            format,
            width,
            height,
            starts: vec![0.0],
            duration: 0.0,
            restore: 0,
        }
    }

    /// For tests of the clock: frames with these delays.
    pub fn of_delays(format: Format, width: u32, height: u32, delays: &[f64]) -> Timeline {
        Timeline::new(format, width, height, delays, 0)
    }

    pub fn frames(&self) -> u32 {
        self.starts.len() as u32
    }

    /// Whether it moves at all.
    pub fn moves(&self) -> bool {
        self.starts.len() > 1
    }

    /// When frame `i` starts, in seconds from the start of a pass.
    pub fn start(&self, i: u32) -> f64 {
        self.starts.get(i as usize).copied().unwrap_or(0.0)
    }

    /// One frame's bytes: the canvas at four bytes a pixel.
    pub fn frame_bytes(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height) * 4
    }

    /// What the animation is counted as against a theme's image total,
    /// in bytes: what playing it holds at most, counted (the crate's
    /// `memory` test).
    ///
    /// Four frames are the player's: the first (which stands for the
    /// animation when nothing moves), the one shown, the one ahead or
    /// being decoded, and the canvas the frames are composited on. On
    /// top comes what the format's decoder keeps: for a GIF its frame as
    /// palette indices (a byte a pixel); for a PNG up to two frames of
    /// its own buffers; for a WebP the frame being decoded, for as long
    /// as that takes. And [`Timeline::restore`].
    pub fn charge(&self) -> u64 {
        let frame = self.frame_bytes();
        let decoder = match self.format {
            Format::Gif => frame / 4,
            Format::Png => frame * 2,
            Format::Webp | Format::Jpeg => frame,
        };
        frame * 4 + decoder + self.restore
    }

    /// The mean time a frame is shown, in seconds.
    pub fn mean_delay(&self) -> f64 {
        if self.moves() {
            self.duration / self.starts.len() as f64
        } else {
            0.0
        }
    }

    /// Where in a pass `position` (seconds) is: wrapped around when
    /// `looped`, else held at the ends.
    fn within(&self, position: f64, looped: bool) -> f64 {
        if !self.moves() || !position.is_finite() {
            return 0.0;
        }
        if looped {
            position.rem_euclid(self.duration)
        } else {
            position.clamp(0.0, self.duration)
        }
    }

    /// The frame due at `position`: the last one whose start is at or
    /// before it. Past the end, `looped` wraps around; otherwise the last
    /// frame is held.
    pub fn frame_at(&self, position: f64, looped: bool) -> u32 {
        let p = self.within(position, looped);
        // The first start after `p`, less one.
        let after = self.starts.partition_point(|s| *s <= p);
        (after.max(1) - 1) as u32
    }

    /// Seconds from `position` (on a looping clock) until the next frame
    /// is due: the start of the frame after the one showing, and never
    /// sooner than [`MIN_SHOWN`]. `None` for a still.
    pub fn next_due(&self, position: f64) -> Option<f64> {
        if !self.moves() {
            return None;
        }
        let p = self.within(position, true);
        let next = self.frame_at(p, true) as usize + 1;
        let start = self.starts.get(next).copied().unwrap_or(self.duration);
        Some((start - p).max(MIN_SHOWN))
    }

    /// Seconds from `position` (on a looping clock) until the frame after
    /// `frame` starts, and never sooner than [`MIN_SHOWN`]: when a layer
    /// that shows `frame` at `position` is to be drawn again. `None` for
    /// a still.
    ///
    /// (`frame` is the caller's choice: a clock that wakes a hair before
    /// a frame's start may show that frame already.)
    pub fn due_after(&self, frame: u32, position: f64) -> Option<f64> {
        if !self.moves() {
            return None;
        }
        let p = self.within(position, true);
        let next = frame as usize + 1;
        let start = self.starts.get(next).copied().unwrap_or(self.duration);
        let mut wait = start - p;
        // The frame shown is of the pass after the one `position` is
        // still in the last moments of.
        if wait < -self.duration / 2.0 {
            wait += self.duration;
        }
        Some(wait.max(MIN_SHOWN))
    }

    /// The frame that follows `i`, the first again after the last.
    pub fn after(&self, i: u32) -> u32 {
        if i + 1 >= self.frames() { 0 } else { i + 1 }
    }
}

fn be16(bytes: &[u8], at: usize) -> u32 {
    u32::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]))
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn too_many(limits: &AnimLimits) -> Error {
    Error::Frames { max: limits.frames }
}

/// A GIF's frames, by its decoder with frame decoding skipped.
fn gif(bytes: &Bytes, width: u32, height: u32, limits: &AnimLimits) -> Result<Timeline, Error> {
    let mut options = gif::DecodeOptions::new();
    options.skip_frame_decoding(true);
    let mut decoder = options
        .read_info(Cursor::new(bytes.clone()))
        .map_err(broken)?;
    let mut delays = Vec::new();
    let mut restore = 0;
    while let Some(frame) = decoder.next_frame_info().map_err(broken)? {
        if delays.len() as u32 >= limits.frames {
            return Err(too_many(limits));
        }
        // Hundredths of a second.
        delays.push(f64::from(frame.delay) / 100.0);
        if frame.dispose == gif::DisposalMethod::Previous {
            let w = u64::from(frame.width).min(u64::from(width));
            let h = u64::from(frame.height).min(u64::from(height));
            restore = restore.max(w * h * 4);
        }
    }
    if delays.is_empty() {
        return Err(broken("the GIF file has no picture"));
    }
    Ok(Timeline::new(Format::Gif, width, height, &delays, restore))
}

/// An animated PNG's frames, by its `fcTL` chunks: nothing is inflated.
fn apng(bytes: &[u8], width: u32, height: u32, limits: &AnimLimits) -> Result<Timeline, Error> {
    let mut delays = Vec::new();
    let mut restore = 0;
    let mut announced = None;
    for (kind, body, len) in png_chunks(bytes) {
        match &kind {
            b"acTL" if len >= 8 => announced = Some(be32(bytes, body)),
            b"fcTL" if len >= 26 => {
                if delays.len() as u32 >= limits.frames {
                    return Err(too_many(limits));
                }
                let (num, den) = (be16(bytes, body + 20), be16(bytes, body + 22));
                // A denominator of 0 is 100.
                delays.push(f64::from(num) / f64::from(if den == 0 { 100 } else { den }));
                // dispose_op 2: put back what was there before the frame.
                if bytes[body + 24] == 2 {
                    let w = u64::from(be32(bytes, body + 4)).min(u64::from(width));
                    let h = u64::from(be32(bytes, body + 8)).min(u64::from(height));
                    restore = restore.max(w * h * 4);
                }
            }
            b"IEND" => break,
            _ => {}
        }
    }
    if announced.is_some_and(|n| n > limits.frames) {
        return Err(too_many(limits));
    }
    // The decoder goes by what `acTL` announces: a file that announces
    // other than it holds would end early or late.
    if announced != Some(delays.len() as u32) || delays.is_empty() {
        return Err(broken(
            "the animated PNG doesn't hold the frames it announces",
        ));
    }
    Ok(Timeline::new(Format::Png, width, height, &delays, restore))
}

/// An animated WebP's frames, by its `ANMF` chunks.
fn webp(bytes: &Bytes, width: u32, height: u32, limits: &AnimLimits) -> Result<Timeline, Error> {
    let frames = webp_frames(bytes, limits.frames as usize).ok_or_else(|| too_many(limits))?;
    // Milliseconds.
    let delays: Vec<f64> = frames
        .iter()
        .map(|f| f64::from(f.duration) / 1000.0)
        .collect();
    // The decoder walks the file its own way: both must find the same.
    let decoder = image_webp::WebPDecoder::new(Cursor::new(bytes.clone())).map_err(broken)?;
    if !decoder.is_animated()
        || decoder.num_frames() as usize != delays.len()
        || decoder.dimensions() != (width, height)
        || delays.is_empty()
    {
        return Err(broken(
            "the animated WebP doesn't hold the frames it announces",
        ));
    }
    Ok(Timeline::new(Format::Webp, width, height, &delays, 0))
}

/// Opens an animation: its kind by its first bytes, its canvas from its
/// header (refused over [`AnimLimits::side`]), and its timeline by a walk
/// that decodes no pixel (refused over [`AnimLimits::frames`]).
///
/// A GIF, a PNG with an `acTL` chunk before its `IDAT`, or a WebP with
/// the animation flag. A still PNG, WebP or GIF is an animation of one
/// frame. A JPEG is refused.
pub fn open(bytes: &Bytes, limits: &AnimLimits) -> Result<Timeline, Error> {
    let info = probe(bytes)?;
    if info.format == Format::Jpeg {
        return Err(Error::NotAnimation(info.format));
    }
    check_size(&info, limits.side)?;
    let (w, h) = (info.width, info.height);
    shielded(|| match (info.format, info.animated) {
        (Format::Gif, _) => gif(bytes, w, h, limits),
        (Format::Png, true) => apng(bytes, w, h, limits),
        (Format::Webp, true) => webp(bytes, w, h, limits),
        (format, _) => Ok(Timeline::still(format, w, h)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(delays: &[f64]) -> Timeline {
        Timeline::of_delays(Format::Gif, 4, 4, delays)
    }

    #[test]
    fn delays_are_the_files_within_reason() {
        // Under 20 ms (0 above all): 100 ms. Over 10 s: 10 s.
        let t = line(&[0.0, 0.01, 0.02, 0.5, 60.0]);
        let want = [0.0, 0.1, 0.2, 0.22, 0.72];
        for (i, w) in want.iter().enumerate() {
            assert!((t.start(i as u32) - w).abs() < 1e-9, "frame {i}");
        }
        assert!((t.duration - 10.72).abs() < 1e-9);
        assert!((line(&[f64::NAN, 0.1]).duration - 0.2).abs() < 1e-9);
        assert!((t.mean_delay() - 10.72 / 5.0).abs() < 1e-9);
    }

    #[test]
    fn the_frame_due_is_the_last_that_has_started() {
        // Delays a binary fraction can hold, so the starts are exact.
        let t = line(&[0.125, 0.25, 0.375]);
        assert_eq!(t.frames(), 3);
        assert_eq!(t.duration, 0.75);
        for (at, frame) in [
            (0.0, 0),
            (0.124, 0),
            (0.125, 1),
            (0.37, 1),
            (0.375, 2),
            (0.74, 2),
        ] {
            assert_eq!(t.frame_at(at, true), frame, "at {at}");
            assert_eq!(t.frame_at(at, false), frame, "at {at}");
        }
        // Past the end it wraps to the first frame, or holds the last.
        assert_eq!(t.frame_at(0.75, true), 0);
        assert_eq!(t.frame_at(0.75 + 0.25, true), 1);
        assert_eq!(t.frame_at(6.0 + 0.5, true), 2);
        assert_eq!(t.frame_at(0.75, false), 2);
        assert_eq!(t.frame_at(99.0, false), 2);
        // Before the start: wrapped from the end, or held at the first.
        assert_eq!(t.frame_at(-0.1, true), 2);
        assert_eq!(t.frame_at(-0.1, false), 0);
        assert_eq!(t.frame_at(f64::NAN, true), 0);
        assert_eq!(t.after(0), 1);
        assert_eq!(t.after(2), 0);
    }

    #[test]
    fn the_next_frame_is_due_at_its_start_and_never_within_a_thirtieth() {
        let t = line(&[0.125, 0.25, 0.375]);
        assert_eq!(t.next_due(0.0), Some(0.125));
        assert_eq!(t.next_due(0.25), Some(0.125));
        // The last frame's successor is the first of the next pass.
        assert_eq!(t.next_due(0.5), Some(0.25));
        assert_eq!(t.next_due(0.75 + 0.5), Some(0.25));
        // Close to a boundary: not sooner than 1/30 s.
        assert_eq!(t.next_due(0.124), Some(MIN_SHOWN));
        // A 50-a-second animation is shown 30 times a second.
        let fast = line(&[0.02; 10]);
        assert_eq!(fast.next_due(0.0), Some(MIN_SHOWN));
        // After a frame of the caller's choosing: a clock that woke a
        // millisecond before frame 1 starts and shows it already waits
        // for frame 2 from where it is.
        assert_eq!(t.due_after(1, 0.124), Some(0.251));
        assert_eq!(t.due_after(0, 0.0), Some(0.125));
        // The same at the loop's end: the first frame of the next pass,
        // then its second.
        assert!((t.due_after(0, 0.749).unwrap() - 0.126).abs() < 1e-12);
        assert_eq!(t.due_after(2, 0.5), Some(0.25));
        assert_eq!(t.due_after(2, 0.749), Some(MIN_SHOWN));
        // A still asks for nothing.
        assert_eq!(line(&[0.5]).due_after(0, 0.0), None);
        assert_eq!(line(&[0.5]).next_due(0.0), None);
        assert_eq!(line(&[0.5]).frame_at(3.0, true), 0);
        assert_eq!(line(&[0.5]).duration, 0.0);
    }
}
