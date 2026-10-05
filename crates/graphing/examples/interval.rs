//! Interval enclosures of an expression over a box, and their cost.
//!
//! ```text
//! cargo run --release -p graphing --example interval -- 'tan(x)*cos(x)' 1.5 1.6 [deg|grad]
//! cargo run --release -p graphing --example interval -- --bench
//! ```
//!
//! Prints f's enclosure over [lo, hi] with its decoration (com: defined,
//! continuous and bounded there; dac: defined and continuous; def: defined;
//! trv: maybe undefined somewhere) and strict-sign facts, the Taylor
//! coefficients f′, f″/2, f‴/6 when they're valid, and the same over ten
//! equal parts of the box. `--bench` times interval evaluation at orders
//! 0–3 against the compiled f64 program.

use graphing::Equation;
use graphing::compile::{CompileOptions, Program};
use graphing::functions::TrigUnit;
use graphing::interval::{Ctx, DecInterval, Interval, Literals, derivs_valid, taylor};
use graphing::lexer::ParseOptions;
use std::time::Instant;

fn show(d: &DecInterval) -> String {
    let mut s = if d.is_empty() {
        "∅".to_string()
    } else {
        format!("[{:e}, {:e}]", d.lo(), d.hi())
    };
    s.push_str(&format!(" {:?}", d.dec));
    if d.pos {
        s.push_str(" (> 0)");
    }
    if d.neg {
        s.push_str(" (< 0)");
    }
    s
}

fn parse(src: &str) -> (Equation, Literals) {
    let text = if src.contains('=') {
        src.to_string()
    } else {
        format!("y={src}")
    };
    let eq = Equation::parse(&text).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    let lits = Literals::of(&text, ParseOptions::default()).expect("parsed once already");
    (eq, lits)
}

fn print(src: &str, lo: f64, hi: f64, unit: TrigUnit) {
    let (eq, lits) = parse(src);
    let Some((_, ast)) = eq.explicit() else {
        eprintln!("not an explicit function");
        return;
    };
    let opts = CompileOptions {
        trig_unit: unit,
        ..CompileOptions::default()
    };
    let ctx = Ctx::new(opts, &lits);
    let x = Interval::new(lo, hi);
    let s = taylor(ast, x, 3, &ctx);
    println!("f on [{lo:e}, {hi:e}]: {}", show(&s[0]));
    if derivs_valid(&s, 3) {
        for (k, name) in [(1, "f′"), (2, "f″/2"), (3, "f‴/6")] {
            println!("  {name}: {}", show(&s[k]));
        }
    } else {
        println!("  derivatives: not valid on this box");
    }
    if lo.is_finite() && hi.is_finite() && lo < hi {
        println!("in ten parts:");
        for i in 0..10 {
            let a = lo + (hi - lo) * i as f64 / 10.0;
            let b = if i == 9 {
                hi
            } else {
                lo + (hi - lo) * (i + 1) as f64 / 10.0
            };
            let s = taylor(ast, Interval::new(a, b), 1, &ctx);
            println!("  [{a:.6e}, {b:.6e}]: {}", show(&s[0]));
        }
    }
}

fn bench() {
    let cases = [
        "x^3-2x+1/(x-1)",
        "sin(x)*cos(3x)+sin(5x)/x",
        "tan(x)*cos(x)",
        "2*exp(-(x-1522756)^2/0.000001)+exp(-x^2)",
        "sin(x)+sin(sqrt(2)*x)",
        "ln(x)+sqrt(x)+atan(x)",
        "floor(cos(x))",
    ];
    println!(
        "{:<44} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "expression", "f64 ns", "o0 ns", "o1 ns", "o2 ns", "o3 ns"
    );
    for src in cases {
        let (eq, lits) = parse(src);
        let (_, ast) = eq.explicit().expect("explicit");
        let opts = CompileOptions::default();
        let ctx = Ctx::new(opts, &lits);
        let prog = Program::compile_typed(ast, &opts, &lits).expect("compiles");
        let n = 20_000usize;
        let xs: Vec<f64> = (0..n).map(|i| 0.37 + i as f64 * 1e-3).collect();
        let t = Instant::now();
        let mut acc = 0.0;
        for &x in &xs {
            acc += prog.eval(x, 0.0);
        }
        std::hint::black_box(acc);
        let f64_ns = t.elapsed().as_secs_f64() * 1e9 / n as f64;
        let mut row = vec![f64_ns];
        for order in 0..=3 {
            let m = 5_000usize;
            let t = Instant::now();
            for i in 0..m {
                let x = 0.37 + i as f64 * 1e-3;
                let s = taylor(ast, Interval::new(x, x + 1e-4), order, &ctx);
                std::hint::black_box(s);
            }
            row.push(t.elapsed().as_secs_f64() * 1e9 / m as f64);
        }
        println!(
            "{:<44} {:>9.1} {:>9.1} {:>9.1} {:>9.1} {:>9.1}",
            src, row[0], row[1], row[2], row[3], row[4]
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--bench") {
        bench();
        return;
    }
    if args.len() < 3 {
        eprintln!("usage: interval EXPR LO HI [deg|grad]   or   interval --bench");
        std::process::exit(2);
    }
    let lo: f64 = args[1].parse().expect("LO");
    let hi: f64 = args[2].parse().expect("HI");
    let unit = match args.get(3).map(String::as_str) {
        Some("deg") => TrigUnit::Degrees,
        Some("grad") => TrigUnit::Grads,
        _ => TrigUnit::Radians,
    };
    print(&args[0], lo, hi, unit);
}
