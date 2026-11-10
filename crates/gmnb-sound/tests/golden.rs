//! The synthesiser's test vectors (docs/theming-media.md, "The
//! synthesiser, exactly"): the same recipe gives the same samples, here
//! and in every build. If one of these hashes changes, themes sound
//! different: the arithmetic in `synth.rs` was reordered or a function
//! swapped, and that is the bug, not the hash.
//!
//! The hashes are the ones the design's reference renderer printed in a
//! debug build, a release build, x86-64-v3 with and without FMA, and with
//! CORE-MATH's baseline code. `cargo test` checks them at the dev
//! profile's optimisation and `cargo test --release` at the release one.

mod common;

use common::{hash, sha256};
use gmnb_sound::{Recipe, render};

#[test]
fn sha256_is_sha256() {
    assert_eq!(
        sha256(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    // More than one block's worth of padding to get right.
    assert_eq!(
        sha256(&[b'a'; 1000]),
        "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
    );
}

/// Name, frames, SHA-256.
const EXPECTED: [(&str, usize, &str); 10] = [
    (
        "click-soft",
        2184,
        "21c869ae7b1c53cb7049330a055fc4e7be80f991af3e04719b7ceaa69b5ce816",
    ),
    (
        "click-low",
        2664,
        "028a27b50a1d472fbf2256664b35312c3dd892ea5c6feab1647ced516fe21d6d",
    ),
    (
        "chime-glass",
        65_624,
        "767b821cea4900be32d7dac2bc441c0f6d000eabe1753ed477e1064f6a118dd1",
    ),
    (
        "error-buzz",
        13_632,
        "02c74414c9ac4e7a4180a032de47c27634d14f41983af170d484e2d9bbcd1579",
    ),
    (
        "equals-thump",
        6768,
        "5cfc54bfc7502b15cad28a86a657821eb084c3942f95112c601e253745ce65b8",
    ),
    (
        "mode-whoosh",
        13_920,
        "d3574b812dabd00fb4a1d7499c3b210f444806dd018c347e612c77b5d400aa72",
    ),
    (
        "toggle-on",
        6096,
        "89d243d184fa4500c362abe06bf109f24407aaa1f61e254f17d9003126494821",
    ),
    (
        "toggle-off",
        6096,
        "945360c5e814fbb417c06289e9872e5b46265e775ae8737c45e053f9c601c8c8",
    ),
    (
        "arcade-result",
        43_320,
        "2da49fbc9e754d97ed782346f8f94169348ebe587defeee4d926ef7dce804038",
    ),
    (
        "clear-fall",
        8736,
        "f3c7d902b905bd29936bf9e96f4299678d9db7bf2449372a2f12849606317e3e",
    ),
];

#[test]
fn the_ten_example_sounds() {
    let sounds = common::sounds();
    assert_eq!(sounds.len(), EXPECTED.len());
    let mut wrong = Vec::new();
    for ((name, recipe), (expected_name, frames, expected)) in sounds.iter().zip(EXPECTED) {
        assert_eq!(*name, expected_name);
        let samples = render(recipe);
        let got = hash(&samples);
        if samples.len() != frames * 2 || got != expected {
            wrong.push(format!("{name}: {} frames, {got}", samples.len() / 2));
        }
    }
    assert!(wrong.is_empty(), "samples changed:\n{}", wrong.join("\n"));
}

/// The chapter's three vectors by name (they are among the ten).
#[test]
fn the_chapters_vectors() {
    for (recipe, frames, expected) in [
        (
            common::click_soft(),
            2184,
            "21c869ae7b1c53cb7049330a055fc4e7be80f991af3e04719b7ceaa69b5ce816",
        ),
        (
            common::chime_glass(),
            65_624,
            "767b821cea4900be32d7dac2bc441c0f6d000eabe1753ed477e1064f6a118dd1",
        ),
        (
            common::error_buzz(),
            13_632,
            "02c74414c9ac4e7a4180a032de47c27634d14f41983af170d484e2d9bbcd1579",
        ),
    ] {
        let samples = render(&recipe);
        assert_eq!(samples.len(), frames * 2);
        assert_eq!(hash(&samples), expected);
    }
}

/// Eight 4 s layers with everything on and four effects: every operation
/// of the specification in one sound. (The reference printed the first 16
/// hex digits.)
#[test]
fn the_most_a_recipe_may_ask_for() {
    let samples = render(&common::worst());
    assert_eq!(samples.len(), 192_000 * 2);
    assert!(hash(&samples).starts_with("0e715a4afa668de5"));
}

#[test]
fn rendering_twice_gives_the_same_samples() {
    for (name, recipe) in common::sounds() {
        assert_eq!(render(&recipe), render(&recipe), "{name}");
    }
}

/// The loudest sample is `gain`: a recipe cannot clip.
#[test]
fn the_peak_is_the_gain() {
    for (name, recipe) in common::sounds() {
        let peak = render(&recipe)
            .iter()
            .map(|s| s.unsigned_abs())
            .max()
            .unwrap();
        let want = (recipe.gain * 32767.0).round() as u16;
        assert_eq!(peak, want, "{name}");
    }
    let silent = Recipe {
        gain: 0.0,
        ..common::click_soft()
    };
    assert!(render(&silent).iter().all(|s| *s == 0));
}
