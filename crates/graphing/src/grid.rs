//! Grid lines, axis ticks and axis label formatting for any zoom level.
//!
//! Major lines use a 1-2-5 progression chosen so labelled lines are at least
//! `target_px` apart; minor lines subdivide a major step into 5 (for 1×10ⁿ
//! and 5×10ⁿ) or 4 (for 2×10ⁿ) parts.

use crate::viewport::Viewport;

/// Default minimum distance between labelled grid lines, in pixels.
pub const DEFAULT_MAJOR_SPACING_PX: f64 = 80.0;

/// A labelled (major) tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    /// World coordinate of the line.
    pub value: f64,
    /// Formatted label, e.g. `-2`, `0.5`, `1.5×10⁷`.
    pub label: String,
}

/// Grid lines for one axis.
#[derive(Clone, Debug, PartialEq)]
pub struct AxisTicks {
    /// Distance between major lines.
    pub major_step: f64,
    /// Distance between minor lines.
    pub minor_step: f64,
    /// Major (labelled) lines within the visible range.
    pub major: Vec<Tick>,
    /// Minor lines within the visible range (excluding those on major lines).
    pub minor: Vec<f64>,
}

/// Grid lines for both axes.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    /// Vertical lines (x values).
    pub x: AxisTicks,
    /// Horizontal lines (y values).
    pub y: AxisTicks,
}

impl Grid {
    /// Grid for a viewport using [`DEFAULT_MAJOR_SPACING_PX`].
    pub fn for_viewport(vp: &Viewport) -> Grid {
        Grid::with_spacing(vp, DEFAULT_MAJOR_SPACING_PX)
    }

    /// Grid with a custom minimum spacing (pixels) between major lines.
    pub fn with_spacing(vp: &Viewport, target_px: f64) -> Grid {
        Grid {
            x: axis_ticks(vp.x_min, vp.x_max, vp.width, target_px),
            y: axis_ticks(vp.y_min, vp.y_max, vp.height, target_px),
        }
    }
}

/// Chooses `(major_step, minor_step)` so that major lines are at least
/// `target_px` pixels apart.
pub fn nice_steps(span: f64, pixels: f64, target_px: f64) -> (f64, f64) {
    let raw = span.abs() * target_px / pixels.max(1.0);
    if !(raw.is_finite() && raw > 0.0) {
        return (1.0, 0.2);
    }
    let mut e = raw.log10().floor();
    let mut f = raw / 10f64.powf(e);
    // Guard against log10 rounding (e.g. raw = 1000 → f = 0.9999…).
    if f >= 9.999_999 {
        e += 1.0;
        f = raw / 10f64.powf(e);
    }
    let (m, div) = if f <= 1.0 + 1e-9 {
        (1.0, 5.0)
    } else if f <= 2.0 + 1e-9 {
        (2.0, 4.0)
    } else if f <= 5.0 + 1e-9 {
        (5.0, 5.0)
    } else {
        (10.0, 5.0)
    };
    let p = if e.abs() < 300.0 {
        10f64.powi(e as i32)
    } else {
        10f64.powf(e)
    };
    let major = m * p;
    (major, major / div)
}

/// Grid lines for one axis covering `[min, max]` drawn over `pixels`.
pub fn axis_ticks(min: f64, max: f64, pixels: f64, target_px: f64) -> AxisTicks {
    let (major_step, minor_step) = nice_steps(max - min, pixels, target_px);
    let mut major = Vec::new();
    let mut minor = Vec::new();
    if major_step > 0.0 && (max - min) / minor_step < 100_000.0 {
        let k0 = (min / major_step - 1e-9).ceil() as i64;
        let k1 = (max / major_step + 1e-9).floor() as i64;
        for k in k0..=k1 {
            let v = k as f64 * major_step;
            major.push(Tick {
                value: v,
                label: format_axis_label(v, major_step),
            });
        }
        let j0 = (min / minor_step - 1e-9).ceil() as i64;
        let j1 = (max / minor_step + 1e-9).floor() as i64;
        let per = (major_step / minor_step).round() as i64;
        for j in j0..=j1 {
            if per > 0 && j % per == 0 {
                continue;
            }
            minor.push(j as f64 * minor_step);
        }
    }
    AxisTicks {
        major_step,
        minor_step,
        major,
        minor,
    }
}

fn superscript(n: i32) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '-' => '⁻',
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            _ => '⁹',
        })
        .collect()
}

/// Formats an axis label for a value that is a multiple of `step`, with
/// just enough decimals for the step, never printing `-0` or binary noise.
/// Very large or very small magnitudes use `m×10ⁿ` notation.
pub fn format_axis_label(value: f64, step: f64) -> String {
    let step = step.abs();
    if value == 0.0 || (step > 0.0 && value.abs() < step * 1e-6) {
        return "0".to_string();
    }
    let step_exp = if step > 0.0 {
        step.log10().floor() as i32
    } else {
        0
    };
    let mag = value.abs();
    if mag >= 1e7 || (mag < 1e-4 && step_exp < -5) {
        let mut e = mag.log10().floor() as i32;
        let mut mant = value / 10f64.powi(e);
        let digits = (e - step_exp).clamp(0, 12) as usize;
        let mut s = format!("{:.*}", digits, mant);
        if s.trim_start_matches('-').starts_with("10") {
            e += 1;
            mant = value / 10f64.powi(e);
            s = format!("{:.*}", digits.saturating_sub(1), mant);
        }
        let s = trim_zeros(&s);
        return format!("{s}×10{}", superscript(e));
    }
    let decimals = (-step_exp).clamp(0, 15) as usize;
    let s = format!("{:.*}", decimals, value);
    let s = trim_zeros(&s);
    if s == "-0" { "0".to_string() } else { s }
}

fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        let t = s.trim_end_matches('0').trim_end_matches('.');
        t.to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_two_five() {
        assert_eq!(nice_steps(20.0, 1000.0, 80.0), (2.0, 0.5));
        assert_eq!(nice_steps(20.0, 2000.0, 80.0), (1.0, 0.2));
        assert_eq!(nice_steps(20.0, 400.0, 80.0), (5.0, 1.0));
        assert_eq!(nice_steps(2000.0, 1000.0, 80.0), (200.0, 50.0));
        let (m, _) = nice_steps(1e-6, 1000.0, 80.0);
        assert!((m - 1e-7).abs() < 1e-20);
    }

    #[test]
    fn ticks_cover_range() {
        let t = axis_ticks(-10.0, 10.0, 1000.0, 80.0);
        let labels: Vec<&str> = t.major.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            ["-10", "-8", "-6", "-4", "-2", "0", "2", "4", "6", "8", "10"]
        );
        assert_eq!(t.minor.len(), 30);
        let t = axis_ticks(0.1, 0.3, 1000.0, 80.0);
        let labels: Vec<&str> = t.major.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "0.1", "0.12", "0.14", "0.16", "0.18", "0.2", "0.22", "0.24", "0.26", "0.28", "0.3"
            ]
        );
    }

    #[test]
    fn label_formatting() {
        assert_eq!(format_axis_label(0.30000000000000004, 0.1), "0.3");
        assert_eq!(format_axis_label(-0.0, 0.1), "0");
        assert_eq!(format_axis_label(1e-17, 0.1), "0");
        assert_eq!(format_axis_label(2.5, 0.5), "2.5");
        assert_eq!(format_axis_label(1500.0, 500.0), "1500");
        assert_eq!(format_axis_label(2e8, 1e8), "2×10⁸");
        assert_eq!(format_axis_label(1.5e8, 5e7), "1.5×10⁸");
        assert_eq!(format_axis_label(-3e-7, 1e-7), "-3×10⁻⁷");
        assert_eq!(format_axis_label(1000.000002, 0.000001), "1000.000002");
    }
}
