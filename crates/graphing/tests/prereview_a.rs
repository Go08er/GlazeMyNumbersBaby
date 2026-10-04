//! Pre-review A (red team on the certified analysis at 693dfad): the
//! panel's findings, pinned. The soundness findings (F1, F3) are in the
//! certificate corpus and replay (`certify_corpus.rs`, `certify_replay.rs`).

use graphing::analysis::{AnalysisError, KeyGraphFeatures, Monotonicity, analyze};
use graphing::compile::CompileOptions;
use graphing::equation::Equation;
use graphing::functions::TrigUnit;

fn panel(src: &str, unit: TrigUnit) -> KeyGraphFeatures {
    let eq = Equation::parse(src).unwrap();
    let r = analyze(
        &eq,
        &CompileOptions {
            trig_unit: unit,
            variables: &(),
        },
    );
    assert_eq!(r.analysis_error, AnalysisError::NoError, "{src}");
    r
}

fn rad(src: &str) -> KeyGraphFeatures {
    panel(src, TrigUnit::Radians)
}

/// F2: over one period of a period known only to an enclosure (π², 360/π)
/// each monotone piece is a family, and the data repeats with it.
#[test]
fn f2_monotone_pieces_repeat_with_an_enclosed_period() {
    let cases = [
        ("tan(x/pi)", TrigUnit::Radians),
        ("sin(x/pi)", TrigUnit::Radians),
        ("cos(pi^2*x)", TrigUnit::Radians),
        ("sin(pi*x)", TrigUnit::Degrees),
        ("tan(pi*x)", TrigUnit::Degrees),
        ("cos(pi*x)", TrigUnit::Degrees),
        ("sin(2*pi*x)+1", TrigUnit::Degrees),
    ];
    for (src, unit) in cases {
        let r = panel(src, unit);
        assert!(!r.monotonicity.is_empty(), "{src} [{unit:?}]: no pieces");
        for (t, _) in &r.monotonicity {
            assert!(t.ends_with(", k ∈ ℤ"), "{src} [{unit:?}]: {t}");
        }
        assert!(
            r.data.period.or(r.data.repeat).is_some(),
            "{src} [{unit:?}]: the pieces repeat with no period"
        );
    }
    assert_eq!(
        rad("tan(x/pi)").monotonicity,
        [(
            "(≈−4.9348 + ≈9.8696k, ≈4.9348 + ≈9.8696k), k ∈ ℤ".to_string(),
            Monotonicity::Increasing
        )]
    );
}

/// L1: a multiple of an approximate period stays marked approximate.
#[test]
fn l1_approximate_multiples_keep_their_mark() {
    assert_eq!(rad("tan(x/pi)").x_intercept, "≈9.8696k, k ∈ ℤ");
    let r = rad("cos(pi^2*x)");
    assert_eq!(r.periodicity_expression, "≈0.63662");
    assert!(
        r.maxima.iter().all(|m| m.contains("≈0.63662k")),
        "{:?}",
        r.maxima
    );
}
