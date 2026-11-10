//! Bringing a sound file to the mixer's rate: a band-limited
//! (windowed-sinc) resampler, 32 taps wide when the rate goes up and wider
//! by the ratio when it goes down, so that what would fold back is
//! filtered out first.

use std::sync::OnceLock;

use core_math as cr;

/// Zero crossings of the sinc on each side of its middle.
const HALF: usize = 16;
/// Table entries per zero crossing; between them, a straight line.
const STEPS: usize = 512;

/// The kernel `sinc(x) × blackman(x / HALF)` for `x` from 0 to `HALF`.
fn table() -> &'static [f64] {
    static TABLE: OnceLock<Vec<f64>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let pi = crate::recipe::TAU / 2.0;
        (0..=HALF * STEPS)
            .map(|k| {
                let x = k as f64 / STEPS as f64;
                let sinc = if k == 0 {
                    1.0
                } else {
                    cr::sin(pi * x) / (pi * x)
                };
                let u = x / HALF as f64;
                let window = 0.42 + 0.5 * cr::cos(pi * u) + 0.08 * cr::cos(2.0 * pi * u);
                sinc * window
            })
            .collect()
    })
}

/// The kernel at `x` (0 beyond its width).
fn kernel(table: &[f64], x: f64) -> f64 {
    let at = x.abs() * STEPS as f64;
    let i = at as usize;
    if i + 1 >= table.len() {
        return 0.0;
    }
    table[i] + (table[i + 1] - table[i]) * (at - i as f64)
}

/// How many frames `frames` at `from` Hz are at `to` Hz.
pub fn resampled_len(frames: usize, from: u32, to: u32) -> usize {
    if from == 0 {
        return 0;
    }
    ((frames as u64 * u64::from(to) + u64::from(from) / 2) / u64::from(from)) as usize
}

/// One channel, from `from` Hz to `to` Hz. What lies before the first
/// sample and after the last counts as silence.
pub fn resample(input: &[f64], from: u32, to: u32) -> Vec<f64> {
    if from == to || from == 0 || to == 0 || input.is_empty() {
        return input.to_vec();
    }
    let table = table();
    // Going down, the kernel is stretched: its cutoff is the new rate's.
    let scale = (f64::from(to) / f64::from(from)).min(1.0);
    let reach = (HALF as f64 / scale).ceil() as i64;
    let n = resampled_len(input.len(), from, to);
    let mut out = Vec::with_capacity(n);
    for k in 0..n as u64 {
        // Where output sample k lies among the input's: a whole part and
        // a fraction, exactly.
        let num = k * u64::from(from);
        let whole = (num / u64::from(to)) as i64;
        let frac = (num % u64::from(to)) as f64 / f64::from(to);
        let (mut sum, mut weight) = (0.0f64, 0.0f64);
        for i in whole - reach + 1..=whole + reach {
            let w = kernel(table, (i - whole) as f64 * scale - frac * scale);
            weight += w;
            if i >= 0 && (i as usize) < input.len() {
                sum += w * input[i as usize];
            }
        }
        // The weights sum to one: a steady level comes out as it went in.
        out.push(if weight != 0.0 { sum / weight } else { 0.0 });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f64, rate: u32, frames: usize) -> Vec<f64> {
        (0..frames)
            .map(|i| (std::f64::consts::TAU * hz * i as f64 / f64::from(rate)).sin())
            .collect()
    }

    fn rms(v: &[f64]) -> f64 {
        (v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64).sqrt()
    }

    #[test]
    fn lengths() {
        assert_eq!(resampled_len(44_100, 44_100, 48_000), 48_000);
        assert_eq!(resampled_len(32_000, 8_000, 48_000), 192_000);
        assert_eq!(resampled_len(384_000, 96_000, 48_000), 192_000);
        assert_eq!(resampled_len(1, 8_000, 48_000), 6);
        assert_eq!(resampled_len(0, 8_000, 48_000), 0);
        assert_eq!(resample(&[], 8_000, 48_000).len(), 0);
        assert_eq!(resample(&[0.25; 100], 44_100, 48_000).len(), 109);
        let same = [0.1, -0.2, 0.3];
        assert_eq!(resample(&same, 48_000, 48_000), same);
    }

    #[test]
    fn a_tone_stays_the_same_tone() {
        for (from, hz) in [
            (44_100, 1000.0),
            (22_050, 3000.0),
            (8_000, 440.0),
            (96_000, 5000.0),
            (11_025, 2500.0),
            (37_123, 1234.5),
        ] {
            let input = tone(hz, from, from as usize / 4);
            let out = resample(&input, from, 48_000);
            let want = tone(hz, 48_000, out.len());
            // Away from the ends, where the silence outside is felt.
            let edge = 400;
            let worst = out[edge..out.len() - edge]
                .iter()
                .zip(&want[edge..])
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f64, f64::max);
            assert!(worst < 2e-3, "{from} Hz, {hz} Hz tone: off by {worst}");
        }
    }

    #[test]
    fn a_steady_level_stays() {
        for from in [8_000, 44_100, 96_000] {
            let out = resample(&[0.5; 2000], from, 48_000);
            let mid = &out[out.len() / 4..out.len() * 3 / 4];
            assert!(mid.iter().all(|v| (v - 0.5).abs() < 1e-9), "{from}");
        }
    }

    #[test]
    fn going_down_filters_what_would_fold_back() {
        // 30 kHz at 96 kHz would come back as 18 kHz at 48 kHz.
        let high = resample(&tone(30_000.0, 96_000, 9600), 96_000, 48_000);
        let level = rms(&high[400..high.len() - 400]);
        assert!(level < 0.003, "30 kHz left at {level}");
        // What is in band passes untouched.
        let low = resample(&tone(5000.0, 96_000, 9600), 96_000, 48_000);
        let level = rms(&low[400..low.len() - 400]);
        assert!((level - 0.5f64.sqrt()).abs() < 0.002, "5 kHz at {level}");
    }

    #[test]
    fn a_click_stays_bounded() {
        let mut input = vec![0.0; 64];
        input[32] = 1.0;
        let out = resample(&input, 8_000, 48_000);
        assert!(out.iter().all(|v| v.is_finite() && v.abs() <= 1.0 + 1e-9));
        // Its top lands where it was, six times further along.
        let top = out
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap();
        assert_eq!(top.0, 192);
    }
}
