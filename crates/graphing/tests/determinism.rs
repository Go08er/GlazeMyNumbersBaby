//! The same function always gets the same analysis: certificates and panel
//! rows are byte-identical between runs, even when the machine is busy.
//! (Every limit in the certifier and the simplifier is a count — of
//! evaluations, e-graph nodes, rounds — never a time.)

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use graphing::analysis::analyze_str;
use graphing::certify::{DEFAULT_BUDGET, certify_text};
use graphing::compile::CompileOptions;

/// Ordinary functions, ones that need the simplifier (holes, identities)
/// and ones that run its derivative trees, plus shifted and scaled forms.
const FUNCTIONS: &[&str] = &[
    "x^2",
    "x^3-2x+1/(x-1)",
    "sin(x)",
    "tan(x)",
    "1/x",
    "ln(x)",
    "sqrt(x)",
    "e^x",
    "(x^2-1)/(x-1)",
    "x^x",
    "sin(x)/x",
    "x*e^(-x)",
    "atan(1/x)",
    "e^(-1/x^2)",
    "1/(1+e^x)",
    "x*ln(x)",
    "tan(x)*cos(x)",
    "cos(x)^2/cos(x)^2",
    "abs(x)",
    "x*abs(x)",
    "sqrt(1/x)",
    "coth(x)",
    "(x-1000)^2+3",
    "sin(x/1000)",
    "1/(x^2+0.000001)",
    "exp(-(x-100)^2)+exp(-x^2)",
];

/// Everything one analysis produces, as text.
fn run(src: &str) -> String {
    let cert = certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None)
        .map(|a| serde_json::to_string(&a).expect("certificate serialises"))
        .unwrap_or_else(|e| format!("error: {e}"));
    let panel = format!("{:?}", analyze_str(&format!("y={src}")));
    format!("{cert}\n{panel}")
}

/// Keeps every core busy until dropped.
struct Load {
    stop: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl Load {
    fn start() -> Load {
        let stop = Arc::new(AtomicBool::new(false));
        let n = std::thread::available_parallelism().map_or(4, |n| n.get()) * 2;
        let threads = (0..n)
            .map(|i| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    let mut x = i as f64 + 0.5;
                    while !stop.load(Ordering::Relaxed) {
                        for _ in 0..10_000 {
                            x = (x * 1.000_001).sin() + 1.5;
                        }
                    }
                    std::hint::black_box(x);
                })
            })
            .collect();
        Load { stop, threads }
    }
}

impl Drop for Load {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

#[test]
fn analyses_are_identical_run_to_run_and_under_load() {
    let first: Vec<String> = FUNCTIONS.iter().map(|f| run(f)).collect();
    let second: Vec<String> = FUNCTIONS.iter().map(|f| run(f)).collect();
    let loaded: Vec<String> = {
        let _load = Load::start();
        FUNCTIONS.iter().map(|f| run(f)).collect()
    };
    for (i, f) in FUNCTIONS.iter().enumerate() {
        assert_eq!(first[i], second[i], "y={f}: two runs differ");
        assert_eq!(
            first[i], loaded[i],
            "y={f}: differs when the machine is busy"
        );
    }
}
