//! Replays the certificate of one function claim by claim, with the
//! independent checker of `tests/replay/` (MPFR; dev only).
//!
//! ```text
//! cargo run --release -p graphing --features mpfr-oracle --example replay -- 'x^3-2x+1/(x-1)' [deg|grad] [--all] [--lenient] [--json FILE]
//! ```
//!
//! Prints each claim the replay doesn't prove strongly (every claim with
//! `--all`), its outcome and why, then each row's problems and notes, the
//! certifier's trees against f's, and a tally. `--json FILE` replays a
//! certificate saved as JSON instead of certifying the expression.
//!
//! Exits with status 0 only if the certificate replays in full, as the
//! strict corpus test (`tests/certify_replay.rs`) requires: status 1 if
//! the replay rejects the certificate, a claim is refuted, a row doesn't
//! follow from its claims or a certifier's tree is shown wrong, and also
//! if a claim behind a Certified or Partial row is left open, neither
//! proved nor refuted (unconfirmed) or about something the replay doesn't
//! model (unsupported). `--lenient` keeps the old rule (review 17,
//! Question 2): open claims are noted, not failures, so its status 0 says
//! only that nothing was shown wrong, not that every claim was confirmed.
//!
//! What the replay can't check: functions that use the factorial `n!`, the
//! double factorial `n!!`, `nCr` or `nPr` anywhere, a constant like `5!`
//! included. It has no Γ of its own for them, so every claim about such a
//! function is unsupported and the strict status is 1 for its certificate,
//! genuine or not (`nCr(x,1)`: 37 claims, all unsupported; review 18,
//! Question 1): not checked, rather than checked and wrong. Everything
//! else in the language is modelled (`tests/replay/mod.rs` has the scope).

#[path = "../tests/replay/mod.rs"]
mod replay;

use graphing::certify::{DEFAULT_BUDGET, certify_text};
use graphing::compile::CompileOptions;
use graphing::functions::TrigUnit;
use replay::Class;

const USAGE: &str = "\
usage: replay 'EXPR' [deg|grad] [--all] [--lenient] [--json FILE]

Replays the certificate of y = EXPR (or the one saved in FILE) claim by
claim. Status 0: every claim behind a certified or partial row is proved
and every row follows; 1: a claim is refuted or left open, or a row
doesn't follow (--lenient passes open claims); 2: nothing to replay.

Not checked: functions that use n!, n!!, nCr or nPr anywhere (a constant
like 5! included). The replay has no gamma function of its own, so every
claim about such a function is unsupported and its certificate gets status
1, genuine or not: not checked, rather than checked and wrong.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return;
    }
    let all = args.iter().any(|a| a == "--all");
    let lenient = args.iter().any(|a| a == "--lenient");
    let unit = if args.iter().any(|a| a == "deg") {
        TrigUnit::Degrees
    } else if args.iter().any(|a| a == "grad") {
        TrigUnit::Grads
    } else {
        TrigUnit::Radians
    };
    let json_file = args
        .iter()
        .position(|a| a == "--json")
        .and_then(|i| args.get(i + 1));
    let value: serde_json::Value = match json_file {
        Some(path) => {
            let text = std::fs::read_to_string(path).expect("a readable certificate");
            serde_json::from_str(&text).expect("a JSON certificate")
        }
        None => {
            let Some(src) = args
                .iter()
                .find(|a| !a.starts_with("--") && *a != "deg" && *a != "grad")
            else {
                eprintln!("{USAGE}");
                std::process::exit(2);
            };
            let opts = CompileOptions {
                trig_unit: unit,
                ..CompileOptions::default()
            };
            let a = certify_text(src, opts, DEFAULT_BUDGET, None).unwrap_or_else(|e| {
                eprintln!("not certified: {e}");
                std::process::exit(2);
            });
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap()
        }
    };
    let r = match replay::replay(&value) {
        Ok(r) => r,
        Err(e) => {
            println!("rejected: {e}");
            std::process::exit(1);
        }
    };
    println!("{}", r.source);
    for b in &r.binding {
        println!("  binding: {b}");
    }
    for t in &r.trees {
        println!("  {t}");
    }
    let mut bad = false;
    // Claims behind a Certified or Partial row left open.
    let mut open = 0;
    for c in &r.claims {
        if c.outcome.class == Class::Refuted {
            bad = true;
        }
        let certified = c.rows.iter().any(|name| {
            r.rows.iter().any(|row| {
                row.name == *name && matches!(row.status.as_str(), "Certified" | "Partial")
            })
        });
        if certified && matches!(c.outcome.class, Class::Unconfirmed | Class::Unsupported) {
            open += 1;
        }
        if all || c.outcome.class != Class::Strong {
            println!(
                "  {:<11} {:?}  [{}]{}",
                c.outcome.class.name(),
                c.claim,
                c.rows.join(", "),
                if c.outcome.note.is_empty() {
                    String::new()
                } else {
                    format!("\n              {}", c.outcome.note)
                }
            );
        }
    }
    for row in &r.rows {
        let mark = if row.problems.is_empty() {
            "ok"
        } else {
            "PROBLEM"
        };
        println!("  row {:<13} {:<9} {mark}", row.name, row.status);
        for p in &row.problems {
            bad = true;
            println!("      problem: {p}");
        }
        for n in &row.notes {
            println!("      note: {n}");
        }
    }
    for p in &r.tree_problems {
        bad = true;
        println!("  PROBLEM {p}");
    }
    let mut tally = std::collections::BTreeMap::new();
    for c in &r.claims {
        *tally.entry(c.outcome.class.name()).or_insert(0) += 1;
    }
    println!("  {} claims: {tally:?}", r.claims.len());
    if open > 0 {
        if lenient {
            println!(
                "  {open} claims behind certified or partial rows are open (unconfirmed or \
                 unsupported): accepted with --lenient"
            );
        } else {
            println!(
                "  FAIL: {open} claims behind certified or partial rows are open (unconfirmed \
                 or unsupported); --lenient accepts them"
            );
            bad = true;
        }
    }
    if bad {
        std::process::exit(1);
    }
}
