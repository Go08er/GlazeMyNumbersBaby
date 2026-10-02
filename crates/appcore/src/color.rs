//! Colour maths shared by both twins: sRGB ↔ HSL, hex, WCAG luminance and
//! contrast, and helpers that nudge a colour until text on it is readable.
//! Colours are `[r, g, b]` in 0..1.

pub type Rgb = [f32; 3];

pub const WHITE: Rgb = [1.0, 1.0, 1.0];
pub const BLACK: Rgb = [0.0, 0.0, 0.0];

/// WCAG 2 minimum for body text.
pub const TEXT_CONTRAST: f32 = 4.5;
/// WCAG 2 minimum for large text and meaningful non-text UI (lines, icons).
pub const UI_CONTRAST: f32 = 3.0;

pub const fn hex(c: u32) -> Rgb {
    [
        ((c >> 16) & 0xff) as f32 / 255.0,
        ((c >> 8) & 0xff) as f32 / 255.0,
        (c & 0xff) as f32 / 255.0,
    ]
}

pub fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(hex(v))
}

pub fn to_hex(c: Rgb) -> String {
    let [r, g, b] = to_u8(c);
    format!("#{r:02x}{g:02x}{b:02x}")
}

pub fn to_u8(c: Rgb) -> [u8; 3] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// RGB → (hue degrees, saturation, lightness).
pub fn to_hsl(c: Rgb) -> (f32, f32, f32) {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let h = if max == c[0] {
        60.0 * (((c[1] - c[2]) / d).rem_euclid(6.0))
    } else if max == c[1] {
        60.0 * ((c[2] - c[0]) / d + 2.0)
    } else {
        60.0 * ((c[0] - c[1]) / d + 4.0)
    };
    (h, s.clamp(0.0, 1.0), l)
}

pub fn from_hsl(h: f32, s: f32, l: f32) -> Rgb {
    let (s, l) = (s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    let h = h.rem_euclid(360.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m]
}

/// The colour opposite on the colour wheel.
pub fn complement(c: Rgb) -> Rgb {
    let (h, s, l) = to_hsl(c);
    from_hsl(h + 180.0, s, l)
}

pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// WCAG relative luminance.
pub fn luminance(c: Rgb) -> f32 {
    let lin = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// WCAG contrast ratio (1…21).
pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Adjust `c`'s lightness (keeping its hue) as little as possible so it
/// reaches `target` contrast against `against`. Moves away from `against`:
/// darker on light backgrounds, lighter on dark ones.
pub fn with_contrast(c: Rgb, against: Rgb, target: f32) -> Rgb {
    if contrast(c, against) >= target {
        return c;
    }
    let (h, s, l) = to_hsl(c);
    // Which direction can actually get there? Prefer the natural one.
    let darker_ok = contrast(BLACK, against) >= target;
    let lighter_ok = contrast(WHITE, against) >= target;
    let go_dark = if luminance(against) > 0.18 {
        darker_ok || !lighter_ok
    } else {
        !lighter_ok && darker_ok
    };
    let (mut lo, mut hi) = if go_dark { (0.0, l) } else { (l, 1.0) };
    // Binary search for the lightness closest to `l` that passes.
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        let ok = contrast(from_hsl(h, s, mid), against) >= target;
        if go_dark {
            if ok { lo = mid } else { hi = mid }
        } else if ok {
            hi = mid
        } else {
            lo = mid
        }
    }
    from_hsl(h, s, if go_dark { lo } else { hi })
}

/// Lowest contrast of `fg` against any of `bgs`.
pub fn worst_contrast(fg: Rgb, bgs: &[Rgb]) -> f32 {
    bgs.iter()
        .map(|&b| contrast(fg, b))
        .fold(f32::INFINITY, f32::min)
}

/// Text on a two-stop gradient: picks `light` or `dark` text, whichever reads
/// better, then adjusts the stops (see [`gradient_for_text`]). Returns
/// `(text, stop_a, stop_b)`.
pub fn readable_gradient(a: Rgb, b: Rgb, light: Rgb, dark: Rgb, target: f32) -> (Rgb, Rgb, Rgb) {
    let stops = [a, b, mix(a, b, 0.5)];
    let text = if worst_contrast(light, &stops) >= worst_contrast(dark, &stops) {
        light
    } else {
        dark
    };
    let (a, b) = gradient_for_text(text, a, b, target);
    (text, a, b)
}

/// Darken/lighten a gradient's stops just enough for `text` to reach
/// `target` at both ends and the midpoint.
pub fn gradient_for_text(text: Rgb, a: Rgb, b: Rgb, target: f32) -> (Rgb, Rgb) {
    let stops = |a, b| [a, b, mix(a, b, 0.5)];
    let (mut a, mut b) = (
        with_contrast(a, text, target),
        with_contrast(b, text, target),
    );
    // The sRGB midpoint can be lighter than both stops; tighten until it passes.
    let mut extra = target;
    for _ in 0..40 {
        if worst_contrast(text, &stops(a, b)) >= target {
            break;
        }
        extra += 0.1;
        a = with_contrast(a, text, extra);
        b = with_contrast(b, text, extra);
    }
    (a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_hsl_round_trip() {
        assert_eq!(parse_hex("#7c4dff").map(to_hex).as_deref(), Some("#7c4dff"));
        assert_eq!(parse_hex("nope"), None);
        for c in [hex(0x7c4dff), hex(0x19d3c5), hex(0xffd36e), WHITE, BLACK] {
            let (h, s, l) = to_hsl(c);
            let back = from_hsl(h, s, l);
            for i in 0..3 {
                assert!((back[i] - c[i]).abs() < 1e-4, "{c:?} → {back:?}");
            }
        }
    }

    #[test]
    fn contrast_matches_wcag_reference_values() {
        assert!((contrast(WHITE, BLACK) - 21.0).abs() < 0.01);
        assert!((contrast(hex(0x767676), WHITE) - 4.54).abs() < 0.02);
    }

    #[test]
    fn with_contrast_reaches_target_in_both_directions() {
        let yellow = hex(0xffff00);
        let fixed = with_contrast(yellow, WHITE, TEXT_CONTRAST);
        assert!(contrast(fixed, WHITE) >= TEXT_CONTRAST);
        let navy = hex(0x000080);
        let fixed = with_contrast(navy, hex(0x101018), TEXT_CONTRAST);
        assert!(contrast(fixed, hex(0x101018)) >= TEXT_CONTRAST);
        // Already fine: unchanged.
        assert_eq!(with_contrast(BLACK, WHITE, TEXT_CONTRAST), BLACK);
    }

    #[test]
    fn gradients_get_readable_text_for_extreme_inputs() {
        for (a, b) in [
            (hex(0xffff00), hex(0x0000ff)),
            (WHITE, hex(0xeeeeee)),
            (hex(0x777777), hex(0x888888)),
            (BLACK, hex(0x111111)),
            (hex(0x00ff00), hex(0xff00ff)),
        ] {
            let (text, a2, b2) = readable_gradient(a, b, WHITE, hex(0x1a1030), TEXT_CONTRAST);
            let w = worst_contrast(text, &[a2, b2, mix(a2, b2, 0.5)]);
            assert!(w >= TEXT_CONTRAST, "{a:?}/{b:?}: {w}");
        }
    }
}
