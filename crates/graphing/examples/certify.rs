//! What the certified analysis proves for a function, row by row.
//!
//! ```text
//! cargo run --release -p graphing --example certify -- 'x^3-2x+1/(x-1)' [deg|grad] [--json] [--digits=N]
//! ```
//!
//! Each row is certified (proven complete), partial (proven items, maybe
//! not all) or unknown, with the number of claims its certificate holds.
//! `--json` prints the whole analysis, certificates included.

use graphing::certify::{
    Analysis, Bound, Enc, ExtKind, Horizontal, Monotone, Piece, Row, Spot, certify_text_with,
};
use graphing::compile::CompileOptions;
use graphing::functions::TrigUnit;
use std::time::Instant;

fn num(v: f64) -> String {
    if v == f64::INFINITY {
        "∞".into()
    } else if v == f64::NEG_INFINITY {
        "−∞".into()
    } else {
        format!("{v}")
    }
}

fn enc(e: &Enc) -> String {
    if e.is_point() {
        num(e.lo.0)
    } else {
        let w = e.hi.0 - e.lo.0;
        if w <= 1e-9 * e.mid().abs().max(1e-300) {
            format!("{}…", num(e.mid()))
        } else {
            format!("[{}, {}]", num(e.lo.0), num(e.hi.0))
        }
    }
}

fn bound(b: &Bound, low: bool) -> String {
    match b {
        Bound::NegInf => "(−∞".into(),
        Bound::PosInf => "∞)".into(),
        Bound::At { x, closed } => {
            if low {
                format!("{}{}", if *closed { "[" } else { "(" }, enc(x))
            } else {
                format!("{}{}", enc(x), if *closed { "]" } else { ")" })
            }
        }
    }
}

fn piece(p: &Piece) -> String {
    format!("{}, {}", bound(&p.lo, true), bound(&p.hi, false))
}

fn spot(s: &Spot) -> String {
    match s {
        Spot::At(e) => enc(e),
        Spot::Every(f) => format!("{} + k·{}", enc(&f.x0), enc(&f.period)),
    }
}

fn row<T>(name: &str, r: &Row<T>, show: impl Fn(&T) -> String) {
    match r {
        Row::Certified { value, cert } => println!(
            "  {name:<13} certified  {}   ({} claims)",
            show(value),
            cert.claims.len()
        ),
        Row::Partial { value, cert } => println!(
            "  {name:<13} partial    {}   ({} claims, covers {:?})",
            show(value),
            cert.claims.len(),
            cert.covers
        ),
        Row::Unknown { reason } => println!("  {name:<13} unknown    ({reason})"),
    }
}

fn list<T>(v: &[T], f: impl Fn(&T) -> String) -> String {
    if v.is_empty() {
        "none".into()
    } else {
        v.iter().map(f).collect::<Vec<_>>().join("; ")
    }
}

fn print(a: &Analysis, ms: f64) {
    println!(
        "y = {}   [{}]   {} evaluation units, {ms:.1} ms",
        a.source, a.unit, a.evals
    );
    if let Some(g) = &a.evaluated {
        println!("  (evaluated as {g})");
    }
    row("domain", &a.domain, |d| {
        let mut s = list(&d.pieces, piece);
        for f in &d.excluded {
            s.push_str(&format!(" minus {} + k·{}", enc(&f.x0), enc(&f.period)));
        }
        s
    });
    row("x-intercepts", &a.x_intercepts, |v| list(v, spot));
    row("y-intercept", &a.y_intercept, |v| match v {
        Some(e) => enc(e),
        None => "none".into(),
    });
    row("parity", &a.parity, |p| format!("{p:?}"));
    row("period", &a.period, |p| match p {
        Some(e) => enc(e),
        None => "not periodic".into(),
    });
    row("extrema", &a.extrema, |v| {
        list(v, |e| {
            format!(
                "{} ({}, {})",
                if e.kind == ExtKind::Max { "max" } else { "min" },
                enc(&e.x),
                enc(&e.y)
            )
        })
    });
    row("inflections", &a.inflections, |v| {
        list(v, |i| format!("({}, {})", enc(&i.x), enc(&i.y)))
    });
    row("monotonicity", &a.monotonicity, |v| {
        list(v, |m: &Monotone| format!("{} {:?}", piece(&m.on), m.dir))
    });
    row("range", &a.range, |v| list(v, piece));
    row("vertical", &a.vertical, |v| list(v, spot));
    row("horizontal", &a.horizontal, |v| {
        list(v, |h: &Horizontal| match (&h.exact, h.looks_like) {
            (Some(x), _) => format!("y = {x} ∈ {} ({:?})", enc(&h.y), h.side),
            (None, Some(y)) => format!("y ∈ {} (looks like {}) ({:?})", enc(&h.y), y.0, h.side),
            (None, None) => format!("y ∈ {} ({:?})", enc(&h.y), h.side),
        })
    });
    row("oblique", &a.oblique, |v| {
        list(v, |o| {
            format!("y = {}·x + {} ({:?})", enc(&o.m), enc(&o.b), o.side)
        })
    });
    if let Some(s) = &a.stopped {
        println!("  stopped: {s}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let unit = if args.iter().any(|a| a == "deg") {
        TrigUnit::Degrees
    } else if args.iter().any(|a| a == "grad") {
        TrigUnit::Grads
    } else {
        TrigUnit::Radians
    };
    let opts = CompileOptions {
        trig_unit: unit,
        ..CompileOptions::default()
    };
    // `--digits=N`: each number typed rounded to N significant digits on
    // entry (the apps' default is 14; recorded in the binding).
    let parse = graphing::lexer::ParseOptions {
        literal_digits: args
            .iter()
            .find_map(|a| a.strip_prefix("--digits="))
            .map(|n| n.parse().expect("--digits=N, N a number of digits")),
        ..Default::default()
    };
    for src in args
        .iter()
        .filter(|a| !a.starts_with("--") && !["deg", "grad"].contains(&a.as_str()))
    {
        let t = Instant::now();
        match certify_text_with(src, parse, opts, graphing::certify::DEFAULT_BUDGET, None) {
            Ok(a) => {
                let ms = t.elapsed().as_secs_f64() * 1e3;
                if json {
                    match serde_json::to_string_pretty(&a) {
                        Ok(s) => println!("{s}"),
                        Err(e) => println!("y = {src}: {e}"),
                    }
                } else {
                    print(&a, ms);
                }
            }
            Err(e) => println!("y = {src}: {e}"),
        }
    }
}
