//! Round 9: what the analysis shows is right or says it can't tell. The
//! reproducers of R9-M-01..03 (close centres, mixed frames, noise cleanup,
//! false periods), checked as "correct or unknown": the gate in
//! `analysis::verify` drops any claim the function contradicts.

use std::f64::consts::{FRAC_PI_2, PI};

use graphing::analysis::{KeyGraphFeatures, Periodicity, analyze_str, flags};

fn k(src: &str) -> KeyGraphFeatures {
    let r = analyze_str(src);
    assert!(r.analysis_error_string().is_none(), "{src}: refused");
    r
}

fn unknown(r: &KeyGraphFeatures, flag: u32) -> bool {
    r.too_complex_features & flag != 0
}

/// The range is unknown, or exactly these pieces: (lo, lo closed, hi, hi
/// closed), each bound to 10⁻⁹ of its size.
fn range_is(src: &str, want: &[(f64, bool, f64, bool)]) {
    let r = k(src);
    if unknown(&r, flags::RANGE) {
        assert!(r.data.range.is_empty(), "{src}: unknown range keeps values");
        return;
    }
    let got = &r.data.range;
    let close = |a: f64, b: f64| a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs());
    let same = got.len() == want.len()
        && got.iter().zip(want).all(|(g, w)| {
            close(g.lo.value, w.0)
                && g.lo.closed == w.1
                && close(g.hi.value, w.2)
                && g.hi.closed == w.3
        });
    assert!(same, "{src}: range {} ({got:?}), want {want:?}", r.range);
}

/// The period is unknown, or `want` (None: not periodic).
fn period_is(src: &str, want: Option<f64>) {
    let r = k(src);
    if unknown(&r, flags::PERIODICITY) {
        return;
    }
    match want {
        Some(p) => {
            assert_eq!(r.periodicity_direction, Periodicity::Periodic, "{src}");
            let got = r.data.period.expect("a period");
            assert!((got - p).abs() <= 1e-9 * p, "{src}: period {got}, want {p}");
        }
        None => assert_eq!(r.periodicity_direction, Periodicity::NotPeriodic, "{src}"),
    }
}

/// The x-intercepts are unknown, or exactly these.
fn zeros_are(src: &str, want: &[f64]) {
    let r = k(src);
    if unknown(&r, flags::ZEROS) {
        return;
    }
    let mut got: Vec<f64> = r.data.zeros.iter().map(|z| z.x).collect();
    got.sort_by(f64::total_cmp);
    assert_eq!(
        got.len(),
        want.len(),
        "{src}: x-intercepts {}",
        r.x_intercept
    );
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g - w).abs() <= 1e-9 * w.abs().max(1.0),
            "{src}: zero {g}, want {w}"
        );
    }
}

/// R9-M-01: two roots 1/16 apart at 10⁹ are two zeros, and f dips to
/// −1/1024 between them.
#[test]
fn close_roots_far_out() {
    let src = "y=(x-1000000000)*(x-1000000000.0625)";
    zeros_are(src, &[1e9, 1e9 + 0.0625]);
    range_is(src, &[(-1.0 / 1024.0, true, f64::INFINITY, false)]);
    let r = k(src);
    if !unknown(&r, flags::MINIMA) {
        assert_eq!(r.data.minima.len(), 1, "{src}");
        let (x, y) = (r.data.minima[0].0.x, r.data.minima[0].1);
        assert!(
            (x - (1e9 + 0.03125)).abs() < 1e-3 && (y + 1.0 / 1024.0).abs() < 1e-9,
            "{src}: {x} {y}"
        );
    }
}

/// R9-M-01: a mixed frame; the minimum is near 10⁶, about 1.1·10⁻⁶, not
/// the far side's 9.8·10¹¹.
#[test]
fn mixed_frame_minimum() {
    let src = "y=(x-1000000)^2+0.0000001+(x/1000000000)^2";
    // Minimum of (x − A)² + c + (x/S)²: A²/(S² + 1) + c.
    let (a, s, c) = (1e6f64, 1e9f64, 1e-7f64);
    let min = a * a / (s * s + 1.0) + c;
    let r = k(src);
    if !unknown(&r, flags::RANGE) {
        let lo = r.data.range[0].lo.value;
        assert!((lo - min).abs() <= 1e-3 * min, "{src}: range {}", r.range);
    }
    zeros_are(src, &[]);
}

/// R9-M-01: sin x + sin(√2·x) is bounded by 2 but never reaches it, and
/// repeats nowhere.
#[test]
fn incommensurate_sines() {
    let src = "y=sin(x)+sin(sqrt(2)*x)";
    range_is(src, &[(-2.0, false, 2.0, false)]);
    period_is(src, None);
}

/// R9-M-02: atan(tan x) + 1 takes every value in (1 − π/2, 1 + π/2),
/// including its y-intercept 1.
#[test]
fn atan_tan_offset() {
    let src = "y=atan(tan(x))+1";
    range_is(src, &[(1.0 - FRAC_PI_2, false, 1.0 + FRAC_PI_2, false)]);
    period_is(src, Some(PI));
}

/// R9-M-02/03: floor(cos x) is −1, 0 and 1, with period 2π.
#[test]
fn floor_of_cosine() {
    let src = "y=floor(cos(x))";
    range_is(
        src,
        &[
            (-1.0, true, -1.0, true),
            (0.0, true, 0.0, true),
            (1.0, true, 1.0, true),
        ],
    );
    period_is(src, Some(2.0 * PI));
}

/// R9-M-03: an offset of 10¹³ leaves sin's period 2π.
#[test]
fn sine_far_offset_period() {
    period_is("y=sin(x)+10000000000000", Some(2.0 * PI));
    period_is("y=cos(x)+10000000000000", Some(2.0 * PI));
}

/// R9-M-02: 1/(1 + cos x) is at least 1/2, never 0.
#[test]
fn reciprocal_of_one_plus_cosine() {
    range_is("y=1/(1+cos(x))", &[(0.5, true, f64::INFINITY, false)]);
}

/// The logistic function never reaches 0 or 1, though floats round to 1.
#[test]
fn logistic_bounds_are_open() {
    range_is("y=exp(x)/(exp(x)+1)", &[(0.0, false, 1.0, false)]);
}

/// What sampling can't refute must not cost what it can show: ordinary
/// answers stay definite.
#[test]
fn ordinary_answers_stay() {
    for (src, range) in [
        ("y=x^2", "y ∈ [0, ∞)"),
        ("y=sin(x)", "y ∈ [−1, 1]"),
        ("y=1/x", "y ∈ ℝ \\ {0}"),
        ("y=sqrt(x)", "y ∈ [0, ∞)"),
        ("y=ln(x)", "y ∈ ℝ"),
        ("y=e^x", "y ∈ (0, ∞)"),
        ("y=sin(x^2)", "y ∈ [−1, 1]"),
        ("y=sin(x)/x", "y ∈ [−0.217234, 1)"),
        ("y=(x^2-1)/(x-1)", "y ∈ ℝ \\ {2}"),
    ] {
        let r = k(src);
        assert_eq!(r.range, range, "{src}");
    }
    let r = k("y=x^3-3x");
    assert_eq!(r.too_complex_features, 0, "{}", r.x_intercept);
    let r = k("y=tan(x)");
    assert_eq!(r.too_complex_features, 0);
}
