//! Round 10: what the analysis shows is right or says it can't tell. The
//! reproducers of R10-M-02..05 and of the independent oracle
//! (`examples/oracle.rs`): values below the doubles taken for zeros, poles
//! sampled but not reported, peaks with flat tops or at centres written as
//! products, the panel's own number formatting, unbounded noise, tails and
//! edges that only approach a range's end. Checked as "correct or unknown";
//! the last test is the control that ordinary answers stay.

use graphing::analysis::{KeyGraphFeatures, Parity, analyze_str, flags};

fn k(src: &str) -> KeyGraphFeatures {
    let r = analyze_str(src);
    assert!(r.analysis_error_string().is_none(), "{src}: refused");
    r
}

fn unknown(r: &KeyGraphFeatures, flag: u32) -> bool {
    r.too_complex_features & flag != 0
}

/// Every text the panel shows for the function.
fn shown(r: &KeyGraphFeatures) -> String {
    let mut all = vec![
        r.domain.clone(),
        r.range.clone(),
        r.x_intercept.clone(),
        r.y_intercept.clone(),
    ];
    for l in [
        &r.minima,
        &r.maxima,
        &r.inflection_points,
        &r.vertical_asymptotes,
        &r.horizontal_asymptotes,
    ] {
        all.extend(l.iter().cloned());
    }
    all.extend(r.monotonicity.iter().map(|m| m.0.clone()));
    all.join(" | ")
}

/// R10-M-02: a sum of Gaussians is positive everywhere; below the doubles
/// it is still not 0. No zero, no minimum of 0, no closed range end at 0.
#[test]
fn positive_values_below_the_doubles_are_no_zeros() {
    for src in [
        "y=exp(-(x-100)^2)+exp(-x^2)",
        "y=exp(-(x-1000)^2)+exp(-x^2)",
        "y=exp(-(x-1234)^2)+exp(-x^2)",
        "y=exp(-(x-50)^2)+exp(-(x+70)^2)",
        "y=exp(-(x-100)^2)+exp(-(x+100)^2)",
        "y=exp(-(x-1000)^2)+exp(-(x+1000)^2)",
        "y=exp(-(x-1234)^2)+exp(-(x+1234)^2)",
    ] {
        let r = k(src);
        assert!(
            r.x_intercept.is_empty(),
            "{src}: x-intercepts {}",
            r.x_intercept
        );
        assert!(
            r.data.minima.iter().all(|m| m.1 != 0.0),
            "{src}: minima {:?}",
            r.minima
        );
        assert!(
            r.data
                .range
                .iter()
                .all(|i| !(i.lo.value == 0.0 && i.lo.closed)),
            "{src}: range {}",
            r.range
        );
        // (Nor the monotone pieces split at a false minimum.)
        assert!(!shown(&r).contains("48.5941"), "{src}: {}", shown(&r));
    }
}

/// The oracle's class: f(0) a nonzero value below (or above) the doubles,
/// shown as a y-intercept of 0.
#[test]
fn y_intercept_beyond_the_doubles_is_not_zero() {
    for src in [
        "y=e^(x-1000)",
        "y=(x-1000)*e^(x-1000)",
        "y=csch(x-1000)",
        "y=2^(x-1000000)",
        "y=e^(-(x-1000)^2)",
        "y=(x-1000)^(x-1000)",
        "y=exp(-(x-100)^2)+exp(-(x+100)^2)",
        "y=exp(-(x-7*11*13)^2/0.01)+x^2/10^9",
        "y=sqrt(exp(-(x+800)))*exp((x+800)/2)",
    ] {
        let r = k(src);
        assert_ne!(r.y_intercept, "0", "{src}");
    }
}

/// R10-M-03: the pole of a quotient whose centres are close and far out,
/// sampled undefined at its centre, is a vertical asymptote or the
/// asymptotes are unknown; never "none".
#[test]
fn a_sampled_pole_is_not_missed() {
    for src in [
        "y=(x-1000000000000)/(x-1000000000000.0625)",
        "y=(x-1000000000)/(x-1000000000.0625)",
        "y=(x-2^40)/(x-2^40-1/16)",
    ] {
        let r = k(src);
        assert!(
            unknown(&r, flags::VERTICAL_ASYMPTOTES) || !r.vertical_asymptotes.is_empty(),
            "{src}: {:?}",
            r.vertical_asymptotes
        );
    }
}

/// R10-M-04: a second peak far out, its centre written as a literal or as a
/// product of literals, with a top flat to the floats: the maxima include it
/// or are unknown; parity and monotonicity don't pretend it isn't there.
#[test]
fn far_and_flat_topped_peaks_are_not_missed() {
    for (src, c) in [
        ("y=2*exp(-(x-1522756)^2)+exp(-x^2)", 1522756.0),
        ("y=exp(-(x-1234*1234)^2)+exp(-x^2)", 1522756.0),
        ("y=3*exp(-(x-2^20)^2)+exp(-x^2)", 1048576.0),
        ("y=exp(-(x-123456)^2)+exp(-x^2)", 123456.0),
        ("y=exp(-(x-123456)^2)+exp(-(x+123456)^2)", 123456.0),
        ("y=exp(-(x-1000*1000)^2)+exp(-(x-999*1001)^2)", 999999.5),
    ] {
        let r = k(src);
        assert!(
            unknown(&r, flags::MAXIMA) || r.data.maxima.iter().any(|m| (m.0.x - c).abs() <= 1.0),
            "{src}: maxima {:?}",
            r.maxima
        );
        if !src.contains("(x+123456)") {
            assert_ne!(r.parity, Parity::Even, "{src}");
        }
        assert!(
            !r.monotonicity.iter().any(|m| m.0 == "(0, ∞)"),
            "{src}: {:?}",
            r.monotonicity
        );
    }
    // A narrow bump on a wide bowl: the bump's top and the dip beside it.
    let r = k("y=exp(-(x-7*11*13)^2/0.01)+x^2/10^9");
    assert!(
        unknown(&r, flags::MAXIMA) || !r.maxima.is_empty(),
        "{:?}",
        r.maxima
    );
    assert!(
        unknown(&r, flags::MINIMA) || r.data.minima.iter().any(|m| (m.0.x - 1001.4).abs() < 1.0),
        "{:?}",
        r.minima
    );
    assert_ne!(r.parity, Parity::Even);
    assert!(
        !r.monotonicity.iter().any(|m| m.0 == "(0, ∞)"),
        "{:?}",
        r.monotonicity
    );
    // A minimum at a far centre, not "none".
    let r = k("y=(x-1000000)^2+1+(x/1000000000)^2");
    assert!(
        unknown(&r, flags::MINIMA) || !r.minima.is_empty(),
        "{:?}",
        r.minima
    );
}

/// R10-M-05: values compared as the panel writes them (whole numbers in
/// full to 10¹⁵), and an asymmetry far out found before rounding near 0.
#[test]
fn values_read_as_the_panel_writes_them() {
    let r = k("y=sin(x)+10000000000000");
    assert!(
        unknown(&r, flags::RANGE)
            || (r.range.contains("9999999999999") && r.range.contains("10000000000001")),
        "{}",
        r.range
    );
    let r = k("y=3*exp(-(x-2^20)^2)+exp(-x^2)");
    assert_ne!(r.range, "y ∈ (0, 1]");
    // The limit of 10¹²·atan(1/x) at 0 is 1570796326794.8966, not the
    // whole number next to it.
    let r = k("y=1000000000000*atan(1/x)");
    assert!(!r.range.contains("1570796326795"), "{}", r.range);
    let r = k("y=0.000001*(1/(1+e^(x+1000000)))+1000000");
    assert_ne!(r.parity, Parity::Even);
    // (10⁶, 10⁶ + 10⁻⁶), but the panel writes both ends as 1000000.
    assert_ne!(r.range, "y ∈ (1000000, 1000000)");
}

/// A value whose noise no bound limits (an intermediate past the doubles)
/// vouches for nothing: 1 + √(−e^(−x−800)) is undefined everywhere, and
/// √(e^(−x−800))·e^((x+800)/2) is 1.
#[test]
fn unbounded_noise_vouches_for_nothing() {
    // (Refusing it, as nowhere defined, is right too.)
    let r = analyze_str("y=1+sqrt(-exp(-(x+800)))");
    assert!(!r.domain.contains("54.8668"), "{}", r.domain);
    assert_ne!(r.y_intercept, "1");
    assert!(
        r.horizontal_asymptotes.is_empty(),
        "{:?}",
        r.horizontal_asymptotes
    );
    let r = k("y=sqrt(exp(-(x+800)))*exp((x+800)/2)");
    assert_ne!(r.y_intercept, "0");
}

/// The control: ordinary answers stay, including ranges whose ends are only
/// approached (a pole, a slow tail, an edge of the domain).
#[test]
fn ordinary_answers_stay() {
    for (src, range) in [
        ("y=x^2", "y ∈ [0, ∞)"),
        ("y=sin(x)", "y ∈ [−1, 1]"),
        ("y=tan(x)", "y ∈ ℝ"),
        ("y=1/x", "y ∈ ℝ \\ {0}"),
        ("y=ln(x)", "y ∈ ℝ"),
        ("y=e^(-x^2)", "y ∈ (0, 1]"),
        ("y=1/(1+e^-x)", "y ∈ (0, 1)"),
        ("y=atan(1/x)", "y ∈ (−π/2, 0) ∪ (0, π/2)"),
        ("y=1/(sqrt(x)-1)", "y ∈ (−∞, −1] ∪ (0, ∞)"),
        ("y=x^-0.01", "y ∈ (0, ∞)"),
        ("y=exp(-atanh(x))", "y ∈ (0, ∞)"),
        ("y=sin(x^2)", "y ∈ [−1, 1]"),
    ] {
        assert_eq!(k(src).range, range, "{src}");
    }
    let r = k("y=e^(-x^2)");
    assert_eq!(r.maxima, ["(0, 1)"]);
    assert_eq!(r.parity, Parity::Even);
    assert_eq!(r.horizontal_asymptotes, ["y = 0"]);
    let r = k("y=1/(1+e^-x)");
    assert_eq!(r.horizontal_asymptotes, ["y = 1", "y = 0"]);
    let r = k("y=exp(-(x-100)^2)+exp(-x^2)");
    assert_eq!(r.maxima, ["(0, 1)", "(100, 1)"]);
    assert_eq!(r.horizontal_asymptotes, ["y = 0"]);
}
