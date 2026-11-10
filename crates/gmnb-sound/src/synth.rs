//! The synthesiser (docs/theming-media.md, "The synthesiser, exactly"): a
//! recipe rendered once into interleaved stereo 16-bit samples at 48 kHz.
//!
//! The same recipe gives the same samples on every machine and in every
//! build. All arithmetic is binary64 in the order written here, each
//! operation rounded once (Rust never fuses a multiply and an add by
//! itself), and every transcendental function is CORE-MATH's correctly
//! rounded one. This is a port of the reference renderer the chapter's
//! test vectors were made with; `tests/golden.rs` holds it to them. Do not
//! reorder an expression here.

use core_math as cr;

use crate::recipe::{
    Curve, DEFAULT_SEED, Effect, Filter, FilterKind, K, Layer, Recipe, SR, TAU, Wave, limits,
    samples,
};

/// The fade at the end of a sound that was trimmed, in samples (5 ms).
const FADE: usize = 240;

fn shape(curve: Curve, u: f64, e1: f64) -> f64 {
    match curve {
        Curve::Linear => 1.0 - u,
        Curve::Exp => (cr::exp(-K * u) - e1) / (1.0 - e1),
    }
}

fn blep(p: f64, dt: f64) -> f64 {
    if dt <= 0.0 {
        0.0
    } else if p < dt {
        let x = p / dt;
        x + x - x * x - 1.0
    } else if p > 1.0 - dt {
        let x = (p - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// The generator of `gmnb.noise`: a value in -1 to 1.
fn xorshift(s: &mut u32) -> f64 {
    *s ^= *s << 13;
    *s ^= *s >> 17;
    *s ^= *s << 5;
    f64::from(*s) / 2147483648.0 - 1.0
}

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// The RBJ cookbook's coefficients.
    fn set(&mut self, kind: FilterKind, freq: f64, q: f64) {
        let f = freq.clamp(20.0, 20_000.0);
        let q = q.clamp(0.1, 30.0);
        let (s, c) = cr::sincos(TAU * f / SR);
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = match kind {
            FilterKind::Lowpass => ((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0),
            FilterKind::Highpass => ((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0),
            FilterKind::Bandpass => (alpha, 0.0, -alpha),
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = -2.0 * c / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    /// Transposed direct form II.
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

fn render_layer(l: &Layer, left: &mut [f64], right: &mut [f64]) {
    let start = samples(l.at);
    let n = l.samples();
    if n == 0 || start >= left.len() {
        return;
    }
    let whole = n as f64 / SR;
    let e = &l.env;
    let (na, nd) = (samples(e.attack), samples(e.decay));
    let nh = if e.sustain > 0.0 { samples(e.hold) } else { 0 };
    // Where the decay and the hold end. (A sum past `usize` is a layer no
    // buffer reaches the end of.)
    let decay_end = na.saturating_add(nd);
    let hold_end = decay_end.saturating_add(nh);
    let nr = n.saturating_sub(hold_end);
    let e1 = cr::exp(-K);
    let detune = cr::pow(2.0, l.detune / 1200.0);
    let theta = (l.pan.clamp(-1.0, 1.0) + 1.0) * (TAU / 8.0);
    let (gr, gl) = cr::sincos(theta);
    let mut ph = l.phase - l.phase.floor();
    let mut mph = 0.0f64;
    let mut seed = if l.seed == 0 { DEFAULT_SEED } else { l.seed };
    let mut held = if l.wave == Wave::Noise {
        xorshift(&mut seed)
    } else {
        0.0
    };
    let mut bq = Biquad::default();
    if let Some(f) = &l.filter {
        bq.set(f.kind, f.freq.at(0.0, whole), f.q);
    }
    let width = l.width.clamp(0.05, 0.95);
    let out = left[start..].iter_mut().zip(right[start..].iter_mut());
    for (i, (out_l, out_r)) in out.enumerate().take(n) {
        let t = i as f64 / SR;
        // Envelope.
        let env = if i < na {
            i as f64 / na as f64
        } else if i < decay_end {
            let u = (i - na) as f64 / nd as f64;
            e.sustain + (1.0 - e.sustain) * shape(e.curve, u, e1)
        } else if i < hold_end {
            e.sustain
        } else {
            let u = (i - hold_end) as f64 / nr as f64;
            e.sustain * shape(e.curve, u, e1)
        };
        // Frequency.
        let mut f = match &l.freq {
            Some(s) => s.at(t, whole) * detune,
            None => 0.0,
        };
        if let Some(v) = &l.vibrato {
            f *= cr::pow(2.0, v.depth / 1200.0 * cr::sin(TAU * v.rate * t));
        }
        let f = f.clamp(0.0, 20_000.0);
        let dt = f / SR;
        // Phase modulation.
        let pm = match &l.fm {
            Some(fm) => {
                let m = cr::sin(TAU * mph);
                mph += dt * fm.ratio;
                mph -= mph.floor();
                fm.index.at(t, whole) * m
            }
            None => 0.0,
        };
        // Oscillator.
        let mut o = match l.wave {
            Wave::Sine => cr::sin(TAU * ph + pm),
            Wave::Noise => {
                if l.freq.is_none() {
                    held = xorshift(&mut seed);
                }
                held
            }
            w => {
                let mut p = ph + pm / TAU;
                p -= p.floor();
                match w {
                    Wave::Triangle => {
                        let mut q = p + 0.25;
                        q -= q.floor();
                        1.0 - 4.0 * (q - 0.5).abs()
                    }
                    Wave::Saw => 2.0 * p - 1.0 - blep(p, dt),
                    _ => {
                        let mut p2 = p + 1.0 - width;
                        p2 -= p2.floor();
                        (if p < width { 1.0 } else { -1.0 }) + blep(p, dt) - blep(p2, dt)
                    }
                }
            }
        };
        ph += dt;
        if ph >= 1.0 {
            ph -= ph.floor();
            if l.wave == Wave::Noise && l.freq.is_some() {
                held = xorshift(&mut seed);
            }
        }
        // Filter, then loudness.
        if let Some(fl) = &l.filter {
            if !fl.freq.is_fixed() {
                bq.set(fl.kind, fl.freq.at(t, whole), fl.q);
            }
            o = bq.run(o);
        }
        let mut v = o * env * l.gain;
        if let Some(tr) = &l.tremolo {
            v *= 1.0 - tr.depth * 0.5 * (1.0 - cr::cos(TAU * tr.rate * t));
        }
        *out_l += v * gl;
        *out_r += v * gr;
    }
}

const COMBS: [usize; 8] = [1215, 1293, 1390, 1476, 1548, 1623, 1695, 1760];
const ALLPASSES: [usize; 4] = [605, 480, 371, 245];
/// How much longer the right channel's rings are.
const SPREAD: usize = 25;

/// A ring of samples and where in it we are.
struct Ring {
    buf: Vec<f64>,
    at: usize,
}

impl Ring {
    fn new(len: usize) -> Ring {
        Ring {
            buf: vec![0.0; len],
            at: 0,
        }
    }

    fn step(&mut self) {
        self.at += 1;
        if self.at == self.buf.len() {
            self.at = 0;
        }
    }
}

/// Freeverb's structure, one channel: eight combs in parallel, then four
/// all-passes in series.
struct Reverb {
    combs: Vec<(Ring, f64)>,
    allpasses: Vec<Ring>,
}

impl Reverb {
    fn new(extra: usize) -> Reverb {
        Reverb {
            combs: COMBS.iter().map(|&n| (Ring::new(n + extra), 0.0)).collect(),
            allpasses: ALLPASSES.iter().map(|&n| Ring::new(n + extra)).collect(),
        }
    }

    fn run(&mut self, input: f64, feedback: f64, damp: f64) -> f64 {
        let mut out = 0.0;
        for (ring, store) in &mut self.combs {
            let y = ring.buf[ring.at];
            *store = y * (1.0 - damp) + *store * damp;
            ring.buf[ring.at] = input + *store * feedback;
            ring.step();
            out += y;
        }
        for ring in &mut self.allpasses {
            let b = ring.buf[ring.at];
            let y = b - out;
            ring.buf[ring.at] = out + b * 0.5;
            ring.step();
            out = y;
        }
        out
    }
}

fn filter_effect(f: &Filter, left: &mut [f64], right: &mut [f64]) {
    let whole = left.len() as f64 / SR;
    let mut a = Biquad::default();
    a.set(f.kind, f.freq.at(0.0, whole), f.q);
    let mut b = a;
    for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
        if !f.freq.is_fixed() {
            a.set(f.kind, f.freq.at(i as f64 / SR, whole), f.q);
            b = Biquad {
                z1: b.z1,
                z2: b.z2,
                ..a
            };
        }
        *l = a.run(*l);
        *r = b.run(*r);
    }
}

fn apply(effect: &Effect, left: &mut [f64], right: &mut [f64]) {
    if let Effect::Filter(f) = effect {
        return filter_effect(f, left, right);
    }
    let both = left.iter_mut().zip(right.iter_mut());
    match *effect {
        Effect::Filter(_) => {}
        Effect::Delay {
            time,
            feedback,
            mix,
            ping_pong,
        } => {
            let d = samples(time.clamp(0.001, 1.0)).max(1);
            let fb = feedback.clamp(0.0, 0.9);
            let (mut bl, mut br) = (vec![0.0f64; d], vec![0.0f64; d]);
            let mut k = 0;
            for (l, r) in both {
                let (dl, dr) = (bl[k], br[k]);
                let (xl, xr) = (*l, *r);
                if ping_pong {
                    bl[k] = 0.5 * (xl + xr) + fb * dr;
                    br[k] = fb * dl;
                } else {
                    bl[k] = xl + fb * dl;
                    br[k] = xr + fb * dr;
                }
                *l = xl + mix * dl;
                *r = xr + mix * dr;
                k += 1;
                if k == d {
                    k = 0;
                }
            }
        }
        Effect::Reverb { size, damp, mix } => {
            let fb = 0.7 + 0.28 * size.clamp(0.0, 1.0);
            let dm = 0.4 * damp.clamp(0.0, 1.0);
            let (mut rl, mut rr) = (Reverb::new(0), Reverb::new(SPREAD));
            for (l, r) in both {
                let input = (*l + *r) * 0.015;
                *l += mix * rl.run(input, fb, dm);
                *r += mix * rr.run(input, fb, dm);
            }
        }
        Effect::Drive { amount } => {
            let g = 1.0 + 9.0 * amount.clamp(0.0, 1.0);
            let norm = cr::tanh(g);
            for (l, r) in both {
                *l = cr::tanh(g * *l) / norm;
                *r = cr::tanh(g * *r) / norm;
            }
        }
        Effect::Crush { bits, rate } => {
            let q = cr::pow(2.0, bits.clamp(1.0, 16.0) - 1.0);
            let step = rate.clamp(100.0, SR) / SR;
            let (mut acc, mut hl, mut hr) = (1.0f64, 0.0f64, 0.0f64);
            for (l, r) in both {
                if acc >= 1.0 {
                    acc -= 1.0;
                    hl = (*l * q).round() / q;
                    hr = (*r * q).round() / q;
                }
                acc += step;
                *l = hl;
                *r = hr;
            }
        }
    }
}

fn peak_of(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .chain(right.iter())
        .fold(0.0f64, |m, v| m.max(v.abs()))
}

/// Renders the recipe: interleaved stereo, 48 kHz, at most 4 s. A recipe
/// with no layers, or with more layers or effects than a recipe may have,
/// is silence (no samples).
///
/// Never panics and never allocates more than a 4 s sound needs, whatever
/// numbers the recipe holds; ones that are not finite give samples of 0
/// or nonsense, not an error. (Checking what a theme wrote, with notes for
/// its author, is the theme engine's work.)
pub fn render(r: &Recipe) -> Vec<i16> {
    if !r.within_limits() {
        return Vec::new();
    }
    let dry = r
        .layers
        .iter()
        .map(|l| samples(l.at).saturating_add(l.samples()))
        .max()
        .unwrap_or(0)
        .min(limits::FRAMES);
    let rings = r
        .effects
        .iter()
        .any(|e| matches!(e, Effect::Delay { .. } | Effect::Reverb { .. }));
    let n = match r.length {
        Some(s) => samples(s).min(limits::FRAMES),
        None if rings => limits::FRAMES,
        None => dry,
    };
    let (mut left, mut right) = (vec![0.0f64; n], vec![0.0f64; n]);
    for l in &r.layers {
        render_layer(l, &mut left, &mut right);
    }
    for e in &r.effects {
        apply(e, &mut left, &mut right);
    }
    let mut len = n;
    if rings || r.length.is_some() {
        // Cut the silence at the end, then fade the cut.
        let floor = peak_of(&left, &right) / 1024.0;
        while len > 0 && left[len - 1].abs() < floor && right[len - 1].abs() < floor {
            len -= 1;
        }
        let fade = FADE.min(len);
        for k in 0..fade {
            let g = k as f64 / fade as f64;
            left[len - 1 - k] *= g;
            right[len - 1 - k] *= g;
        }
    }
    let (left, right) = (&left[..len], &right[..len]);
    let peak = peak_of(left, right);
    let scale = if peak > 0.0 {
        r.gain.clamp(0.0, 1.0) / peak
    } else {
        0.0
    };
    let mut out = Vec::with_capacity(len * 2);
    for (l, r) in left.iter().zip(right) {
        out.push((l * scale * 32767.0).round() as i16);
        out.push((r * scale * 32767.0).round() as i16);
    }
    out
}
