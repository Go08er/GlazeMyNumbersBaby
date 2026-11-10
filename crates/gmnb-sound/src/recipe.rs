//! A synthesised sound as data: what `gmnb.synth` is given, checked
//! (docs/theming-media.md, "Making a sound"). [`crate::synth::render`]
//! turns one into samples.

use core_math as cr;

/// The sample rate everything is rendered and mixed at.
pub const SR: f64 = 48_000.0;
/// The same, as a whole number.
pub const RATE: u32 = 48_000;
pub const TAU: f64 = 6.283185307179586;
/// ln 1000: an "exp" segment falls 60 dB over its length.
pub const K: f64 = 6.907755278982137;
/// The noise generator's seed when a layer gives none (or 0).
pub const DEFAULT_SEED: u32 = 0x9e37_79b9;

/// The chapter's limits ("Limits").
pub mod limits {
    use std::time::Duration;

    /// Sounds a theme may make, good or bad.
    pub const SOUNDS: usize = 32;
    /// The longest sound, in seconds.
    pub const SECONDS: f64 = 4.0;
    /// The longest sound, in frames at 48 kHz.
    pub const FRAMES: usize = 192_000;
    /// All of a theme's sounds together, as 16-bit stereo samples, in
    /// bytes (43 s).
    pub const TOTAL_BYTES: usize = 8 << 20;
    /// A sound file, in bytes.
    pub const FILE_BYTES: u64 = 1 << 20;
    /// A sound file's channels.
    pub const FILE_CHANNELS: u16 = 2;
    /// A sound file's sample rate, lowest and highest.
    pub const FILE_RATE: (u32, u32) = (8_000, 96_000);
    /// A recipe's layers.
    pub const LAYERS: usize = 8;
    /// A recipe's effects.
    pub const EFFECTS: usize = 4;
    /// The layers of all of a theme's recipes together, in seconds.
    pub const LAYER_SECONDS: f64 = 64.0;
    /// Voices sounding at once.
    pub const VOICES: usize = 12;
    /// Plays one event may start.
    pub const PLAYS_PER_EVENT: usize = 4;
    /// The same event is not played again within this.
    pub const REPEAT: Duration = Duration::from_millis(30);
    /// Sounds one `sounds` entry may choose from.
    pub const ENTRY_SOUNDS: usize = 8;
    /// Plays waiting for the sound thread; one more is dropped.
    pub const QUEUE: usize = 64;
    /// A play's `rate`, lowest and highest.
    pub const PLAY_RATE: (f32, f32) = (0.5, 2.0);
    /// A play's `delay`, in seconds, at most.
    pub const PLAY_DELAY: f32 = 1.0;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    Sine,
    Triangle,
    Square,
    Saw,
    Noise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    Linear,
    Exp,
}

/// A value that may glide: `from` to `to` over `time` seconds (`None`:
/// the layer's or the sound's whole length), then stays at `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glide {
    pub from: f64,
    pub to: f64,
    pub time: Option<f64>,
    pub curve: Curve,
}

impl Glide {
    pub const fn fixed(v: f64) -> Glide {
        Glide {
            from: v,
            to: v,
            time: None,
            curve: Curve::Linear,
        }
    }

    pub const fn exp(from: f64, to: f64, time: f64) -> Glide {
        Glide {
            from,
            to,
            time: Some(time),
            curve: Curve::Exp,
        }
    }

    pub const fn lin(from: f64, to: f64, time: f64) -> Glide {
        Glide {
            from,
            to,
            time: Some(time),
            curve: Curve::Linear,
        }
    }

    pub(crate) fn is_fixed(&self) -> bool {
        self.from == self.to
    }

    /// The value `t` seconds in; `whole` is the length `time` defaults to.
    pub(crate) fn at(&self, t: f64, whole: f64) -> f64 {
        if self.from == self.to {
            return self.from;
        }
        let time = self.time.unwrap_or(whole);
        let u = if time <= 0.0 || t >= time {
            1.0
        } else {
            t / time
        };
        match self.curve {
            Curve::Linear => self.from + (self.to - self.from) * u,
            Curve::Exp => self.from * cr::pow(self.to / self.from, u),
        }
    }
}

/// A layer's loudness over time, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Env {
    pub attack: f64,
    pub decay: f64,
    pub sustain: f64,
    pub hold: f64,
    pub release: f64,
    pub curve: Curve,
}

impl Default for Env {
    fn default() -> Env {
        Env {
            attack: 0.002,
            decay: 0.1,
            sustain: 0.0,
            hold: 0.0,
            release: 0.0,
            curve: Curve::Exp,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fm {
    /// The modulator's frequency as a multiple of the layer's.
    pub ratio: f64,
    /// How far the phase is pushed, in radians.
    pub index: Glide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterKind {
    Lowpass,
    Highpass,
    Bandpass,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filter {
    pub kind: FilterKind,
    pub freq: Glide,
    pub q: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lfo {
    /// Hertz.
    pub rate: f64,
    /// Vibrato: cents either way. Tremolo: 0 to 1.
    pub depth: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub wave: Wave,
    /// Hertz. For noise: `None` is a new value every sample; a frequency
    /// holds each value for one period.
    pub freq: Option<Glide>,
    /// Cents.
    pub detune: f64,
    /// Square: the fraction of a period spent high.
    pub width: f64,
    /// Where in its period the wave starts, 0 to 1.
    pub phase: f64,
    pub seed: u32,
    pub fm: Option<Fm>,
    pub vibrato: Option<Lfo>,
    pub tremolo: Option<Lfo>,
    pub filter: Option<Filter>,
    pub env: Env,
    pub gain: f64,
    /// -1 left, 0 centre, 1 right.
    pub pan: f64,
    /// Seconds into the sound at which the layer starts.
    pub at: f64,
}

impl Default for Layer {
    fn default() -> Layer {
        Layer {
            wave: Wave::Sine,
            freq: Some(Glide::fixed(440.0)),
            detune: 0.0,
            width: 0.5,
            phase: 0.0,
            seed: DEFAULT_SEED,
            fm: None,
            vibrato: None,
            tremolo: None,
            filter: None,
            env: Env::default(),
            gain: 1.0,
            pan: 0.0,
            at: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    Filter(Filter),
    Delay {
        time: f64,
        feedback: f64,
        mix: f64,
        ping_pong: bool,
    },
    Reverb {
        size: f64,
        damp: f64,
        mix: f64,
    },
    Drive {
        amount: f64,
    },
    Crush {
        bits: f64,
        rate: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    pub layers: Vec<Layer>,
    pub effects: Vec<Effect>,
    /// The peak of the finished sound, 0 to 1.
    pub gain: f64,
    /// The whole length in seconds, tails included.
    pub length: Option<f64>,
}

impl Default for Recipe {
    fn default() -> Recipe {
        Recipe {
            layers: Vec::new(),
            effects: Vec::new(),
            gain: 0.5,
            length: None,
        }
    }
}

/// `seconds` as a count of samples: to nearest, halves away from zero,
/// never below 0 (and never past `usize`, whatever the number).
pub(crate) fn samples(seconds: f64) -> usize {
    (seconds * SR).round().max(0.0) as usize
}

impl Layer {
    /// The layer's length in samples: the envelope's parts it uses.
    pub fn samples(&self) -> usize {
        let e = &self.env;
        let mut n = samples(e.attack).saturating_add(samples(e.decay));
        if e.sustain > 0.0 {
            n = n
                .saturating_add(samples(e.hold))
                .saturating_add(samples(e.release));
        }
        n
    }
}

impl Recipe {
    /// Whether the renderer takes it: 1 to 8 layers, at most 4 effects.
    /// (Anything else renders as silence.)
    pub fn within_limits(&self) -> bool {
        !self.layers.is_empty()
            && self.layers.len() <= limits::LAYERS
            && self.effects.len() <= limits::EFFECTS
    }

    /// What it counts against a theme's layer-seconds: each layer's
    /// length, none longer than a sound may be.
    pub fn layer_seconds(&self) -> f64 {
        let frames: usize = self
            .layers
            .iter()
            .map(|l| l.samples().min(limits::FRAMES))
            .sum();
        frames as f64 / SR
    }

    /// A hash of everything in it, the same on every machine and in
    /// every build: what a dumped sound's file is named by.
    pub fn fingerprint(&self) -> u64 {
        let mut h = Fnv::new();
        let glide = |h: &mut Fnv, g: &Glide| {
            h.f64(g.from);
            h.f64(g.to);
            h.option(g.time);
            h.byte(g.curve as u8);
        };
        let filter = |h: &mut Fnv, f: &Filter| {
            h.byte(f.kind as u8);
            glide(h, &f.freq);
            h.f64(f.q);
        };
        let lfo = |h: &mut Fnv, l: &Option<Lfo>| match l {
            Some(l) => {
                h.byte(1);
                h.f64(l.rate);
                h.f64(l.depth);
            }
            None => h.byte(0),
        };
        h.f64(self.gain);
        h.option(self.length);
        h.byte(self.layers.len() as u8);
        for l in &self.layers {
            h.byte(l.wave as u8);
            match &l.freq {
                Some(g) => {
                    h.byte(1);
                    glide(&mut h, g);
                }
                None => h.byte(0),
            }
            for v in [l.detune, l.width, l.phase, f64::from(l.seed)] {
                h.f64(v);
            }
            match &l.fm {
                Some(fm) => {
                    h.byte(1);
                    h.f64(fm.ratio);
                    glide(&mut h, &fm.index);
                }
                None => h.byte(0),
            }
            lfo(&mut h, &l.vibrato);
            lfo(&mut h, &l.tremolo);
            match &l.filter {
                Some(f) => {
                    h.byte(1);
                    filter(&mut h, f);
                }
                None => h.byte(0),
            }
            let e = &l.env;
            for v in [e.attack, e.decay, e.sustain, e.hold, e.release] {
                h.f64(v);
            }
            h.byte(e.curve as u8);
            for v in [l.gain, l.pan, l.at] {
                h.f64(v);
            }
        }
        h.byte(self.effects.len() as u8);
        for e in &self.effects {
            match e {
                Effect::Filter(f) => {
                    h.byte(1);
                    filter(&mut h, f);
                }
                Effect::Delay {
                    time,
                    feedback,
                    mix,
                    ping_pong,
                } => {
                    h.byte(2);
                    for v in [*time, *feedback, *mix] {
                        h.f64(v);
                    }
                    h.byte(u8::from(*ping_pong));
                }
                Effect::Reverb { size, damp, mix } => {
                    h.byte(3);
                    for v in [*size, *damp, *mix] {
                        h.f64(v);
                    }
                }
                Effect::Drive { amount } => {
                    h.byte(4);
                    h.f64(*amount);
                }
                Effect::Crush { bits, rate } => {
                    h.byte(5);
                    h.f64(*bits);
                    h.f64(*rate);
                }
            }
        }
        h.finish()
    }
}

/// FNV-1a, 64 bits.
pub(crate) struct Fnv(u64);

impl Fnv {
    pub(crate) fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    pub(crate) fn byte(&mut self, b: u8) {
        self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }

    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.byte(*b);
        }
    }

    fn f64(&mut self, v: f64) {
        self.bytes(&v.to_bits().to_le_bytes());
    }

    fn option(&mut self, v: Option<f64>) {
        match v {
            Some(v) => {
                self.byte(1);
                self.f64(v);
            }
            None => self.byte(0),
        }
    }

    pub(crate) fn finish(&self) -> u64 {
        self.0
    }
}

/// A4 = 440 Hz, twelve equal steps to the octave; `midi` 69 is A4.
pub fn note(midi: f64) -> f64 {
    440.0 * cr::pow(2.0, (midi - 69.0) / 12.0)
}

/// "A4", "C#5", "Eb3": a letter `A` to `G`, an optional `#` or `b`, an
/// octave -1 to 9. C4 is MIDI note 60.
pub fn note_name(name: &str) -> Option<f64> {
    let b = name.as_bytes();
    let semis = match b.first()? {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return None,
    };
    let (acc, rest) = match b.get(1)? {
        b'#' => (1, &name[2..]),
        b'b' => (-1, &name[2..]),
        _ => (0, &name[1..]),
    };
    // Digits, with a minus only for octave -1 ("+4" is no octave).
    if rest.starts_with('+') {
        return None;
    }
    let octave: i32 = rest.parse().ok()?;
    if !(-1..=9).contains(&octave) {
        return None;
    }
    Some(note(f64::from((octave + 1) * 12 + semis + acc)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names() {
        assert_eq!(note_name("A4"), Some(440.0));
        assert_eq!(note_name("A5"), Some(880.0));
        assert_eq!(note_name("C4"), Some(note(60.0)));
        assert_eq!(note_name("C#5"), Some(note(73.0)));
        assert_eq!(note_name("Eb3"), Some(note(51.0)));
        assert_eq!(note_name("C-1"), Some(note(0.0)));
        assert_eq!(note_name("G9"), Some(note(127.0)));
        for bad in [
            "", "H4", "a4", "A", "A10", "A-2", "A+4", "A#", "Ab", "A4 ", "é4", "A♯4",
        ] {
            assert_eq!(note_name(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn samples_are_total() {
        assert_eq!(samples(0.0005), 24);
        assert_eq!(samples(-1.0), 0);
        assert_eq!(samples(f64::NAN), 0);
        assert_eq!(samples(f64::INFINITY), usize::MAX);
        assert_eq!(samples(f64::NEG_INFINITY), 0);
        let long = Layer {
            env: Env {
                attack: f64::INFINITY,
                decay: f64::INFINITY,
                sustain: 1.0,
                hold: 1e300,
                release: 1e300,
                curve: Curve::Exp,
            },
            ..Layer::default()
        };
        assert_eq!(long.samples(), usize::MAX);
    }

    #[test]
    fn fingerprints_tell_recipes_apart() {
        let a = Recipe {
            layers: vec![Layer::default()],
            ..Recipe::default()
        };
        let mut b = a.clone();
        assert_eq!(a.fingerprint(), b.fingerprint());
        b.layers[0].detune = 1.0;
        assert_ne!(a.fingerprint(), b.fingerprint());
        let mut c = a.clone();
        c.effects.push(Effect::Drive { amount: 0.0 });
        assert_ne!(a.fingerprint(), c.fingerprint());
    }
}
