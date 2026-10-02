//! Prints the function analysis panel for each equation given on the
//! command line, e.g. `cargo run -p graphing --example kgf -- "y = x^2" "tan(x)"`.

use graphing::analysis::analyze_str;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    for a in &args {
        let t = std::time::Instant::now();
        let k = analyze_str(a);
        let dt = t.elapsed();
        println!("== {a}   ({:.1} ms)", dt.as_secs_f64() * 1e3);
        if let Some(e) = k.analysis_error_string() {
            println!("   {e}");
            continue;
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
}
