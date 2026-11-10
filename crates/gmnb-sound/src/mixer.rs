//! The mixer (docs/theming-media.md, "The mixer and the stream"): up to
//! twelve voices, each a rendered sound read at its own speed with its own
//! loudness and place, summed in 32-bit floats, scaled by the master gain
//! and limited.
//!
//! It has no thread and no clock of its own: whoever owns the output asks
//! it for the next frames ([`Mixer::mix`]).

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::recipe::limits;

/// How long a voice that has to make room fades, in frames (5 ms).
pub const STEAL_FADE: u32 = 240;
/// How long everything fades when sound stops, in frames (30 ms).
pub const STOP_FADE: u32 = 1440;
/// Below this the limiter leaves a sample alone.
const KNEE: f32 = 0.75;

/// How one sound is played.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayParams {
    /// 0 to 1.
    pub volume: f32,
    /// Speed and with it pitch: 0.5 (an octave down) to 2.
    pub rate: f32,
    /// -1 (left) to 1 (right).
    pub pan: f32,
    /// Seconds to wait before it starts, up to 1.
    pub delay: f32,
}

impl Default for PlayParams {
    fn default() -> PlayParams {
        PlayParams {
            volume: 1.0,
            rate: 1.0,
            pan: 0.0,
            delay: 0.0,
        }
    }
}

impl PlayParams {
    /// The same within the limits of a play; a number that is not one
    /// becomes the default.
    pub fn clamped(self) -> PlayParams {
        let or = |v: f32, default: f32| if v.is_nan() { default } else { v };
        PlayParams {
            volume: or(self.volume, 1.0).clamp(0.0, 1.0),
            rate: or(self.rate, 1.0).clamp(limits::PLAY_RATE.0, limits::PLAY_RATE.1),
            pan: or(self.pan, 0.0).clamp(-1.0, 1.0),
            delay: or(self.delay, 0.0).clamp(0.0, limits::PLAY_DELAY),
        }
    }
}

struct Voice {
    /// Interleaved stereo.
    pcm: Arc<[i16]>,
    /// Where in the sound it is, in frames.
    pos: f64,
    /// Frames of the sound per frame of output.
    step: f64,
    gain_l: f32,
    gain_r: f32,
    /// Frames still to wait before it starts.
    wait: u32,
    /// Frames of fade left, of `fade_len`; 0 of 0 is "not fading".
    fade_left: u32,
    fade_len: u32,
    /// When it was asked for, until its first frame is mixed.
    asked: Option<Instant>,
}

impl Voice {
    fn frames(&self) -> usize {
        self.pcm.len() / 2
    }

    fn ended(&self) -> bool {
        self.pos >= self.frames() as f64 || (self.fade_len > 0 && self.fade_left == 0)
    }

    fn fade(&mut self, frames: u32) {
        // A voice already fading keeps the shorter of the two.
        if self.fade_len == 0 || self.fade_left > frames {
            self.fade_left = frames;
            self.fade_len = frames;
        }
    }

    /// Adds the voice's next frames to `acc` (interleaved stereo).
    fn add_to(&mut self, acc: &mut [f32]) {
        let frames = self.frames();
        let sample = |i: usize| f32::from(self.pcm[i]) / 32768.0;
        for out in acc.as_chunks_mut::<2>().0 {
            if self.wait > 0 {
                self.wait -= 1;
                continue;
            }
            let i = self.pos as usize;
            if i >= frames {
                break;
            }
            let mut gain = 1.0;
            if self.fade_len > 0 {
                if self.fade_left == 0 {
                    break;
                }
                gain = self.fade_left as f32 / self.fade_len as f32;
                self.fade_left -= 1;
            }
            // Between samples: a straight line, to silence past the end.
            let t = (self.pos - i as f64) as f32;
            let (l0, r0) = (sample(i * 2), sample(i * 2 + 1));
            let (l1, r1) = if i + 1 < frames {
                (sample(i * 2 + 2), sample(i * 2 + 3))
            } else {
                (0.0, 0.0)
            };
            out[0] += (l0 + (l1 - l0) * t) * self.gain_l * gain;
            out[1] += (r0 + (r1 - r0) * t) * self.gain_r * gain;
            self.pos += self.step;
        }
    }
}

/// What the mixer has done since it was made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MixStats {
    /// Voices started.
    pub started: u64,
    /// Voices that had to make room for a newer one.
    pub stolen: u64,
    /// Frames mixed.
    pub frames: u64,
    /// The most voices sounding or waiting at once (fading ones apart).
    pub most_voices: usize,
}

pub struct Mixer {
    /// Sounding or waiting, oldest first; at most [`limits::VOICES`].
    voices: Vec<Voice>,
    /// Making room: fading out, at most [`limits::VOICES`] more.
    leaving: Vec<Voice>,
    /// The master gain in use, and the one it is moving to.
    gain: f32,
    gain_to: f32,
    /// Frames mixed since the last voice ended.
    quiet: u64,
    acc: Vec<f32>,
    stats: MixStats,
    /// How long each play waited for its first frames to be mixed (the
    /// first [`LATENCIES`] of them, until taken).
    latencies: Vec<Duration>,
}

/// How many latencies are kept for [`Mixer::take_latencies`].
const LATENCIES: usize = 1024;

impl Default for Mixer {
    fn default() -> Mixer {
        Mixer::new(1.0)
    }
}

impl Mixer {
    /// `gain` is the master gain (the Settings volume squared).
    pub fn new(gain: f32) -> Mixer {
        let gain = if gain.is_nan() {
            0.0
        } else {
            gain.clamp(0.0, 1.0)
        };
        Mixer {
            voices: Vec::with_capacity(limits::VOICES),
            leaving: Vec::with_capacity(limits::VOICES),
            gain,
            gain_to: gain,
            quiet: 0,
            acc: Vec::new(),
            stats: MixStats::default(),
            latencies: Vec::new(),
        }
    }

    /// The master gain from now on; it is reached smoothly, over the next
    /// frames mixed.
    pub fn set_gain(&mut self, gain: f32) {
        if !gain.is_nan() {
            self.gain_to = gain.clamp(0.0, 1.0);
        }
    }

    /// Starts a voice. If twelve are already going, the oldest makes
    /// room: it fades out over 240 frames (one that has not started yet
    /// is simply dropped).
    pub fn play(&mut self, pcm: Arc<[i16]>, params: PlayParams) {
        self.play_asked(pcm, params, None);
    }

    /// [`Mixer::play`], remembering when the play was asked for: how long
    /// it then waits for its first frames is kept for
    /// [`Mixer::take_latencies`].
    pub fn play_asked(&mut self, pcm: Arc<[i16]>, params: PlayParams, asked: Option<Instant>) {
        if pcm.len() < 2 {
            return;
        }
        let p = params.clamped();
        if self.voices.len() >= limits::VOICES {
            let mut old = self.voices.remove(0);
            self.stats.stolen += 1;
            if old.wait == 0 && old.pos > 0.0 {
                old.fade(STEAL_FADE);
                if self.leaving.len() >= limits::VOICES {
                    self.leaving.remove(0);
                }
                self.leaving.push(old);
            }
        }
        let theta = (p.pan + 1.0) * std::f32::consts::FRAC_PI_4;
        self.voices.push(Voice {
            pcm,
            pos: 0.0,
            step: f64::from(p.rate),
            gain_l: p.volume * std::f32::consts::SQRT_2 * theta.cos(),
            gain_r: p.volume * std::f32::consts::SQRT_2 * theta.sin(),
            wait: (p.delay * 48_000.0).round() as u32,
            fade_left: 0,
            fade_len: 0,
            asked,
        });
        self.stats.started += 1;
        self.stats.most_voices = self.stats.most_voices.max(self.voices.len());
        self.quiet = 0;
    }

    /// Every voice fades out over 1440 frames (30 ms); ones still waiting
    /// to start are dropped.
    pub fn stop(&mut self) {
        self.voices.retain(|v| v.wait == 0);
        for v in self.voices.iter_mut().chain(self.leaving.iter_mut()) {
            v.fade(STOP_FADE);
        }
    }

    /// Drops every voice at once.
    pub fn clear(&mut self) {
        self.voices.clear();
        self.leaving.clear();
    }

    /// Voices sounding or waiting to start (not the ones fading out to
    /// make room).
    pub fn voices(&self) -> usize {
        self.voices.len()
    }

    /// Whether anything is sounding, fading or waiting.
    pub fn is_active(&self) -> bool {
        !self.voices.is_empty() || !self.leaving.is_empty()
    }

    /// Frames of silence mixed since the last voice ended.
    pub fn quiet_frames(&self) -> u64 {
        self.quiet
    }

    pub fn stats(&self) -> MixStats {
        self.stats
    }

    /// How long plays waited from being asked for to their first frames
    /// being mixed, since the last call (delayed plays are not counted).
    pub fn take_latencies(&mut self) -> Vec<Duration> {
        std::mem::take(&mut self.latencies)
    }

    /// Fills `out` (interleaved stereo, 16-bit) with the next frames.
    pub fn mix(&mut self, out: &mut [i16]) {
        let frames = out.len() / 2;
        let mut acc = std::mem::take(&mut self.acc);
        acc.clear();
        acc.resize(frames * 2, 0.0);
        if frames > 0 && self.voices.iter().any(|v| v.asked.is_some()) {
            let now = Instant::now();
            for v in &mut self.voices {
                if let Some(asked) = v.asked.take()
                    && v.wait == 0
                    && self.latencies.len() < LATENCIES
                {
                    self.latencies.push(now.saturating_duration_since(asked));
                }
            }
        }
        for v in self.voices.iter_mut().chain(self.leaving.iter_mut()) {
            v.add_to(&mut acc);
        }
        self.voices.retain(|v| !v.ended());
        self.leaving.retain(|v| !v.ended());

        // The master gain, moving in a straight line to where it should
        // be by the end of these frames; then the limiter.
        let (from, to) = (self.gain, self.gain_to);
        let frames_out = out.as_chunks_mut::<2>().0.iter_mut();
        for (i, (frame, mixed)) in frames_out.zip(acc.as_chunks::<2>().0).enumerate() {
            let g = from + (to - from) * ((i + 1) as f32 / frames as f32);
            frame[0] = to_i16(limit(mixed[0] * g));
            frame[1] = to_i16(limit(mixed[1] * g));
        }
        self.gain = to;
        self.acc = acc;
        self.stats.frames += frames as u64;
        if self.is_active() {
            self.quiet = 0;
        } else {
            self.quiet = self.quiet.saturating_add(frames as u64);
        }
    }
}

/// `x` up to 0.75 in size; past that it bends towards 1 and never
/// reaches it.
pub fn limit(x: f32) -> f32 {
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        (KNEE + (1.0 - KNEE) * ((a - KNEE) / (1.0 - KNEE)).tanh()).copysign(x)
    }
}

fn to_i16(x: f32) -> i16 {
    // A float-to-integer cast saturates (and takes NaN to 0).
    (x * 32768.0).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sound of `frames` frames, both channels at `level`.
    fn flat(frames: usize, level: i16) -> Arc<[i16]> {
        vec![level; frames * 2].into()
    }

    fn mix(m: &mut Mixer, frames: usize) -> Vec<i16> {
        let mut out = vec![0; frames * 2];
        m.mix(&mut out);
        out
    }

    #[test]
    fn one_voice_at_full_volume_is_the_sound_itself() {
        let pcm: Arc<[i16]> = (0..2000).map(|i| (i * 13 - 12_000) as i16).collect();
        let mut m = Mixer::new(1.0);
        m.play(pcm.clone(), PlayParams::default());
        let out = mix(&mut m, 1200);
        // Centre is unity on both channels, to within float rounding.
        for (got, want) in out.iter().zip(pcm.iter()) {
            assert!((got - want).abs() <= 1, "{got} {want}");
        }
        assert!(out[2000..].iter().all(|s| *s == 0));
        assert!(!m.is_active());
        assert_eq!(m.quiet_frames(), 1200);
    }

    #[test]
    fn volume_and_master_gain_scale() {
        let mut m = Mixer::new(0.25);
        m.play(
            flat(100, 16_000),
            PlayParams {
                volume: 0.5,
                ..PlayParams::default()
            },
        );
        let out = mix(&mut m, 100);
        assert!(out.iter().all(|s| (s - 2000).abs() <= 1), "{:?}", &out[..4]);
    }

    #[test]
    fn pan_is_equal_power() {
        for (pan, l, r) in [
            (-1.0f32, std::f32::consts::SQRT_2, 0.0f32),
            (0.0, 1.0, 1.0),
            (1.0, 0.0, std::f32::consts::SQRT_2),
            (0.5, 0.5412, 1.3066),
        ] {
            let mut m = Mixer::new(1.0);
            m.play(
                flat(10, 8000),
                PlayParams {
                    pan,
                    ..PlayParams::default()
                },
            );
            let out = mix(&mut m, 10);
            assert!(
                (f32::from(out[0]) - 8000.0 * l).abs() < 3.0,
                "{pan}: {out:?}"
            );
            assert!(
                (f32::from(out[1]) - 8000.0 * r).abs() < 3.0,
                "{pan}: {out:?}"
            );
        }
    }

    #[test]
    fn rate_changes_speed_and_reads_between_samples() {
        // A ramp: at half speed every other output frame lies between two.
        let pcm: Arc<[i16]> = (0..100).flat_map(|i| [i * 100, i * 100]).collect();
        let mut m = Mixer::new(1.0);
        m.play(
            pcm.clone(),
            PlayParams {
                rate: 0.5,
                ..PlayParams::default()
            },
        );
        let out = mix(&mut m, 300);
        assert!((out[2] - 50).abs() <= 1 && (out[4] - 100).abs() <= 1);
        // 100 frames at half speed last 200.
        assert!(out[2 * 198] != 0 && out[2 * 200..].iter().all(|s| *s == 0));
        let mut m = Mixer::new(1.0);
        m.play(
            pcm,
            PlayParams {
                rate: 2.0,
                ..PlayParams::default()
            },
        );
        let out = mix(&mut m, 100);
        assert!((out[2] - 200).abs() <= 1);
        assert!(out[2 * 50..].iter().all(|s| *s == 0));
    }

    #[test]
    fn a_delayed_voice_waits_and_counts_as_active() {
        let mut m = Mixer::new(1.0);
        m.play(
            flat(10, 1000),
            PlayParams {
                delay: 0.01,
                ..PlayParams::default()
            },
        );
        let out = mix(&mut m, 400);
        assert!(out.iter().all(|s| *s == 0));
        assert!(m.is_active());
        assert_eq!(m.quiet_frames(), 0);
        let out = mix(&mut m, 200);
        // 480 frames of waiting: it starts 80 frames into this block.
        assert!(out[..160].iter().all(|s| *s == 0));
        assert!((out[160] - 1000).abs() <= 1);
    }

    #[test]
    fn a_thirteenth_voice_takes_the_oldests_place() {
        let mut m = Mixer::new(1.0);
        for _ in 0..12 {
            m.play(flat(48_000, 100), PlayParams::default());
        }
        mix(&mut m, 64);
        assert_eq!(m.voices(), 12);
        m.play(flat(48_000, 100), PlayParams::default());
        assert_eq!(m.voices(), 12);
        assert_eq!(m.stats().stolen, 1);
        // The oldest fades over 240 frames: thirteen sound at first, then
        // twelve.
        let out = mix(&mut m, 480);
        assert!((out[0] - 1300).abs() <= 2, "{}", out[0]);
        assert!(out[2 * 120] > 1230 && out[2 * 120] < 1270);
        assert!((out[2 * 300] - 1200).abs() <= 2);
    }

    #[test]
    fn a_hundred_plays_at_once_leave_twelve_voices() {
        let mut m = Mixer::new(1.0);
        for _ in 0..100 {
            m.play(flat(4800, 100), PlayParams::default());
        }
        assert_eq!(m.voices(), 12);
        assert_eq!(m.stats().started, 100);
        assert_eq!(m.stats().most_voices, 12);
        // None of the 88 had sounded, so none fades.
        let out = mix(&mut m, 16);
        assert!((out[0] - 1200).abs() <= 2);
    }

    #[test]
    fn the_limiter_is_exact_below_the_knee_and_never_reaches_one() {
        assert_eq!(limit(0.5), 0.5);
        assert_eq!(limit(-0.75), -0.75);
        let just = limit(0.76);
        assert!(just > 0.75 && just < 0.76);
        assert!(limit(1.0) < 0.95 && limit(1.0) > 0.9);
        assert!(limit(100.0) <= 1.0 && limit(-100.0) >= -1.0);
        assert_eq!(limit(-2.0), -limit(2.0));
        // Twelve full-scale voices do not wrap.
        let mut m = Mixer::new(1.0);
        for _ in 0..12 {
            m.play(flat(100, i16::MAX), PlayParams::default());
        }
        let out = mix(&mut m, 100);
        assert!(out.iter().all(|s| *s > 32_000));
        let mut m = Mixer::new(1.0);
        for _ in 0..12 {
            m.play(flat(100, i16::MIN), PlayParams::default());
        }
        assert!(mix(&mut m, 100).iter().all(|s| *s < -32_000));
    }

    #[test]
    fn stopping_fades_everything_within_thirty_milliseconds() {
        let mut m = Mixer::new(1.0);
        m.play(flat(48_000, 10_000), PlayParams::default());
        m.play(
            flat(48_000, 10_000),
            PlayParams {
                delay: 0.5,
                ..PlayParams::default()
            },
        );
        mix(&mut m, 100);
        m.stop();
        let out = mix(&mut m, 2000);
        assert!(out[0] > 9900);
        assert!((out[2 * 720] - 5000).abs() < 30);
        assert!(out[2 * 1440..].iter().all(|s| *s == 0));
        assert!(!m.is_active());
    }

    #[test]
    fn the_master_gain_moves_smoothly() {
        let mut m = Mixer::new(1.0);
        m.play(flat(1000, 10_000), PlayParams::default());
        mix(&mut m, 100);
        m.set_gain(0.0);
        let out = mix(&mut m, 100);
        assert!(out[0] > 9800 && out[2 * 50] > 4000 && out[2 * 50] < 6000);
        assert_eq!(out[2 * 99], 0);
        assert!(mix(&mut m, 100).iter().all(|s| *s == 0));
    }

    #[test]
    fn odd_values_do_no_harm() {
        let mut m = Mixer::new(f32::NAN);
        m.set_gain(f32::NAN);
        m.play(Arc::from(Vec::<i16>::new()), PlayParams::default());
        m.play(
            flat(10, 1000),
            PlayParams {
                volume: f32::NAN,
                rate: f32::INFINITY,
                pan: f32::NEG_INFINITY,
                delay: f32::NAN,
            },
        );
        assert_eq!(m.voices(), 1);
        mix(&mut m, 64);
        mix(&mut m, 0);
        assert!(!m.is_active());
    }
}
