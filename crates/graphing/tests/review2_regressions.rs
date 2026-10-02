//! Regression tests for the second external review (REVIEW_2.md: R2-H-01,
//! R2-H-02, R2-M-04). Hostile inputs run on a 2 MiB thread, the default
//! size of a spawned thread (and of plot worker threads).

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use graphing::analysis::AnalysisError;
use graphing::diff::derivative;
use graphing::error::{ErrorCode, EvaluationErrorCode};
use graphing::functions::{self, TrigUnit};
use graphing::graph::Graph;
use graphing::parser::{MAX_EXPRESSION_LEN, parse_expression};
use graphing::viewport::Viewport;

fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("no panic or overflow");
}

fn too_complex(g: &Graph, id: u32) -> bool {
    matches!(
        g.error(id).map(|e| e.code),
        Some(ErrorCode::Evaluation(
            EvaluationErrorCode::EquationTooComplexToPlot
        ))
    )
}

/// R2-H-01: every recursive grammar edge is counted and long or deep input
/// is refused, through the same `Graph` path that restores saved equations.
#[test]
fn hostile_input_is_an_error_on_a_small_stack() {
    on_small_stack(|| {
        let n = 20_000;
        let hostile = [
            format!("y={}x", "√".repeat(n)),
            format!("y={}x", "∛".repeat(n)),
            format!("y={}x", "sin ".repeat(n)),
            format!("y=x{}", " !".repeat(n)),
            format!("y=x{}", "°".repeat(n)),
            format!("y={}x", "x+".repeat(n)),
            format!("y={}", "x ".repeat(n)),
            format!("y=1/{}x", "-".repeat(n)),
            format!("y={}x{}", "|".repeat(2_000), "|".repeat(2_000)),
            format!("y={}x{}", "sin(".repeat(n), ")".repeat(n)),
            // Within the length limit, beyond the nesting limit.
            format!("y={}x", "√".repeat(498)),
            format!("y={}x", "x+".repeat(497)),
            format!("y=x{}", "°".repeat(995)),
        ];
        let mut g = Graph::new();
        for text in &hostile {
            let id = g.add_equation(text);
            assert!(too_complex(&g, id), "{}…", &text[..12]);
            assert!(g.set_equation_text(id, text));
            assert!(too_complex(&g, id));
            assert_eq!(
                g.analyze(id).analysis_error,
                AnalysisError::AnalysisCouldNotBePerformed
            );
        }
        assert!(g.plot(&Viewport::default_for_size(640.0, 480.0)).is_empty());
    });
}

#[test]
fn length_limit() {
    let at_limit = format!("x{}", " ".repeat(MAX_EXPRESSION_LEN - 1));
    assert!(parse_expression(&at_limit).is_ok());
    let over = format!("x{}", " ".repeat(MAX_EXPRESSION_LEN));
    assert!(parse_expression(&over).is_err());
}

/// The deepest inputs that are still accepted survive every downstream
/// stage (compile, plot, symbolic derivative, analysis, printing, drop).
#[test]
fn deepest_accepted_inputs_survive_downstream_on_a_small_stack() {
    on_small_stack(|| {
        let accepted = [
            format!("y={}x", "√".repeat(198)),
            format!("y={}x", "x+".repeat(254)),
            format!("y=x{}", " !".repeat(250)),
            format!("y={}x", "sin ".repeat(198)),
            format!("y={}x{}", "(".repeat(99), ")".repeat(99)),
            format!("y=x{}", "^x".repeat(99)),
            format!("y={}", "x ".repeat(250)),
            format!("y={}x{}", "|".repeat(99), "|".repeat(99)),
            format!("y=1/{}x", "-".repeat(198)),
            format!("y=max({}x)", "x,".repeat(250)),
            format!("y={}x{}", "sin(".repeat(99), ")".repeat(99)),
            format!("{}=y", "sin(x)+".repeat(140) + "y"),
        ];
        let vp = Viewport::default_for_size(640.0, 480.0);
        for text in &accepted {
            let mut g = Graph::new();
            let id = g.add_equation(text);
            assert!(g.error(id).is_none(), "{}…: {:?}", &text[..12], g.error(id));
            let _ = g.plot(&vp);
            let start = Instant::now();
            let _ = g.analyze(id);
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "{}… analysis took {:?}",
                &text[..12],
                start.elapsed()
            );
            let expr = parse_expression(text.split_once('=').map_or(text.as_str(), |s| s.1))
                .unwrap_or_else(|_| parse_expression("x").unwrap());
            if let Some(d) = derivative(&expr, TrigUnit::Radians) {
                let _ = d.to_string();
            }
            let _ = expr.to_string();
        }
    });
}

/// R2-H-02: analysis does a bounded amount of work, reporting TooComplex
/// instead of running for seconds (the review measured ~9 s here).
#[test]
fn analysis_of_nested_radicals_is_bounded() {
    let mut g = Graph::new();
    let id = g.add_equation(&format!("y={}x", "√".repeat(100)));
    assert!(g.error(id).is_none());
    let start = Instant::now();
    let k = g.analyze(id);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "took {:?}",
        start.elapsed()
    );
    assert_eq!(k.analysis_error, AnalysisError::TooComplex);
    assert!(k.analysis_error_string().is_some());
}

#[test]
fn derivatives_are_bounded() {
    // Each argument of max doubles the naive derivative.
    let e = parse_expression(&format!("max({}x)", "x^2,".repeat(80))).unwrap();
    let start = Instant::now();
    assert!(derivative(&e, TrigUnit::Radians).is_none());
    assert!(start.elapsed() < Duration::from_secs(1));
    // Ordinary derivatives are unaffected.
    let e = parse_expression("max(x, x^2, sin(x))").unwrap();
    assert!(derivative(&e, TrigUnit::Radians).is_some());
}

#[test]
fn analysis_can_be_cancelled() {
    let mut g = Graph::new();
    let id = g.add_equation("y=x^3-2x+sin(x)");
    let go = AtomicBool::new(false);
    let full = g.analyze_cancellable(id, &go).expect("not cancelled");
    assert_eq!(full.analysis_error, AnalysisError::NoError);
    assert_eq!(full.items(), g.analyze(id).items());
    let stop = AtomicBool::new(true);
    assert!(g.analyze_cancellable(id, &stop).is_none());
    // Errors that need no work are still reported.
    let bad = g.add_equation("y=");
    assert!(g.analyze_cancellable(bad, &stop).is_some());
}

#[test]
fn plot_can_be_cancelled() {
    let mut g = Graph::new();
    g.add_equation("y=sin(x)");
    g.add_equation("sin(x*y)<0");
    let vp = Viewport::default_for_size(800.0, 600.0);
    let go = AtomicBool::new(false);
    let plots = g
        .plot_parallel_cancellable(&vp, &go)
        .expect("not cancelled");
    let direct = g.plot_parallel(&vp);
    assert_eq!(plots.len(), 2);
    for (a, b) in plots.iter().zip(&direct) {
        assert_eq!(a.plot, b.plot);
    }
    let stop = AtomicBool::new(true);
    assert!(g.plot_parallel_cancellable(&vp, &stop).is_none());
}

/// R2-H-02: implicit relations plotted together share one work budget.
#[test]
fn many_heavy_inequalities_are_bounded() {
    let mut g = Graph::new();
    for _ in 0..14 {
        g.add_equation("sin(x*y)<0");
    }
    let vp = Viewport::default_for_size(1920.0, 1080.0);
    let start = Instant::now();
    let plots = g.plot_parallel(&vp);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "took {:?}",
        start.elapsed()
    );
    let vertices: usize = plots
        .iter()
        .map(|p| p.plot.point_count() + p.plot.fill.iter().map(Vec::len).sum::<usize>())
        .sum();
    // One such inequality alone at full resolution has ~600,000.
    assert!(vertices < 3_000_000, "{vertices} vertices");
    // A lone simple relation keeps full resolution.
    let mut one = Graph::new();
    one.add_equation("x^2+y^2=25");
    let alone = one.plot(&vp);
    assert!(alone[0].plot.point_count() > 1000);
}

/// R2-M-04: coefficients near the float limit do not overflow early.
#[test]
fn ncr_near_float_max_is_finite() {
    let close = |a: f64, b: f64| ((a - b) / b).abs() < 1e-12;
    for (n, r, want) in [
        (1018.0, 509.0, 7.022554277884237e304),
        (1019.0, 509.0, 1.4031338841498113e305),
        (1020.0, 510.0, 2.8062677682996225e305),
        (1021.0, 510.0, 5.607043818853062e305),
        (1022.0, 511.0, 1.1214087637706124e306),
    ] {
        let got = functions::ncr(n, r);
        assert!(close(got, want), "nCr({n}, {r}) = {got}, want {want}");
        assert!(close(functions::ncr(n, n - r), want));
    }
    assert_eq!(functions::ncr(1030.0, 515.0), f64::INFINITY);
    assert_eq!(functions::ncr(1e20, 1e19), f64::INFINITY);
    // Exact small values are unchanged.
    assert_eq!(functions::ncr(60.0, 30.0), 118_264_581_564_861_424.0);
    assert_eq!(functions::ncr(52.0, 5.0), 2_598_960.0);
    // Non-integer arguments beyond Γ's range (computed through ln Γ, so a
    // little less precise).
    let near = |a: f64, b: f64| ((a - b) / b).abs() < 1e-9;
    assert!(near(functions::ncr(1000.5, 2.0), 499_999.875));
    assert!(near(functions::npr(1000.5, 2.0), 999_999.75));
    assert!(close(functions::ncr(10.5, 2.0), 10.5 * 9.5 / 2.0));

    let mut g = Graph::new();
    let id = g.add_equation("y=nCr(1021,510)");
    assert!(g.error(id).is_none());
    let k = g.analyze(id);
    assert_eq!(k.analysis_error, AnalysisError::NoError);
}
