//! Cost per call of a few functions on the build in force:
//! `cargo run --release -p crmath --example overhead` (with
//! `GRAPHING_CRMATH=baseline` for the baseline build). Comparing a baseline
//! build (dispatching to v3) with one compiled for x86-64-v3 (direct calls)
//! shows what the dispatch costs.

use std::hint::black_box;
use std::time::Instant;

fn time(name: &str, f: fn(f64) -> f64, xs: &[f64]) {
    let mut best = f64::INFINITY;
    for _ in 0..5 {
        let t = Instant::now();
        let mut acc = 0.0;
        for &x in xs {
            acc += f(black_box(x));
        }
        black_box(acc);
        best = best.min(t.elapsed().as_secs_f64() * 1e9 / xs.len() as f64);
    }
    println!("{name:<6} {best:6.2} ns/call");
}

fn main() {
    let xs: Vec<f64> = (0..2_000_000).map(|i| 0.001 + i as f64 * 1e-5).collect();
    println!("build: {:?}", core_math::build());
    time("exp", core_math::exp, &xs);
    time("log", core_math::log, &xs);
    time("sin", core_math::sin, &xs);
    time("atan", core_math::atan, &xs);
    time("cbrt", core_math::cbrt, &xs);
}
