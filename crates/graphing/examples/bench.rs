//! Plotting throughput: `cargo run --release -p graphing --example bench`.

use graphing::{Graph, Viewport};
use std::time::Instant;

fn time_frames(g: &Graph, vp: &Viewport, frames: usize, parallel: bool) -> f64 {
    let t = Instant::now();
    for i in 0..frames {
        // Pan a little every frame, as when dragging.
        let mut v = *vp;
        v.pan_pixels(i as f64 * 3.0, i as f64 * 1.5);
        let p = if parallel {
            g.plot_parallel(&v)
        } else {
            g.plot(&v)
        };
        std::hint::black_box(p);
    }
    t.elapsed().as_secs_f64() * 1e3 / frames as f64
}

fn main() {
    let vp = Viewport::default_for_size(1920.0, 1080.0);
    let sets: &[(&str, &[&str])] = &[
        (
            "4 explicit",
            &[
                "y = sin(x)",
                "y = x^3 - 3x",
                "y = tan(x)",
                "y = e^(-x^2) cos(4x)",
            ],
        ),
        (
            "explicit + implicit + inequality",
            &["y = sin(x)", "y = 1/x", "x^2 + y^2 = 25", "y < x^2 - 4"],
        ),
        (
            "4 implicit",
            &[
                "x^2 + y^2 = 25",
                "x^2 - y^2 = 4",
                "sin(x) + cos(y) = 0.5",
                "x^2/16 + y^2/4 = 1",
            ],
        ),
        (
            "undefined-heavy implicit",
            &["x^y = 2", "sqrt(x*y) = 1", "ln(x*y) < 1", "sin(x*y) = 0.5"],
        ),
        (
            "jumps, poles, holes, oscillation",
            &[
                "y = floor(x)",
                "y = sin(1/x)",
                "y = (x^2-1)/(x-1)",
                "y = 1/(x+3)^2",
                "y = arctan(1000000x)",
                "y = x^(1/3)",
            ],
        ),
    ];
    for (name, eqs) in sets {
        let mut g = Graph::new();
        for e in *eqs {
            g.add_equation(e);
        }
        let points: usize = g.plot(&vp).iter().map(|p| p.plot.point_count()).sum();
        let serial = time_frames(&g, &vp, 30, false);
        let parallel = time_frames(&g, &vp, 30, true);
        println!(
            "{name:<36} {serial:7.2} ms/frame serial, {parallel:7.2} ms/frame parallel, {points} curve points"
        );
        for e in *eqs {
            let mut g1 = Graph::new();
            g1.add_equation(e);
            println!("    {e:<30} {:7.2} ms", time_frames(&g1, &vp, 30, false));
        }
    }
    let t = Instant::now();
    for s in [
        "x^3-3x",
        "tan(x)",
        "1/(x^2-4)",
        "e^(-x^2)",
        "sin(x)/x",
        "x+1/x",
    ] {
        std::hint::black_box(graphing::analysis::analyze_str(s));
    }
    println!(
        "analysis of 6 functions: {:.1} ms",
        t.elapsed().as_secs_f64() * 1e3
    );
}
