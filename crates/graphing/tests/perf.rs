//! Throughput: several equations sampled at 1920 px width must be fast
//! enough to re-plot every frame while panning. Thresholds are generous in
//! debug builds (the workspace dev profile is opt-level 1); run
//! `cargo test --release -p graphing --test perf -- --nocapture` for real
//! numbers, or `cargo run --release -p graphing --example bench`.

use graphing::{Graph, Viewport};
use std::time::Instant;

fn per_frame_ms(g: &Graph, vp: &Viewport, frames: usize) -> f64 {
    // Warm up.
    std::hint::black_box(g.plot(vp));
    let t = Instant::now();
    for i in 0..frames {
        let mut v = *vp;
        v.pan_pixels(i as f64 * 7.0, i as f64 * 3.0);
        std::hint::black_box(g.plot(&v));
    }
    t.elapsed().as_secs_f64() * 1e3 / frames as f64
}

fn budget(release_ms: f64) -> f64 {
    if cfg!(debug_assertions) {
        release_ms * 20.0
    } else {
        release_ms
    }
}

#[test]
fn four_explicit_equations_at_1920px() {
    let mut g = Graph::new();
    for e in [
        "y = sin(x)",
        "y = x^3 - 3x",
        "y = tan(x)",
        "y = e^(-x^2) cos(4x)",
    ] {
        g.add_equation(e);
    }
    let vp = Viewport::default_for_size(1920.0, 1080.0);
    let ms = per_frame_ms(&g, &vp, 20);
    println!("4 explicit equations @1920x1080: {ms:.2} ms/frame");
    assert!(ms < budget(16.0), "{ms} ms per frame");
}

#[test]
fn mixed_equations_at_1920px() {
    let mut g = Graph::new();
    for e in ["y = 1/x", "x^2 + y^2 = 25", "y < x^2 - 4", "x = sin(y)"] {
        g.add_equation(e);
    }
    let vp = Viewport::default_for_size(1920.0, 1080.0);
    let ms = per_frame_ms(&g, &vp, 20);
    println!("mixed equations @1920x1080: {ms:.2} ms/frame");
    assert!(ms < budget(16.0), "{ms} ms per frame");
}

#[test]
fn dense_implicit_curve_at_1920px() {
    let mut g = Graph::new();
    g.add_equation("sin(x) + cos(y) = 0.5");
    let vp = Viewport::default_for_size(1920.0, 1080.0);
    let ms = per_frame_ms(&g, &vp, 5);
    println!("sin(x) + cos(y) = 0.5 @1920x1080: {ms:.2} ms/frame");
    assert!(ms < budget(40.0), "{ms} ms per frame");
}

#[test]
fn batch_evaluation_throughput() {
    let p = graphing::compile::compile_str("sin(3x) * e^(-x/4) + x^2", graphing::TrigUnit::Radians)
        .unwrap();
    let xs: Vec<f64> = (0..100_000).map(|i| i as f64 * 1e-4).collect();
    let mut out = vec![0.0; xs.len()];
    let t = Instant::now();
    p.eval_batch(
        graphing::compile::Input::Slice(&xs),
        graphing::compile::Input::Scalar(0.0),
        &mut out,
    );
    let ns = t.elapsed().as_secs_f64() * 1e9 / xs.len() as f64;
    println!("batch eval: {ns:.1} ns/point");
    assert!(ns < budget(200.0));
}
