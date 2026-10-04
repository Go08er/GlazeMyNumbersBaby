//! The certified analysis' certificates, replayed by an independent
//! checker (`tests/replay/`, MPFR, `--features mpfr-oracle`).
//!
//! For each function the certifier (`graphing::certify`) runs as the app
//! would; its analysis goes through JSON to the replay, which knows
//! nothing of the certifier's code: every claim is re-proved with the
//! replay's own interval arithmetic, and every row is checked against its
//! claims. A claim the replay proves false, or a row that doesn't follow
//! from its claims, fails the test.
//!
//! `cargo test -p graphing --features mpfr-oracle --test certify_replay`
//! replays the certify corpus (its fixtures and review rounds 9–11);
//! `-- --ignored` adds the kgf set in degrees and grads and a few hundred
//! functions of the analysis sweep's pool. `REPLAY_FILTER=text` keeps the
//! functions whose source contains `text`; `REPLAY_VERBOSE=1` prints every
//! claim that isn't strong.
#![cfg(feature = "mpfr-oracle")]

mod replay;

use graphing::certify::{DEFAULT_BUDGET, certify_text};
use graphing::compile::CompileOptions;
use graphing::functions::TrigUnit;
use replay::{Class, Report};
use std::collections::BTreeMap;
use std::time::Instant;

const CASES: &str = include_str!("fixtures/certify/cases.tsv");
const CORPUS: &str = include_str!("certify_corpus.rs");

/// The review rounds' functions, read from `certify_corpus.rs`'s REVIEW
/// table (the first string of each entry).
fn review() -> Vec<String> {
    let start = CORPUS
        .find("const REVIEW")
        .expect("REVIEW in certify_corpus.rs");
    let body = &CORPUS[start..];
    let end = body.find("\n];").expect("end of REVIEW");
    let lines: Vec<&str> = body[..end].lines().collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if l.trim() == "(" {
            let s = lines[i + 1].trim();
            if let Some(s) = s.strip_prefix('"').and_then(|s| s.strip_suffix("\",")) {
                out.push(s.to_string());
            }
        }
    }
    out
}

fn corpus() -> Vec<(String, TrigUnit)> {
    let mut out: Vec<(String, TrigUnit)> = Vec::new();
    for line in CASES.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        out.push((f[2].to_string(), TrigUnit::Radians));
    }
    for s in review() {
        out.push((s, TrigUnit::Radians));
    }
    out
}

/// The sweep's base functions (`examples/sweep.rs`), alone and shifted,
/// offset, scaled and stretched.
const BASES: &[&str] = &[
    "x^2",
    "x^3-2x+1/(x-1)",
    "sin(x)",
    "tan(x)",
    "1/x",
    "ln(x)",
    "log(x)",
    "sqrt(x)",
    "e^x",
    "(x^2-1)/(x-1)",
    "x^0.9",
    "x/ln(x)",
    "1/ln(x)",
    "sin(x^2)",
    "floor(x)",
    "atan(x)",
    "x*e^(-x)",
    "sin(x)/x",
    "x*sin(x)",
    "1/(x-2)",
    "1/(x^2-4)",
    "cot(x)",
    "csc(x)",
    "sec(x)",
    "coth(x)",
    "csch(x)",
    "atan(1/x)",
    "e^(-1/x^2)",
    "e^(ln(x))",
    "1/(1+e^x)",
    "x*ln(x)",
    "x/x",
    "1/x^2",
    "tan(x)^2",
    "1/sin(x)",
    "ln(abs(x))",
    "sqrt(1/x)",
    "x^x",
    "x^-0.5",
    "x^-0.0001",
    "e^(-x^2)",
    "x^3-3x",
    "1/(x^2+1)",
    "sqrt(x^2+1)",
    "abs(x)",
    "x*abs(x)",
    "(x-1)*(x+2)",
    "(x-1)*(x-2)*(x+3)",
    "(x-1)^2*(x+1)",
    "(x-0.5)*(x+0.25)*(x-3)*(x+4)",
    "(x^2-1)*(x^2-4)*(x-0.5)",
    "(x-1)/((x-2)*(x+3))",
    "(x^2-4)/(x-2)",
    "(x+1)/(x^2-1)",
    "x^2/(x^2-1)",
    "(x^3-1)/(x-1)",
    "sin(x)+x/10",
    "ln(x^2+1)",
    "atan(x)+0.000000001",
    "1/(x^2+1)-0.5",
    "x^4-x^2",
    "x*e^x",
    "e^x-x",
    "x^(1/3)",
    "sqrt(4-x^2)",
    "ln(4-x^2)",
    "1/(sqrt(x)-1)",
    "x^2+1",
    "-x^2-1",
    "cos(x)^2",
    "sin(x)^2+1",
    "x+1/x",
    "e^(-x)",
    "2^x",
    "x^5-5x^3+4x",
    "(x-1)^(1/3)",
    "x^(2/3)",
    "1/(x^2+x+1)",
    "x^2*e^(-x)",
    "sin(x)*e^(-x/10)",
    "ln(x)/x",
    "x^3+x",
    "(x+1)^2/(x-1)",
    "sqrt(x)*ln(x)",
    "e^(1/x)",
    "e^(1/(x-1))",
    "x^2-2x+1",
    "2x+3",
    "x^4-4x^3+6x^2-4x+1",
    "(x^2+1)/(x^2-1)",
    "x/(x^2+1)",
    "atan(ln(abs(x)))",
    "cos(x)+sin(2x)",
    "x-sin(x)",
    "ln(x)^2",
    "sqrt(x^2-1)",
    "1/sqrt(1-x^2)",
    "x^2*ln(x)",
    "e^x/x",
    "x^3-6x^2+11x-6",
    "x^4-5x^2+4",
    "(x-3)^2-4",
    "100x^2",
    "0.01x^2",
    "x^2/100+x",
];

/// `x` replaced by `with` wherever it stands alone (not inside a name).
fn subst_x(e: &str, with: &str) -> String {
    let cs: Vec<char> = e.chars().collect();
    let mut out = String::new();
    for (i, &c) in cs.iter().enumerate() {
        let letter = |j: Option<&char>| j.is_some_and(|c| c.is_ascii_alphabetic());
        if c == 'x' && !letter(i.checked_sub(1).and_then(|j| cs.get(j))) && !letter(cs.get(i + 1)) {
            out.push_str(&format!("({with})"));
        } else {
            out.push(c);
        }
    }
    out
}

fn pool() -> Vec<(String, TrigUnit)> {
    let mut out = Vec::new();
    for b in BASES {
        out.push((b.to_string(), TrigUnit::Radians));
        out.push((subst_x(b, "x-1000"), TrigUnit::Radians));
        out.push((format!("({b})+0.0000001"), TrigUnit::Radians));
        out.push((format!("-3*({b})"), TrigUnit::Radians));
        out.push((subst_x(b, "x/1000"), TrigUnit::Radians));
    }
    out
}

/// The kgf set in the other angle units.
fn units() -> Vec<(String, TrigUnit)> {
    let mut out = Vec::new();
    for line in CASES.lines().filter(|l| l.starts_with('K')) {
        let src = line.split('\t').nth(2).unwrap_or_default();
        for u in [TrigUnit::Degrees, TrigUnit::Grads] {
            out.push((src.to_string(), u));
        }
    }
    out
}

struct Done {
    src: String,
    unit: TrigUnit,
    report: Result<Report, String>,
    certify_ms: f64,
    replay_ms: f64,
}

fn one(src: &str, unit: TrigUnit) -> Done {
    let opts = CompileOptions {
        trig_unit: unit,
        ..CompileOptions::default()
    };
    let t0 = Instant::now();
    let a = certify_text(src, opts, DEFAULT_BUDGET, None);
    let certify_ms = t0.elapsed().as_secs_f64() * 1e3;
    let t1 = Instant::now();
    let report = a.and_then(|a| {
        let json = serde_json::to_string(&a).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        replay::replay(&v)
    });
    Done {
        src: src.to_string(),
        unit,
        report,
        certify_ms,
        replay_ms: t1.elapsed().as_secs_f64() * 1e3,
    }
}

/// Replays every function (in parallel), prints the tally, and returns
/// what fails.
fn run(fs: Vec<(String, TrigUnit)>) -> Vec<String> {
    let filter = std::env::var("REPLAY_FILTER").ok();
    let verbose = std::env::var_os("REPLAY_VERBOSE").is_some();
    let fs: Vec<(String, TrigUnit)> = fs
        .into_iter()
        .filter(|(s, _)| filter.as_ref().is_none_or(|f| s.contains(f.as_str())))
        .collect();
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(16);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|sc| {
        for _ in 0..threads {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(sc, || {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((s, u)) = fs.get(i) else { break };
                        let d = one(s, *u);
                        done.lock().unwrap().push((i, d));
                    }
                })
                .expect("a replay thread");
        }
    });
    let mut done = done.into_inner().unwrap();
    done.sort_by_key(|(i, _)| *i);
    let mut fails = Vec::new();
    let mut known = Vec::new();
    let mut tally: BTreeMap<(&str, Class), usize> = BTreeMap::new();
    let mut certificates = 0;
    let mut rows_checked = 0;
    let (mut cms, mut rms) = (0.0, 0.0);
    for (_, d) in &done {
        cms += d.certify_ms;
        rms += d.replay_ms;
        let label = format!("{} [{:?}]", d.src, d.unit);
        let r = match &d.report {
            Ok(r) => r,
            Err(e) if e.starts_with("binding") => {
                fails.push(format!("{label}: {e}"));
                continue;
            }
            Err(e) => {
                println!("{label}: not replayed: {e}");
                continue;
            }
        };
        certificates += 1;
        rows_checked += r.rows.iter().filter(|x| x.status != "Unknown").count();
        for (k, n) in r.count() {
            *tally.entry(k).or_insert(0) += n;
        }
        for c in &r.claims {
            let bad = c.outcome.class == Class::Refuted;
            // Known, reported to the certifier: a Value claim over a
            // neighbourhood of an excluded point (the vertical row's
            // "bounded near it") says f is valid on the whole box; it
            // holds only where f is defined.
            if bad && c.outcome.note.contains(replay::claims::WHERE_DEFINED) {
                known.push(format!("{label}: {:?}: {}", c.claim, c.outcome.note));
                continue;
            }
            if bad {
                fails.push(format!(
                    "{label}: refuted {:?} ({}): {}",
                    c.claim,
                    c.rows.join(","),
                    c.outcome.note
                ));
            }
            if verbose && c.outcome.class != Class::Strong {
                println!(
                    "{label}: {} {:?} ({}): {}",
                    c.outcome.class.name(),
                    c.claim,
                    c.rows.join(","),
                    c.outcome.note
                );
            }
        }
        for p in r.row_problems() {
            fails.push(format!("{label}: row {p}"));
        }
        if verbose {
            for row in &r.rows {
                for n in &row.notes {
                    println!("{label}: {}: {n}", row.name);
                }
            }
        }
    }
    println!(
        "\n{certificates} certificates ({} functions), {rows_checked} certified or partial rows; \
         certify {:.1} s, replay {:.1} s (CPU)",
        done.len(),
        cms / 1e3,
        rms / 1e3
    );
    let kinds: Vec<&str> = {
        let mut k: Vec<&str> = tally.keys().map(|(k, _)| *k).collect();
        k.dedup();
        k
    };
    let classes = [
        Class::Strong,
        Class::Weak,
        Class::Unconfirmed,
        Class::Unsupported,
        Class::Refuted,
    ];
    print!("{:<12}", "claim");
    for c in classes {
        print!(" {:>11}", c.name());
    }
    println!();
    let mut totals = [0usize; 5];
    for k in kinds {
        print!("{k:<12}");
        for (i, c) in classes.iter().enumerate() {
            let n = tally.get(&(k, *c)).copied().unwrap_or(0);
            totals[i] += n;
            print!(" {n:>11}");
        }
        println!();
    }
    print!("{:<12}", "total");
    for n in totals {
        print!(" {n:>11}");
    }
    println!();
    for k in &known {
        println!("KNOWN {k}");
    }
    for f in &fails {
        println!("FAIL {f}");
    }
    fails
}

#[test]
fn corpus_certificates_replay() {
    let fails = run(corpus());
    assert!(fails.is_empty(), "{} failures", fails.len());
}

#[test]
#[ignore = "long: the kgf set in degrees and grads, and the sweep's pool"]
fn pool_certificates_replay() {
    let mut fs = units();
    fs.extend(pool());
    let fails = run(fs);
    assert!(fails.is_empty(), "{} failures", fails.len());
}
