//! Prints what the simplifier makes of functions of x.
//!
//! `cargo run -p graphing --example simplify -- 'tan(x)*cos(x)' [--deg|--grad]`
//! (no argument: a built-in list).

use std::time::Instant;

use graphing::compile::CompileOptions;
use graphing::simplify::{
    Dir, Settings, limit_at, prove_parity, prove_period, rational_form, simplify,
};
use graphing::{Equation, TrigUnit};

const LIST: &[&str] = &[
    "tan(x)*cos(x)",
    "cos(x)^2/cos(x)^2",
    "ln(exp(1000000000000000)^4)-4000000000000000",
    "acot(1000000000000000)*1000000000000000",
    "(x^2-1)/(x-1)",
    "sin(x)^2+cos(x)^2",
    "x/x",
    "exp(ln(x))",
    "sin(x)+sin(sqrt(2)*x)",
    "x*sin(x)",
    "sin(2x)+cos(3x)",
    "sqrt(x+1)-sqrt(x)",
    "1-cos(x)",
    "x^x",
    "(x-1000000000000)/(x-1000000000000.0625)",
    "atan(x)",
    "1/(1+e^x)",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let unit = if args.iter().any(|a| a == "--deg") {
        TrigUnit::Degrees
    } else if args.iter().any(|a| a == "--grad") {
        TrigUnit::Grads
    } else {
        TrigUnit::Radians
    };
    let inputs: Vec<String> = {
        let given: Vec<String> = args.into_iter().filter(|a| !a.starts_with("--")).collect();
        if given.is_empty() {
            LIST.iter().map(|s| s.to_string()).collect()
        } else {
            given
        }
    };
    let opts = CompileOptions {
        trig_unit: unit,
        variables: &(),
    };
    for s in inputs {
        let text = format!("y={s}");
        let eq = match Equation::parse(&text) {
            Ok(e) => e,
            Err(e) => {
                println!("{s}: {e:?}");
                continue;
            }
        };
        let Some((_, f)) = eq.explicit() else {
            println!("{s}: not a function of x");
            continue;
        };
        let settings = Settings::new(&opts);
        let t = Instant::now();
        let r = match simplify(f, &settings) {
            Ok(r) => r,
            Err(e) => {
                println!("{s}: {e:?}");
                continue;
            }
        };
        let ms = t.elapsed().as_secs_f64() * 1e3;
        println!("{s}");
        println!("  simplified  {}  ({:?}, {ms:.2} ms)", r.expr, r.stop);
        for c in &r.conditions {
            println!("  condition   {c:?}");
        }
        println!("  parity      {:?}", prove_parity(f, &settings));
        if let Some(p) = prove_period(f, &settings) {
            println!("  period      {:?} ≈ {}", p.value, p.to_f64());
        }
        if let Some(rf) = rational_form(f) {
            println!(
                "  rational    {}  (cancelled {})",
                rf.reduced.to_expr(),
                rf.cancelled.to_expr()
            );
            if let Some(h) = rf.reduced.horizontal() {
                println!("  horizontal  y = {h}");
            }
            if let Some((m, b)) = rf.reduced.oblique() {
                println!("  oblique     y = {m}·x + {b}");
            }
        }
        println!(
            "  limits      −∞: {:?}   +∞: {:?}",
            limit_at(f, Dir::NegInf, &settings),
            limit_at(f, Dir::PosInf, &settings)
        );
    }
}
