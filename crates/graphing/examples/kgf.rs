//! Prints the function analysis panel for each equation given on the
//! command line, e.g. `cargo run -p graphing --example kgf -- "y = x^2" "tan(x)"`.
//!
//! With `--gate`, also what the engine alone said and why the gate
//! (`analysis::verify`) dropped what it dropped.

use graphing::Equation;
use graphing::analysis::{KeyGraphFeatures, analyze_str, analyze_ungated, verify};
use graphing::compile::CompileOptions;

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
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let gate = args.iter().any(|a| a == "--gate");
    for a in args.iter().filter(|a| *a != "--gate") {
        let t = std::time::Instant::now();
        let k = analyze_str(a);
        let dt = t.elapsed();
        println!("== {a}   ({:.1} ms)", dt.as_secs_f64() * 1e3);
        print(&k);
        if !gate {
            continue;
        }
        let Ok(eq) = Equation::parse(a) else {
            continue;
        };
        let opts = CompileOptions::default();
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
