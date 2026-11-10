//! What the tests share: the designer's ten example sounds as recipes, and
//! SHA-256 (the chapter's test vectors are SHA-256 over the samples).
#![allow(dead_code)]

use gmnb_sound::{
    Curve, Effect, Env, Filter, FilterKind, Fm, Glide, Layer, Lfo, Recipe, Wave, note_name,
};

// ---- SHA-256 ---------------------------------------------------------------

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 of `data`, in hex.
pub fn sha256(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&(data.len() as u64 * 8).to_be_bytes());
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for (k, w) in K.iter().zip(w) {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(w);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    h.iter().map(|w| format!("{w:08x}")).collect()
}

/// The chapter's hash of a sound: SHA-256 over the samples as
/// little-endian 16-bit, left before right.
pub fn hash(samples: &[i16]) -> String {
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    sha256(&bytes)
}

// ---- The ten example sounds ----------------------------------------------------
//
// From the design's prototype (theming-work/media/synth/src/main.rs), where
// they were written as these values; the hashes in `golden.rs` are the ones
// it printed in five builds.

fn n(name: &str) -> f64 {
    note_name(name).unwrap()
}

fn perc(attack: f64, decay: f64) -> Env {
    Env {
        attack,
        decay,
        ..Env::default()
    }
}

fn tone(attack: f64, decay: f64, sustain: f64, hold: f64, release: f64) -> Env {
    Env {
        attack,
        decay,
        sustain,
        hold,
        release,
        curve: Curve::Exp,
    }
}

fn bandpass(freq: f64, q: f64) -> Option<Filter> {
    Some(Filter {
        kind: FilterKind::Bandpass,
        freq: Glide::fixed(freq),
        q,
    })
}

fn lowpass(freq: f64, q: f64) -> Option<Filter> {
    Some(Filter {
        kind: FilterKind::Lowpass,
        freq: Glide::fixed(freq),
        q,
    })
}

fn noise(seed: u32, filter: Option<Filter>, env: Env, gain: f64) -> Layer {
    Layer {
        wave: Wave::Noise,
        freq: None,
        seed,
        filter,
        env,
        gain,
        ..Layer::default()
    }
}

pub fn click_soft() -> Recipe {
    Recipe {
        gain: 0.5,
        layers: vec![
            Layer {
                freq: Some(Glide::exp(950.0, 420.0, 0.03)),
                env: perc(0.0005, 0.045),
                ..Layer::default()
            },
            Layer {
                freq: Some(Glide::exp(2400.0, 1800.0, 0.012)),
                env: perc(0.0, 0.015),
                gain: 0.15,
                ..Layer::default()
            },
            noise(1, bandpass(3800.0, 1.2), perc(0.0, 0.012), 0.35),
        ],
        ..Recipe::default()
    }
}

pub fn chime_glass() -> Recipe {
    let bell = |name: &str, at: f64, pan: f64, gain: f64| Layer {
        freq: Some(Glide::fixed(n(name))),
        fm: Some(Fm {
            ratio: 3.5,
            index: Glide::exp(2.6, 0.05, 0.35),
        }),
        env: perc(0.002, 0.9),
        at,
        pan,
        gain,
        ..Layer::default()
    };
    Recipe {
        gain: 0.4,
        layers: vec![
            bell("E6", 0.0, -0.3, 1.0),
            bell("G#6", 0.075, 0.3, 0.85),
            // A faint octave above, a few cents sharp, so the tail shimmers.
            Layer {
                freq: Some(Glide::fixed(n("E7"))),
                detune: 7.0,
                env: perc(0.004, 0.6),
                gain: 0.2,
                ..Layer::default()
            },
        ],
        effects: vec![Effect::Reverb {
            size: 0.55,
            damp: 0.3,
            mix: 0.3,
        }],
        length: Some(1.6),
    }
}

pub fn error_buzz() -> Recipe {
    let growl = |freq: f64, at: f64| {
        vec![
            Layer {
                wave: Wave::Square,
                width: 0.35,
                freq: Some(Glide::lin(freq, freq * 0.94, 0.12)),
                filter: lowpass(900.0, 0.8),
                env: tone(0.004, 0.02, 0.8, 0.07, 0.04),
                at,
                ..Layer::default()
            },
            Layer {
                wave: Wave::Saw,
                freq: Some(Glide::lin(freq, freq * 0.94, 0.12)),
                detune: 14.0,
                filter: lowpass(700.0, 0.7),
                env: tone(0.004, 0.02, 0.8, 0.07, 0.04),
                gain: 0.5,
                at,
                ..Layer::default()
            },
        ]
    };
    Recipe {
        gain: 0.42,
        layers: [growl(116.5, 0.0), growl(98.0, 0.15)].concat(),
        effects: vec![Effect::Drive { amount: 0.25 }],
        ..Recipe::default()
    }
}

/// The ten, by the names of the design's WAV files.
pub fn sounds() -> Vec<(&'static str, Recipe)> {
    let mut v = vec![("click-soft", click_soft())];

    // The same click, a touch lower and duller: for function keys.
    v.push((
        "click-low",
        Recipe {
            gain: 0.5,
            layers: vec![
                Layer {
                    freq: Some(Glide::exp(700.0, 300.0, 0.035)),
                    env: perc(0.0005, 0.055),
                    ..Layer::default()
                },
                noise(2, bandpass(2600.0, 1.0), perc(0.0, 0.014), 0.3),
            ],
            ..Recipe::default()
        },
    ));

    v.push(("chime-glass", chime_glass()));
    v.push(("error-buzz", error_buzz()));

    // Equals: a soft thump with a bright tick on top.
    v.push((
        "equals-thump",
        Recipe {
            gain: 0.55,
            layers: vec![
                Layer {
                    freq: Some(Glide::exp(240.0, 95.0, 0.07)),
                    env: perc(0.001, 0.14),
                    ..Layer::default()
                },
                Layer {
                    wave: Wave::Triangle,
                    freq: Some(Glide::fixed(n("E5"))),
                    env: perc(0.001, 0.09),
                    gain: 0.25,
                    ..Layer::default()
                },
                noise(3, bandpass(5200.0, 2.0), perc(0.0, 0.01), 0.25),
            ],
            ..Recipe::default()
        },
    ));

    // A mode change: air moving past.
    v.push((
        "mode-whoosh",
        Recipe {
            gain: 0.35,
            layers: vec![noise(
                4,
                Some(Filter {
                    kind: FilterKind::Bandpass,
                    freq: Glide::exp(500.0, 3200.0, 0.22),
                    q: 1.4,
                }),
                Env {
                    attack: 0.09,
                    decay: 0.2,
                    ..Env::default()
                },
                1.0,
            )],
            ..Recipe::default()
        },
    ));

    // Toggles: two notes up for on, the same two down for off.
    let pip = |name: &str, at: f64| Layer {
        wave: Wave::Triangle,
        freq: Some(Glide::fixed(n(name))),
        env: perc(0.002, 0.07),
        at,
        ..Layer::default()
    };
    v.push((
        "toggle-on",
        Recipe {
            gain: 0.4,
            layers: vec![pip("A5", 0.0), pip("D6", 0.055)],
            ..Recipe::default()
        },
    ));
    v.push((
        "toggle-off",
        Recipe {
            gain: 0.4,
            layers: vec![pip("D6", 0.0), pip("A5", 0.055)],
            ..Recipe::default()
        },
    ));

    // An arcade result: a rising square arpeggio, crushed, with an echo.
    let step = |name: &str, i: usize| Layer {
        wave: Wave::Square,
        width: 0.25,
        freq: Some(Glide::fixed(n(name))),
        env: tone(0.001, 0.01, 0.6, 0.035, 0.03),
        at: i as f64 * 0.06,
        ..Layer::default()
    };
    v.push((
        "arcade-result",
        Recipe {
            gain: 0.3,
            layers: vec![step("C5", 0), step("E5", 1), step("G5", 2), step("C6", 3)],
            effects: vec![
                Effect::Crush {
                    bits: 5.0,
                    rate: 11_025.0,
                },
                Effect::Delay {
                    time: 0.12,
                    feedback: 0.35,
                    mix: 0.3,
                    ping_pong: true,
                },
            ],
            length: Some(1.2),
        },
    ));

    // Clear: a quick fall with a wobble.
    v.push((
        "clear-fall",
        Recipe {
            gain: 0.4,
            layers: vec![Layer {
                wave: Wave::Triangle,
                freq: Some(Glide::exp(1400.0, 260.0, 0.16)),
                vibrato: Some(Lfo {
                    rate: 28.0,
                    depth: 60.0,
                }),
                env: perc(0.002, 0.18),
                ..Layer::default()
            }],
            ..Recipe::default()
        },
    ));
    v
}

/// The most a recipe may ask for: eight 4 s layers with everything on,
/// four effects.
pub fn worst() -> Recipe {
    let layer = |i: usize| Layer {
        wave: [Wave::Sine, Wave::Square, Wave::Saw, Wave::Triangle][i % 4],
        freq: Some(Glide::exp(200.0 + 100.0 * i as f64, 2000.0, 4.0)),
        fm: Some(Fm {
            ratio: 2.5,
            index: Glide::exp(3.0, 0.1, 4.0),
        }),
        vibrato: Some(Lfo {
            rate: 5.0,
            depth: 30.0,
        }),
        tremolo: Some(Lfo {
            rate: 7.0,
            depth: 0.5,
        }),
        filter: Some(Filter {
            kind: FilterKind::Lowpass,
            freq: Glide::exp(8000.0, 300.0, 4.0),
            q: 2.0,
        }),
        env: Env {
            attack: 0.5,
            decay: 3.5,
            ..Env::default()
        },
        pan: i as f64 / 4.0 - 1.0,
        ..Layer::default()
    };
    Recipe {
        gain: 0.5,
        layers: (0..8).map(layer).collect(),
        effects: vec![
            Effect::Drive { amount: 0.5 },
            Effect::Filter(Filter {
                kind: FilterKind::Highpass,
                freq: Glide::exp(40.0, 400.0, 4.0),
                q: 0.7,
            }),
            Effect::Delay {
                time: 0.25,
                feedback: 0.5,
                mix: 0.4,
                ping_pong: true,
            },
            Effect::Reverb {
                size: 0.9,
                damp: 0.4,
                mix: 0.4,
            },
        ],
        length: None,
    }
}
