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
        // The certifier writes a binding: one missing is a failure.
        for b in &r.binding {
            if b.starts_with("missing") {
                fails.push(format!("{label}: {b}"));
            } else if verbose {
                println!("{label}: binding: {b}");
            }
        }
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

// ------------------------------------------------------------ self-test

/// The certificate of `src` as JSON.
fn certificate(src: &str) -> serde_json::Value {
    let a = certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None).expect("certified");
    serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap()
}

/// Whether the replay rejects a certificate: a claim refuted or left
/// unproven, a row that doesn't follow, a broken binding.
fn rejected(v: &serde_json::Value) -> Option<String> {
    match replay::replay(v) {
        Err(e) => Some(e),
        Ok(r) => {
            if let Some(c) = r
                .claims
                .iter()
                .find(|c| matches!(c.outcome.class, Class::Refuted | Class::Unconfirmed))
            {
                return Some(format!(
                    "{} {:?}: {}",
                    c.outcome.class.name(),
                    c.claim,
                    c.outcome.note
                ));
            }
            r.row_problems().into_iter().next()
        }
    }
}

/// The claims of a row (mutable).
fn claims_of<'v>(v: &'v mut serde_json::Value, row: &str) -> &'v mut Vec<serde_json::Value> {
    let r = v[row].as_object_mut().unwrap();
    let body = r.values_mut().next().unwrap();
    body["cert"]["claims"].as_array_mut().unwrap()
}

fn value_of<'v>(v: &'v mut serde_json::Value, row: &str) -> &'v mut serde_json::Value {
    let r = v[row].as_object_mut().unwrap();
    &mut r.values_mut().next().unwrap()["value"]
}

/// The first claim of a kind in a row.
fn first<'v>(v: &'v mut serde_json::Value, row: &str, kind: &str) -> &'v mut serde_json::Value {
    claims_of(v, row)
        .iter_mut()
        .find(|c| c.get(kind).is_some())
        .unwrap_or_else(|| panic!("no {kind} in {row}"))
        .get_mut(kind)
        .unwrap()
}

type Plant = (&'static str, &'static str, fn(&mut serde_json::Value));

/// False facts planted into real certificates: each must be caught.
const PLANTS: &[Plant] = &[
    ("x^3-2x+1/(x-1)", "an x-intercept left out", |v| {
        value_of(v, "x_intercepts")
            .as_array_mut()
            .unwrap()
            .remove(0);
    }),
    ("x^3-2x+1/(x-1)", "an x-intercept made up", |v| {
        value_of(v, "x_intercepts")
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"At": {"lo": 3.5, "hi": 3.5}}));
    }),
    ("x^3-2x+1/(x-1)", "a maximum called a minimum", |v| {
        let e = &mut value_of(v, "extrema")[0]["kind"];
        *e = serde_json::json!(if e == "Max" { "Min" } else { "Max" });
    }),
    ("x^3-2x+1/(x-1)", "a sign flipped", |v| {
        let b = first(v, "x_intercepts", "Beyond");
        b["above"] = serde_json::json!(!b["above"].as_bool().unwrap());
    }),
    ("x^3-2x+1/(x-1)", "a value moved", |v| {
        let c = first(v, "extrema", "Value");
        c["lo"] = serde_json::json!(c["lo"].as_f64().unwrap() + 1.0);
        c["hi"] = serde_json::json!(c["hi"].as_f64().unwrap() + 1.0);
    }),
    ("x^3-2x+1/(x-1)", "a box taken out of the cover", |v| {
        let cs = claims_of(v, "x_intercepts");
        let i = cs.iter().position(|c| c.get("Beyond").is_some()).unwrap();
        cs.remove(i);
    }),
    (
        "x^3-2x+1/(x-1)",
        "a crossing's box moved off its zero",
        |v| {
            let e = first(v, "x_intercepts", "OneCross");
            let a = e["x"]["a"].as_f64().unwrap();
            let b = e["x"]["b"].as_f64().unwrap();
            e["x"]["b"] = serde_json::json!(a + (b - a) * 1e-3);
        },
    ),
    ("x^3-2x+1/(x-1)", "the y-intercept moved", |v| {
        let y = value_of(v, "y_intercept");
        y["lo"] = serde_json::json!(-2.0);
        y["hi"] = serde_json::json!(-2.0);
    }),
    ("x^3-2x+1/(x-1)", "a monotone piece reversed", |v| {
        let d = &mut value_of(v, "monotonicity")[0]["dir"];
        *d = serde_json::json!(if d == "Increasing" {
            "Decreasing"
        } else {
            "Increasing"
        });
    }),
    ("x^3-2x+1/(x-1)", "a vertical asymptote made up", |v| {
        value_of(v, "vertical")
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"At": {"lo": 5.5, "hi": 5.5}}));
    }),
    ("x^3-2x+1/(x-1)", "the pole moved", |v| {
        let u = first(v, "vertical", "Unbounded");
        u["at"] = serde_json::json!({"a": 1.25, "b": 1.25});
        u["near"] = serde_json::json!({"a": 1.2, "b": 1.3});
    }),
    (
        "x^3-2x+1/(x-1)",
        "the excluded point dropped from the domain",
        |v| {
            *value_of(v, "domain") = serde_json::json!({
                "pieces": [{"lo": "NegInf", "hi": "PosInf"}], "excluded": []
            });
        },
    ),
    ("x^3-2x+1/(x-1)", "another formula", |v| {
        v["formula"] = serde_json::json!("Add(Pow(x,3),Div(1,Sub(x,2)))");
    }),
    ("x^3-2x+1/(x-1)", "a tail's sign flipped", |v| {
        let t = first(v, "x_intercepts", "TailChain");
        t["above"] = serde_json::json!(!t["above"].as_bool().unwrap());
    }),
    ("1/x", "a tail's sign flipped", |v| {
        let t = first(v, "x_intercepts", "TailBeyond");
        t["above"] = serde_json::json!(!t["above"].as_bool().unwrap());
    }),
    ("sqrt(x)", "a closed end opened", |v| {
        value_of(v, "domain")["pieces"][0]["lo"]["At"]["closed"] = serde_json::json!(false);
    }),
    ("sqrt(x)", "the domain extended", |v| {
        value_of(v, "domain")["pieces"][0]["lo"] = serde_json::json!("NegInf");
    }),
    ("x^2-1", "parity changed", |v| {
        *value_of(v, "parity") = serde_json::json!("Odd");
    }),
    ("x^2-1", "a value at a point moved", |v| {
        let c = first(v, "y_intercept", "Value");
        c["lo"] = serde_json::json!(0.0);
        c["hi"] = serde_json::json!(0.0);
    }),
    ("x^2-1", "the minimum's value moved", |v| {
        let y = &mut value_of(v, "extrema")[0]["y"];
        y["lo"] = serde_json::json!(0.0);
        y["hi"] = serde_json::json!(0.0);
    }),
    ("sin(x)", "the period halved", |v| {
        let p = value_of(v, "period");
        p["lo"] = serde_json::json!(std::f64::consts::PI);
        p["hi"] = serde_json::json!(std::f64::consts::PI);
    }),
    ("sin(x)", "an inflection left out", |v| {
        value_of(v, "inflections").as_array_mut().unwrap().clear();
    }),
    ("tan(x)", "the excluded family moved", |v| {
        let f = &mut value_of(v, "domain")["excluded"][0]["x0"];
        f["lo"] = serde_json::json!(1.0);
        f["hi"] = serde_json::json!(1.0);
    }),
    ("tan(x)", "the family claim's period doubled", |v| {
        let f = first(v, "domain", "Family");
        f["period"]["a"] = serde_json::json!(6.283185307179586);
        f["period"]["b"] = serde_json::json!(6.283185307179587);
    }),
    ("1/(1+x^2)", "the horizontal asymptote moved", |v| {
        let y = &mut value_of(v, "horizontal")[0]["y"];
        y["lo"] = serde_json::json!(1.0);
        y["hi"] = serde_json::json!(1.0);
    }),
    ("x^2-1", "another power convention", |v| {
        v["binding"]["power"] = serde_json::json!("ieee-pow");
    }),
    ("x^2-1", "another certifier", |v| {
        v["binding"]["certifier"] = serde_json::json!("graphing 0.1.0");
    }),
    ("a*x^2-1", "a slider's value changed", |v| {
        v["binding"]["sliders"]["a"] = serde_json::json!(2.0);
    }),
    ("1/(1+x^2)", "the range's top end moved", |v| {
        let hi = &mut value_of(v, "range")[0]["hi"]["At"]["x"];
        hi["lo"] = serde_json::json!(2.0);
        hi["hi"] = serde_json::json!(2.0);
    }),
];

#[test]
fn replay_catches_planted_errors() {
    let mut missed = Vec::new();
    let mut cache: BTreeMap<&str, serde_json::Value> = BTreeMap::new();
    for (src, what, plant) in PLANTS {
        let base = cache.entry(src).or_insert_with(|| certificate(src)).clone();
        if let Some(why) = rejected(&base) {
            panic!("{src}: the true certificate is rejected: {why}");
        }
        let mut v = base.clone();
        plant(&mut v);
        match rejected(&v) {
            Some(why) => println!("{src}: {what}: caught ({why})"),
            None => missed.push(format!("{src}: {what}")),
        }
    }
    assert!(missed.is_empty(), "planted errors not caught: {missed:#?}");
}

// ------------------------------------------------------------ known issues

/// Certifier issues the replay found (research/replay-issues.md, by
/// number), pinned: (issue, source, row). Each must still reproduce: when
/// the certifier is fixed the test fails, and the entry goes (with the
/// issue's write-up).
const KNOWN_ISSUES: &[(u32, &str, &str)] = &[
    // 1: a Value claim over a neighbourhood of an excluded point says f is
    // valid on the whole box; it holds only where f is defined.
    (1, "x/ln(x)", "vertical"),
    (1, "0/x", "vertical"),
    (1, "atan(1/x)", "vertical"),
    (1, "x^x", "vertical"),
    // 2: f″ claimed on the whole line for |x| (the derivative tree sign(x)
    // hides the kink; f′ doesn't exist at 0): unconfirmed.
    (2, "abs(x)", "inflections"),
    (2, "10^-12*abs(x)", "inflections"),
    // 3: f⁽ᵏ⁾'s clearness in the gaps is not in the certificate.
    (3, "1/x", "x_intercepts"),
    (3, "sqrt(x)", "extrema"),
    // 4: a gap thousands of doubles wide (unconfirmed).
    (4, "tan((x-1000))", "x_intercepts"),
];

fn issue_reproduces(issue: u32, r: &Report, row: &str) -> bool {
    use replay::{Claim, Subject};
    let in_row = |c: &replay::ClaimResult| c.rows.iter().any(|x| x == row);
    match issue {
        1 => r.claims.iter().any(|c| {
            in_row(c)
                && matches!(c.claim, Claim::Value { .. })
                && c.outcome.class == Class::Refuted
                && c.outcome.note.contains(replay::claims::WHERE_DEFINED)
        }),
        2 => r.claims.iter().any(|c| {
            in_row(c)
                && matches!(c.claim, Claim::Value { x, of: Subject::F(2), .. }
                    if x.0 == f64::NEG_INFINITY && x.1 == f64::INFINITY)
                && c.outcome.class == Class::Unconfirmed
        }),
        3 => r.rows.iter().any(|x| {
            x.name == row
                && x.notes
                    .iter()
                    .any(|n| n.contains(replay::rows::GAPS_NOT_IN_CERT))
        }),
        4 => r.claims.iter().any(|c| {
            in_row(c)
                && matches!(c.claim, Claim::Gap(_))
                && c.outcome.class == Class::Unconfirmed
                && c.outcome.note.contains("over 64 doubles")
        }),
        _ => false,
    }
}

#[test]
fn known_certifier_issues_still_reproduce() {
    let mut fixed = Vec::new();
    for (issue, src, row) in KNOWN_ISSUES {
        let d = one(src, TrigUnit::Radians);
        let r = d
            .report
            .unwrap_or_else(|e| panic!("{src}: not replayed: {e}"));
        if !issue_reproduces(*issue, &r, row) {
            fixed.push(format!("issue {issue}: {src} ({row})"));
        }
    }
    assert!(
        fixed.is_empty(),
        "no longer reproduces (fixed?): {fixed:#?}\n\
         Remove these from KNOWN_ISSUES and research/replay-issues.md."
    );
}

/// Sliders at values other than the default, in every angle unit: the
/// replay evaluates them at the binding's values.
#[test]
fn sliders_replay() {
    let vars: BTreeMap<String, f64> = [("a".to_string(), 2.5), ("b".to_string(), -0.75)].into();
    let mut fails = Vec::new();
    for src in ["a*sin(x)+b", "a*x^2+b", "(x-a)/(x+b)", "ln(a*x)+b"] {
        for unit in [TrigUnit::Radians, TrigUnit::Degrees, TrigUnit::Grads] {
            let opts = CompileOptions {
                trig_unit: unit,
                variables: &vars,
            };
            let a = certify_text(src, opts, DEFAULT_BUDGET, None).expect("certified");
            let v: serde_json::Value =
                serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
            assert_eq!(
                v["binding"]["sliders"]["a"],
                serde_json::json!(2.5),
                "{src}"
            );
            if let Some(why) = rejected(&v) {
                fails.push(format!("{src} [{unit:?}]: {why}"));
            }
        }
    }
    assert!(fails.is_empty(), "{fails:#?}");
}
