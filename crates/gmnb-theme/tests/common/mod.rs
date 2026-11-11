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
