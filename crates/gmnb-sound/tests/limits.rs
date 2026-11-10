//! The synthesiser at and past its limits: whatever a recipe holds, the
//! renderer gives at most 4 s and never panics. (What a theme wrote is
//! checked before it gets here, by gmnb-theme; this is the renderer's own
//! footing.)

mod common;

use gmnb_sound::{
    Curve, Effect, Env, Filter, FilterKind, Fm, Glide, Layer, Lfo, Recipe, Wave, limits, render,
};

fn pluck(seconds: f64) -> Layer {
    Layer {
        env: Env {
            attack: 0.0,
            decay: seconds,
            ..Env::default()
        },
        ..Layer::default()
    }
}

#[test]
fn layers_and_effects_at_and_past_their_caps() {
    let with = |layers: usize, effects: usize| Recipe {
        layers: vec![pluck(0.01); layers],
        effects: vec![Effect::Drive { amount: 0.2 }; effects],
        ..Recipe::default()
    };
    assert_eq!(render(&with(1, 0)).len(), 480 * 2);
    assert_eq!(
        render(&with(limits::LAYERS, limits::EFFECTS)).len(),
        480 * 2
    );
    // One too many of either, or none at all: silence.
    assert!(render(&with(limits::LAYERS + 1, 0)).is_empty());
    assert!(render(&with(1, limits::EFFECTS + 1)).is_empty());
    assert!(render(&with(0, 0)).is_empty());
    assert!(!with(0, 1).within_limits() && with(8, 4).within_limits());
}

#[test]
fn a_sound_is_never_longer_than_four_seconds() {
    let frames = |r: &Recipe| render(r).len() / 2;
    // Layers that would run on are cut at 4 s.
    let long = Recipe {
        layers: vec![pluck(30.0)],
        ..Recipe::default()
    };
    assert_eq!(frames(&long), limits::FRAMES);
    // So is a `length` past it, and a layer that starts beyond it adds
    // nothing.
    let asked = Recipe {
        length: Some(60.0),
        layers: vec![
            pluck(30.0),
            Layer {
                at: 5.0,
                ..pluck(1.0)
            },
        ],
        ..Recipe::default()
    };
    assert_eq!(frames(&asked), limits::FRAMES);
    // A delay that never dies away is stopped at 4 s too.
    let ringing = Recipe {
        layers: vec![pluck(0.05)],
        effects: vec![Effect::Delay {
            time: 0.1,
            feedback: 0.9,
            mix: 1.0,
            ping_pong: false,
        }],
        ..Recipe::default()
    };
    assert!(frames(&ringing) <= limits::FRAMES);
    assert!(frames(&ringing) > 48_000);
    // `length` shorter than the layers cuts them, with a fade.
    let cut = Recipe {
        length: Some(0.5),
        layers: vec![Layer {
            env: Env {
                attack: 0.0,
                decay: 0.0,
                sustain: 1.0,
                hold: 2.0,
                release: 0.0,
                curve: Curve::Linear,
            },
            ..Layer::default()
        }],
        ..Recipe::default()
    };
    let samples = render(&cut);
    assert_eq!(samples.len() / 2, 24_000);
    assert_eq!(&samples[samples.len() - 2..], [0, 0]);
    // A length of nothing is nothing.
    let nothing = Recipe {
        length: Some(0.0),
        ..cut
    };
    assert!(render(&nothing).is_empty());
}

/// Recipes built straight from numbers a theme can never get past the
/// checks: not-a-number, infinities, the largest and smallest doubles,
/// negatives, in every field. Rendering each gives at most 4 s, and never
/// panics.
#[test]
fn any_numbers_at_all_do_no_harm() {
    let mut seed = 0x1234_5678u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    const ODD: [f64; 16] = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        -0.0,
        0.0,
        -1.0,
        1e-300,
        1e300,
        4.0,
        4.000001,
        192_000.0,
        -1e-9,
        0.5,
    ];
    // Mostly small, sensible values (so that the sounds are short), with
    // an odd one now and then.
    let mut number = |usual: f64| -> f64 {
        match next() % 5 {
            0 => ODD[next() as usize % ODD.len()],
            1 => usual * 0.5,
            _ => usual,
        }
    };
    let mut rendered = 0;
    for round in 0..600 {
        let curve = |n: u32| {
            if n.is_multiple_of(2) {
                Curve::Exp
            } else {
                Curve::Linear
            }
        };
        let glide = |usual: f64, number: &mut dyn FnMut(f64) -> f64| Glide {
            from: number(usual),
            to: number(usual * 0.7),
            time: (round % 3 != 0).then(|| number(0.01)),
            curve: curve(round),
        };
        let kinds = [
            FilterKind::Lowpass,
            FilterKind::Highpass,
            FilterKind::Bandpass,
        ];
        let waves = [
            Wave::Sine,
            Wave::Triangle,
            Wave::Square,
            Wave::Saw,
            Wave::Noise,
        ];
        let mut layers = Vec::new();
        for i in 0..1 + round % 3 {
            let wave = waves[(round as usize + i as usize) % 5];
            layers.push(Layer {
                wave,
                freq: (round % 7 != 0).then(|| glide(440.0, &mut number)),
                detune: number(5.0),
                width: number(0.5),
                phase: number(0.25),
                seed: round,
                fm: (round % 2 == 0).then(|| Fm {
                    ratio: number(2.0),
                    index: glide(1.5, &mut number),
                }),
                vibrato: (round % 3 == 0).then(|| Lfo {
                    rate: number(6.0),
                    depth: number(20.0),
                }),
                tremolo: (round % 4 == 0).then(|| Lfo {
                    rate: number(5.0),
                    depth: number(0.5),
                }),
                filter: (round % 2 == 1).then(|| Filter {
                    kind: kinds[round as usize % 3],
                    freq: glide(2000.0, &mut number),
                    q: number(0.8),
                }),
                env: Env {
                    attack: number(0.001),
                    decay: number(0.01),
                    sustain: number(if round % 2 == 0 { 0.0 } else { 0.6 }),
                    hold: number(0.005),
                    release: number(0.005),
                    curve: curve(round / 2),
                },
                gain: number(1.0),
                pan: number(0.0),
                at: number(0.002),
            });
        }
        let effects = match round % 6 {
            0 => vec![Effect::Delay {
                time: number(0.01),
                feedback: number(0.3),
                mix: number(0.3),
                ping_pong: round % 4 == 0,
            }],
            1 => vec![Effect::Reverb {
                size: number(0.5),
                damp: number(0.5),
                mix: number(0.3),
            }],
            2 => vec![
                Effect::Drive {
                    amount: number(0.3),
                },
                Effect::Crush {
                    bits: number(6.0),
                    rate: number(8000.0),
                },
            ],
            3 => vec![Effect::Filter(Filter {
                kind: kinds[round as usize % 3],
                freq: glide(1000.0, &mut number),
                q: number(1.0),
            })],
            _ => Vec::new(),
        };
        let recipe = Recipe {
            layers,
            effects,
            gain: number(0.5),
            // A length keeps the delays and reverbs short.
            length: (round % 6 < 2 || round % 5 == 0).then(|| number(0.05)),
        };
        let samples = render(&recipe);
        assert!(samples.len() <= limits::FRAMES * 2, "{recipe:?}");
        assert!(samples.len().is_multiple_of(2));
        // The same nonsense twice is the same samples.
        if round % 10 == 0 {
            assert_eq!(render(&recipe), samples, "{recipe:?}");
        }
        rendered += samples.len();
    }
    assert!(rendered > 100_000, "the recipes were all silent");
    // And the reference recipes with one number at a time made odd.
    for odd in ODD {
        let mut r = common::click_soft();
        r.layers[0].env.decay = odd;
        r.layers[1].at = odd;
        r.layers[2].gain = odd;
        r.gain = odd;
        assert!(render(&r).len() <= limits::FRAMES * 2);
        let mut r = common::chime_glass();
        r.length = Some(odd);
        r.layers[0].fm.as_mut().unwrap().index.to = odd;
        r.layers[1].detune = odd;
        assert!(render(&r).len() <= limits::FRAMES * 2);
        let mut r = common::error_buzz();
        r.layers[0].width = odd;
        r.layers[1].filter.as_mut().unwrap().q = odd;
        r.layers[2].freq.as_mut().unwrap().time = Some(odd);
        r.effects = vec![Effect::Drive { amount: odd }];
        assert!(render(&r).len() <= limits::FRAMES * 2);
    }
}
