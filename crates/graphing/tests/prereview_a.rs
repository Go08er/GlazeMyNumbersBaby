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

/// F4 (and pre-review D, M1, L1): the slowest functions known take about
/// a second at most, and a cancelled analysis returns soon after the
/// flag, whenever it is set (generous limits: shared CI machines).
#[test]
fn f4_heavy_analyses_are_bounded_and_cancel_promptly() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    let sum = |n: usize, f: &dyn Fn(usize) -> String| (1..=n).map(f).collect::<Vec<_>>().join("+");
    let list = |n: usize, f: &dyn Fn(usize) -> String| {
        format!("max({})", (1..=n).map(f).collect::<Vec<_>>().join(","))
    };
    let mut nested = "x".to_string();
    for _ in 0..40 {
        nested = format!("sin({nested})");
    }
    let cases = [
        sum(40, &|k| format!("sin({k}x)/{k}")),
        sum(39, &|k| format!("floor({k}x)/{k}")),
        sum(29, &|k| format!("e^(-(x-{k})^2)")),
        nested,
        (1..=20)
            .map(|k| format!("sin({k}x)"))
            .collect::<Vec<_>>()
            .join("*"),
        "x+tan(190x)".to_string(),
        "x+tan(x)-30000".to_string(),
        list(79, &|k| format!("tan({k}x)")),
        list(59, &|k| format!("sin({k}x)^{k}")),
    ];
    let slow = if cfg!(debug_assertions) { 30.0 } else { 3.0 };
    // (Unloaded, release: under 10 ms mostly, 80 ms at worst, when the
    // flag comes during a simplifier run, which only its node limit
    // bounds. Generous here: CI machines are shared.)
    let lag = if cfg!(debug_assertions) {
        Duration::from_millis(5000)
    } else {
        Duration::from_millis(1000)
    };
    for src in &cases {
        let eq = Arc::new(Equation::parse(&format!("y={src}")).unwrap());
        let opts = CompileOptions::default();
        let t = Instant::now();
        let r = analyze(&eq, &opts);
        let took = t.elapsed().as_secs_f64();
        assert_eq!(r.analysis_error, AnalysisError::NoError, "{src}");
        assert!(took < slow, "{src}: {took:.2} s");
        for at in [50, 200, 500] {
            let cancel = Arc::new(AtomicBool::new(false));
            let (e, c) = (eq.clone(), cancel.clone());
            let worker = std::thread::spawn(move || {
                let opts = CompileOptions::default();
                let r = graphing::analysis::analyze_cancellable(&e, &opts, Some(&c));
                (r.is_none(), Instant::now())
            });
            std::thread::sleep(Duration::from_millis(at));
            let set = Instant::now();
            cancel.store(true, Ordering::Relaxed);
            let (cancelled, done) = worker.join().unwrap();
            let after = done.saturating_duration_since(set);
            if std::env::var_os("LAG_VERBOSE").is_some() {
                println!(
                    "{:.30}: cancel at {at} ms: {after:?} later (None: {cancelled})",
                    src
                );
            }
            assert!(
                after <= lag,
                "{src}: cancelled at {at} ms, returned {after:?} later (None: {cancelled})"
            );
        }
    }
}

/// L5: a slider at 0 makes a/x the zero function on x ≠ 0: both even and
/// odd, constant on each piece, the whole list (no "some of them").
#[test]
fn l5_a_slider_at_zero_gives_the_zero_function() {
    let eq = Equation::parse("y = a/x").unwrap();
    let vars = [("a", 0.0)];
    let r = analyze(
        &eq,
        &CompileOptions {
            trig_unit: TrigUnit::Radians,
            variables: &vars,
        },
    );
    assert_eq!(r.parity, graphing::analysis::Parity::Both);
    assert!(!r.monotonicity.is_empty());
    assert!(
        r.monotonicity
            .iter()
            .all(|(_, m)| *m == Monotonicity::Constant)
    );
    assert_eq!(
        r.partial_features & graphing::analysis::flags::MONOTONE_INTERVALS,
        0,
        "{:?}",
        r.notes
    );
}

/// L2: zeros 2·10⁻¹⁵ apart don't read alike.
#[test]
fn l2_close_zeros_read_apart() {
    let r = rad("(x-1)^2-10^(-30)");
    if !r.x_intercept.is_empty() {
        let xs: Vec<&str> = r.x_intercept.split(", ").collect();
        assert_eq!(xs.len(), 2, "{}", r.x_intercept);
        assert_ne!(xs[0], xs[1]);
    }
}

/// L3: |x| − x is 0 on all of [0, ∞): not a list of points.
#[test]
fn l3_a_ray_of_zeros_is_no_list_of_points() {
    for src in ["abs(x)-x", "abs(x)+x"] {
        let r = rad(src);
        assert!(
            r.x_intercept.is_empty() || r.x_intercept.contains('['),
            "{src}: {}",
            r.x_intercept
        );
    }
    // A zero alone between undecided boxes is still listed.
    assert_eq!(rad("sign(x)").x_intercept, "0");
}

/// L4: a slope of 1 is written x.
#[test]
fn l4_unit_slopes_read_x() {
    let r = rad("abs(x-pi)");
    for l in &r.oblique_asymptotes {
        assert!(!l.contains("1x"), "{l}");
    }
}

/// L7: one family of excluded points once, with its exact period.
#[test]
fn l7_families_once_with_exact_periods() {
    let d = rad("x^2*tan(x+0.5)/tan(x+0.5)").domain;
    assert_eq!(d.matches("1.0708").count(), 1, "{d}");
    assert!(!d.contains("3.14159k"), "{d}");
    let d = rad("x^2*tan(10^10*x+0.5)/tan(10^10*x+0.5)").domain;
    assert!(!d.contains("×10⁻¹⁰k"), "{d}");
}

/// L8: a limit the range knows exactly is written exactly.
#[test]
fn l8_constant_tails_give_exact_limits() {
    let r = rad("atan(x)+atan(1/x)");
    assert_eq!(r.range, "y ∈ {−π/2, π/2}");
    assert_eq!(r.horizontal_asymptotes, ["y = π/2", "y = −π/2"]);
}
