//! What the theme tests share.

use gmnb_theme::Limits;

/// The limits a theme really has when `GMNB_REAL_LIMITS` is set (run so
/// with `--release` on a quiet machine, to hold the real budgets).
/// Otherwise four times each hook's time: these tests check what the themes
/// do, wherever the code is built (CI's runners, the packages' builders,
/// unoptimised builds), and a slow machine there is not a slow theme. Four
/// times still catches a hook gone badly wrong. The engine's own deadline
/// tests (`engine.rs`) and the budgets of `swirls.rs` and `media.rs` keep
/// to the real limits.
pub fn limits() -> Limits {
    let real = Limits::default();
    if std::env::var_os("GMNB_REAL_LIMITS").is_some() {
        return real;
    }
    Limits {
        load: real.load * 4,
        style: real.style * 4,
        frame: real.frame * 4,
        preview: real.preview * 4,
        visible: real.visible * 4,
        action: real.action * 4,
        place: real.place * 4,
        ..real
    }
}

/// The ramp Swirls' paint is coloured through on a window of `opacity`, as
/// the theme makes it (its `thinned`; the picture its paint makes doesn't
/// carry its ramp, as a field's picture did): `bands` (the warm, the cool,
/// the dark one) themselves from 1 on; below, each at its own opacity for
/// the opacity rounded up to a 64th, the bright ones
/// `0.4 + 0.6 smooth((o − 0.1) / 0.9)`, the dark one
/// `smooth((o − 0.5) / 0.5)`. `tests/swirls.rs` holds every pixel of the
/// paint drawn to it at each twentieth of opacity.
#[allow(dead_code)] // (css.rs and swirls.rs)
pub fn swirls_ramp(bands: [[f32; 4]; 3], opacity: f32) -> gmnb_media::Ramp {
    let o = (f64::from(opacity) * 64.0).ceil() / 64.0;
    let smooth = |t: f64| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let (bright, dark) = if o >= 1.0 {
        (1.0, 1.0)
    } else {
        (0.4 + 0.6 * smooth((o - 0.1) / 0.9), smooth((o - 0.5) / 0.5))
    };
    // (gmnb.alpha: the colour's own opacity times the one given.)
    let at = |c: [f32; 4], x: f64| [c[0], c[1], c[2], (c[3] * x as f32).clamp(0.0, 1.0)];
    let [a, b, ground] = bands;
    gmnb_media::Ramp::new(
        &[at(a, bright), at(b, bright), at(ground, dark)],
        [0.08, 0.92],
        true,
    )
    .unwrap()
}
