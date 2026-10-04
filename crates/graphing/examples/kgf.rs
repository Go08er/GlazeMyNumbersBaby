//! Prints the function analysis panel for each equation given on the
//! command line, e.g. `cargo run -p graphing --example kgf -- "y = x^2" "tan(x)"`.
//!
//! With `--legacy`, the earlier engine's panel instead (behind its gate);
//! with `--gate`, also what that engine alone said and why the gate
//! (`analysis::verify`) dropped what it dropped; `--deg`, `--grad` set the
//! angle unit (radians otherwise).

use graphing::Equation;
use graphing::analysis::{
    AnalysisError, KeyGraphFeatures, analyze, analyze_legacy, analyze_ungated, verify,
};
use graphing::compile::CompileOptions;
use graphing::functions::TrigUnit;

fn print(k: &KeyGraphFeatures) {
    if let Some(e) = k.analysis_error_string() {
        println!("   {e}");
        return;
    }
    for item in k.items() {
        if item.grid_items.is_empty() {
            println!("   {:<22} {}", item.title, item.display_items.join(" | "));
        } else {
            let rows: Vec<String> = item
                .grid_items
                .iter()
                .map(|g| format!("{} {}", g.expression, g.direction))
                .collect();
            println!("   {:<22} {}", item.title, rows.join(" | "));
        }
        if !item.note.is_empty() {
            println!("   {:<22} ({})", "", item.note);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let gate = args.iter().any(|a| a == "--gate");
    let legacy = args.iter().any(|a| a == "--legacy");
    let trig_unit = if args.iter().any(|a| a == "--deg") {
        TrigUnit::Degrees
    } else if args.iter().any(|a| a == "--grad") {
        TrigUnit::Grads
    } else {
        TrigUnit::Radians
    };
    let opts = CompileOptions {
        trig_unit,
        ..CompileOptions::default()
    };
    for a in args.iter().filter(|a| !a.starts_with("--")) {
        let t = std::time::Instant::now();
        // `--legacy`: the earlier engine behind its gate, for comparison.
        let k = match (legacy, Equation::parse(a)) {
            (true, Ok(eq)) => analyze_legacy(&eq, &opts, None).expect("not cancellable"),
            (false, Ok(eq)) => analyze(&eq, &opts),
            (_, Err(_)) => KeyGraphFeatures::error(AnalysisError::AnalysisCouldNotBePerformed),
        };
        let dt = t.elapsed();
        println!("== {a}   ({:.1} ms)", dt.as_secs_f64() * 1e3);
        print(&k);
        if !gate {
            continue;
        }
        let Ok(eq) = Equation::parse(a) else {
            continue;
        };
        let t = std::time::Instant::now();
        let raw = analyze_ungated(&eq, &opts);
        let dt = t.elapsed();
        println!("-- engine alone   ({:.1} ms)", dt.as_secs_f64() * 1e3);
        print(&raw);
        if let Some((_, f)) = eq.explicit() {
            let t = std::time::Instant::now();
            let (_, r, spent) = verify::gate_report(raw, f, &opts);
            let dt = t.elapsed();
            println!(
                "-- gate   ({:.1} ms, {spent} units of work)",
                dt.as_secs_f64() * 1e3
            );
            for (f, w, refuted) in &r.work {
                println!("   step {f:#06x}: {w} units (refuted so far {refuted:#06x})");
            }
            println!(
                "   dropped: refuted {:#06x}, unverified {:#06x}, by rule {:#06x}",
                r.dropped.0, r.dropped.1, r.dropped.2
            );
            for (check, lines) in &r.failures {
                for l in lines {
                    println!("   {check}: {l}");
                }
            }
            for (check, lines) in &r.doubts {
                for l in lines {
                    println!("   (unverified) {check}: {l}");
                }
            }
        }
    }
}
