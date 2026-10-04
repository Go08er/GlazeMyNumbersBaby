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
    /// None: the certifier gave no analysis (not a function of x, …);
    /// Some(Err): the replay rejected the certificate outright.
    report: Option<Result<Report, String>>,
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
    let report = a.ok().map(|a| {
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
    let want = fs.len();
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
    let mut tally: BTreeMap<(&str, Class), usize> = BTreeMap::new();
    let mut certificates = 0;
    // The certifier's trees: identical to f's exactly, or agreeing at points.
    let mut tree_tally: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows_checked = 0;
    let (mut cms, mut rms) = (0.0, 0.0);
    for (_, d) in &done {
        cms += d.certify_ms;
        rms += d.replay_ms;
        let label = format!("{} [{:?}]", d.src, d.unit);
        // A certificate the replay can't read (an unknown claim kind, row
        // or tree) is a failure, not a skip: the replay must keep up.
        let r = match &d.report {
            None => {
                println!("{label}: not certified");
                continue;
            }
            Some(Ok(r)) => r,
            Some(Err(e)) => {
                fails.push(format!("{label}: rejected: {e}"));
                continue;
            }
        };
        certificates += 1;
        for t in &r.trees {
            let (what, how) = t.split_once(": ").unwrap_or((t, ""));
            let how = if how.starts_with("identical") {
                "identical exactly"
            } else {
                "agrees at points"
            };
            *tree_tally.entry(format!("{what}: {how}")).or_insert(0) += 1;
        }
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
    for (t, n) in &tree_tally {
        println!("  {t}: {n}");
    }
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
    // Most functions certified and replayed: a run that replays little
    // checks little.
    if filter.is_none() && certificates * 4 < want * 3 {
        fails.push(format!("only {certificates} of {want} functions replayed"));
    }
    for f in &fails {
        println!("FAIL {f}");
    }
    fails
}

#[test]
fn corpus_certificates_replay() {
    let fs = corpus();
    // The corpus read whole (its REVIEW table parsed by layout).
    assert!(
        review().len() >= 80,
        "only {} REVIEW functions read",
        review().len()
    );
    assert!(fs.len() >= 140, "only {} corpus functions", fs.len());
    let fails = run(fs);
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
        .unwrap_or_else(|| {
            panic!(
                "the plant needs a {kind} claim in {row}, which the certifier no longer \
                 writes for this function: retarget the plant"
            )
        })
        .get_mut(kind)
        .unwrap()
}

type Plant = (&'static str, &'static str, fn(&mut serde_json::Value));

/// ln(x)²'s right tail ruled out of lines by f′ monotone far out (f″ < 0
/// from 16) and its value at 10³⁰⁰ (or `value` instead) within 10⁻¹² of
/// 0, as the certifier writes slope evidence; `mono`: with f″'s sign.
fn slope_claims(mono: bool, value: Option<(f64, f64)>) -> Vec<serde_json::Value> {
    let (lo, hi) = value.unwrap_or((1.3815510557964267e-297, 1.3815510557964282e-297));
    let mut out = Vec::new();
    if mono {
        out.push(
            serde_json::json!({"TailBeyond": {"side": "Right", "from": 16.0,
            "of": {"F": "F2"}, "c": 0.0, "above": false}}),
        );
    }
    out.push(serde_json::json!({"Value": {"x": {"a": 1e300, "b": 1e300},
        "of": {"F": "F1"}, "lo": lo, "hi": hi}}));
    out.push(
        serde_json::json!({"TailBeyond": {"side": "Right", "from": 1e300,
        "of": {"F": "F1"}, "c": -5e-324, "above": true}}),
    );
    out.push(serde_json::json!({"Simplifier": {"fact":
        "f/x → 0 as x → +∞ and f has no horizontal asymptote there"}}));
    out
}

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
    ("1/x", "a gap's clearness dropped", |v| {
        claims_of(v, "x_intercepts").retain(|c| c.get("GapClear").is_none());
    }),
    ("1/x", "a gap widened past its clearness", |v| {
        let g = first(v, "x_intercepts", "Gap");
        g["x"]["a"] = serde_json::json!(-1e-300);
    }),
    (
        "sqrt(x)",
        "a gap's clearness for f″ given to f′'s row",
        |v| {
            let c = first(v, "extrema", "GapClear");
            c["of"] = serde_json::json!({"F": "F2"});
        },
    ),
    ("1/x", "the range's gap clearness dropped", |v| {
        claims_of(v, "range").retain(|c| c.get("GapClear").is_none());
    }),
    (
        "ln(4-(x/0.001)^2)",
        "ends known to enclosures closed where f is undefined",
        |v| {
            let p = &mut value_of(v, "domain")["pieces"][0];
            p["lo"]["At"]["closed"] = serde_json::json!(true);
            p["hi"]["At"]["closed"] = serde_json::json!(true);
        },
    ),
    (
        "asin(x/0.3)",
        "ends known to enclosures opened where f is defined",
        |v| {
            let p = &mut value_of(v, "domain")["pieces"][0];
            p["lo"]["At"]["closed"] = serde_json::json!(false);
            p["hi"]["At"]["closed"] = serde_json::json!(false);
        },
    ),
    (
        "0^(x-0.1)",
        "0^b closed where b = 0, inside an enclosure",
        |v| {
            value_of(v, "domain")["pieces"][0]["lo"]["At"]["closed"] = serde_json::json!(true);
        },
    ),
    ("0^x", "0^x's domain extended to the line", |v| {
        value_of(v, "domain")["pieces"][0]["lo"] = serde_json::json!("NegInf");
    }),
    ("0^(x-1)", "0^(x−1) closed at 1", |v| {
        value_of(v, "domain")["pieces"][0]["lo"]["At"]["closed"] = serde_json::json!(true);
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
        f["period"]["a"] = serde_json::json!(std::f64::consts::TAU);
        f["period"]["b"] = serde_json::json!(std::f64::consts::TAU.next_up());
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
    // A limit resting on the simplifier alone, and wrong: (1 + 1/x)^x → e,
    // not 1 (the 1^∞ slip the simplifier once made).
    ("(1+1/x)^x", "a wrong limit of 1 (it is e)", |v| {
        v["horizontal"] = serde_json::json!({"Certified": {
            "value": [{"side": "Right", "y": {"lo": 1.0, "hi": 1.0}, "exact": "1", "looks_like": null}],
            "cert": {"covers": "Line", "claims": [{"Simplifier": {"fact": "f → 1 as x → +∞"}}]}
        }});
    }),
    ("1/(1+e^(-x))", "the simplifier's limit moved", |v| {
        let cs = claims_of(v, "horizontal");
        for c in cs.iter_mut() {
            if c["Simplifier"]["fact"] == "f → 1 as x → +∞" {
                c["Simplifier"]["fact"] = serde_json::json!("f → 2 as x → +∞");
            }
        }
        let y = &mut value_of(v, "horizontal")[1]["y"];
        y["lo"] = serde_json::json!(2.0);
        y["hi"] = serde_json::json!(2.0);
    }),
    ("ln(x)", "a limit where f grows without bound", |v| {
        v["horizontal"] = serde_json::json!({"Certified": {
            "value": [{"side": "Right", "y": {"lo": 50.0, "hi": 50.0}, "exact": "50", "looks_like": null}],
            "cert": {"covers": "Line", "claims": [{"Simplifier": {"fact": "f → 50 as x → +∞"}}]}
        }});
    }),
    // No line: f″ away from 0 on a tail (|f/x| → ∞), and f′ monotone,
    // its values far out within 10⁻¹² of 0 (f/x → 0).
    ("x*abs(x)", "f″'s bound on a tail flipped", |v| {
        let t = first(v, "oblique", "TailBeyond");
        t["above"] = serde_json::json!(!t["above"].as_bool().unwrap());
    }),
    ("x*abs(x)", "f″'s bound given to f′", |v| {
        for c in claims_of(v, "oblique") {
            c["TailBeyond"]["of"] = serde_json::json!({"F": "F1"});
        }
    }),
    // (ln(x)²'s own certificate rules its line out by structure: the slope
    // evidence written out as the certifier writes it where structure
    // doesn't decide.)
    ("ln(x)^2", "f′'s value far out moved off 0", |v| {
        *claims_of(v, "oblique") = slope_claims(true, Some((1e-3, 2e-3)));
    }),
    ("ln(x)^2", "f′'s monotonicity dropped", |v| {
        *claims_of(v, "oblique") = slope_claims(false, None);
    }),
    // Limits by structure.
    ("x^-0.0001", "a slow limit moved", |v| {
        for c in claims_of(v, "horizontal").iter_mut() {
            if let Some(l) = c.get_mut("Limit") {
                l["to"] = serde_json::json!({"In": {"lo": 0.5, "hi": 0.5}});
            }
        }
        let y = &mut value_of(v, "horizontal")[0]["y"];
        y["lo"] = serde_json::json!(0.5);
        y["hi"] = serde_json::json!(0.5);
    }),
    ("1/ln(x)", "a limit at a tail called infinite", |v| {
        first(v, "horizontal", "Limit")["to"] = serde_json::json!("PosInf");
    }),
    (
        "1/ln(x)",
        "a one-sided limit taken from the other side",
        |v| {
            for c in claims_of(v, "range").iter_mut() {
                if let Some(l) = c.get_mut("Limit")
                    && let Some(p) = l["at"].get("Right").cloned()
                {
                    l["at"] = serde_json::json!({ "Left": p });
                }
            }
        },
    ),
    (
        "sqrt(x^2+1)-x",
        "a limit by structure where the tree is ∞ − ∞",
        |v| {
            v["horizontal"] = serde_json::json!({"Certified": {
                "value": [{"side": "Right", "y": {"lo": 0.0, "hi": 0.0}, "exact": null, "looks_like": null}],
                "cert": {"covers": "Line", "claims": [{"Limit": {"at": "PosInf", "over_x": false, "to": {"In": {"lo": 0.0, "hi": 0.0}}}}]}
            }});
        },
    ),
    // A line's graph is the line itself: no asymptote.
    ("5", "a constant's own line listed as its asymptote", |v| {
        v["horizontal"] = serde_json::json!({"Certified": {
            "value": [{"side": "Right", "y": {"lo": 5.0, "hi": 5.0}, "exact": "5", "looks_like": null}],
            "cert": {"covers": "Line", "claims": [{"Limit": {"at": "PosInf", "over_x": false, "to": {"In": {"lo": 5.0, "hi": 5.0}}}}]}
        }});
    }),
    ("2x+1", "a line listed as its own oblique asymptote", |v| {
        value_of(v, "oblique")
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"side": "Right", "m": {"lo": 2.0, "hi": 2.0}, "b": {"lo": 1.0, "hi": 1.0}}));
    }),
    ("1/x", "the values showing f is no constant dropped", |v| {
        claims_of(v, "horizontal").retain(|c| c.get("Value").is_none());
    }),
    (
        "sqrt(x^2+1)",
        "the values showing f is no line dropped",
        |v| {
            claims_of(v, "oblique").retain(|c| c.get("Value").is_none());
        },
    ),
    // No line at all: f − m·x periodic and not constant.
    ("x-sin(x)", "f − m·x given half its period", |v| {
        for c in claims_of(v, "oblique").iter_mut() {
            if let Some(s) = c.get_mut("Simplifier")
                && let Some(f) = s["fact"].as_str()
            {
                s["fact"] = serde_json::json!(f.replace("has period 2·π", "has period 1·π"));
            }
        }
    }),
    (
        "x-sin(x)",
        "the values showing f − m·x not constant dropped",
        |v| {
            claims_of(v, "oblique").retain(|c| c.get("Value").is_none());
        },
    ),
    // Lines from expansions in 1/x.
    ("sqrt(x^2+1)", "a line's slope moved by 10⁻⁹", |v| {
        for c in claims_of(v, "oblique").iter_mut() {
            if let Some(l) = c.get_mut("TailLine") {
                for k in ["lo", "hi"] {
                    let m = l["m"][k].as_f64().unwrap();
                    l["m"][k] = serde_json::json!(m * (1.0 + 1e-9));
                }
            }
        }
        for o in value_of(v, "oblique").as_array_mut().unwrap() {
            for k in ["lo", "hi"] {
                let m = o["m"][k].as_f64().unwrap();
                o["m"][k] = serde_json::json!(m * (1.0 + 1e-9));
            }
        }
    }),
    ("x*atan(x)", "a line's band moved off 0", |v| {
        let l = first(v, "oblique", "TailLine");
        l["lo"] = serde_json::json!(1.0);
        l["hi"] = serde_json::json!(2.0);
    }),
    ("x*e^(1/x)", "a line's tail started too near", |v| {
        let l = first(v, "oblique", "TailLine");
        let s = l["from"].as_f64().unwrap().signum();
        l["from"] = serde_json::json!(s * 1e-3);
    }),
    (
        "x*atan(x)",
        "a line's expansion taken on the other tail",
        |v| {
            for c in claims_of(v, "oblique").iter_mut() {
                if let Some(l) = c.get_mut("TailLine")
                    && l["side"] == "Right"
                {
                    l["side"] = serde_json::json!("Left");
                    l["from"] = serde_json::json!(-l["from"].as_f64().unwrap());
                }
            }
        },
    ),
    ("sqrt(x^2+1)", "a line's expansion dropped", |v| {
        claims_of(v, "oblique").retain(|c| c.get("TailLine").is_none());
    }),
    ("x/ln(x)", "f/x's limit called infinite", |v| {
        first(v, "oblique", "Limit")["to"] = serde_json::json!("PosInf");
    }),
    ("x/ln(x)", "f/x's limit dropped", |v| {
        claims_of(v, "oblique").retain(|c| c.get("Limit").is_none());
    }),
    ("abs(x-3)", "a kink's slope flipped on one side", |v| {
        let k = first(v, "extrema", "KinkAt");
        k["left"] = serde_json::json!(!k["left"].as_bool().unwrap());
    }),
    ("abs(x-3)", "the kink placed a double off", |v| {
        let k = first(v, "extrema", "KinkAt");
        k["at"] = serde_json::json!(3.0f64.next_up());
        k["x"]["b"] = serde_json::json!(3.0f64.next_up().next_up());
    }),
    ("abs(x-3)", "the minimum at a kink moved", |v| {
        let x = &mut value_of(v, "extrema")[0]["x"];
        x["lo"] = serde_json::json!(3.0f64.next_up());
        x["hi"] = serde_json::json!(3.0f64.next_up());
    }),
];

/// The slope evidence the plants above alter, unaltered, replays.
#[test]
fn written_slope_evidence_replays() {
    let mut v = certificate("ln(x)^2");
    *claims_of(&mut v, "oblique") = slope_claims(true, None);
    assert_eq!(rejected(&v), None);
}

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
const KNOWN_ISSUES: &[(u32, &str, &str)] = &[];

/// The issues fixed, by their reproducers: none may come back.
const FIXED_ISSUES: &[(u32, &str, &str)] = &[
    // 1: a Value claim over a neighbourhood of an excluded point said f is
    // valid there (now Bounded: wherever f is defined).
    (1, "x/ln(x)", "vertical"),
    (1, "0/x", "vertical"),
    (1, "atan(1/x)", "vertical"),
    (1, "x^x", "vertical"),
    // 2: f″ claimed on the whole line for |x| (now cut at the kink).
    (2, "abs(x)", "inflections"),
    (2, "10^-12*abs(x)", "inflections"),
    // 3: f⁽ᵏ⁾'s clearness in the gaps was not in the certificate (now
    // GapClear).
    (3, "1/x", "x_intercepts"),
    (3, "sqrt(x)", "extrema"),
    (3, "tan(x)", "monotonicity"),
    (3, "x/ln(x)", "inflections"),
    // 4: a gap thousands of doubles wide (now the enclosure of a family
    // member, which the spec allows).
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
                    .chain(&x.problems)
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
        let r = match d.report {
            Some(Ok(r)) => r,
            Some(Err(e)) => panic!("{src}: rejected: {e}"),
            None => panic!("{src}: not certified"),
        };
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

#[test]
fn fixed_certifier_issues_stay_fixed() {
    let mut back = Vec::new();
    for (issue, src, row) in FIXED_ISSUES {
        let d = one(src, TrigUnit::Radians);
        let r = match d.report {
            Some(Ok(r)) => r,
            Some(Err(e)) => panic!("{src}: rejected: {e}"),
            None => panic!("{src}: not certified"),
        };
        if issue_reproduces(*issue, &r, row) {
            back.push(format!("issue {issue}: {src} ({row})"));
        }
    }
    assert!(back.is_empty(), "fixed issues back: {back:#?}");
}

/// Kinks (replay issue 2): f′ and f″ are claimed only off them, each kink
/// a box f is continuous on; every claim replays (strong, or weak only for
/// the simplifier's own facts) and every row follows from its claims.
#[test]
fn kinks_replay() {
    let mut fails = Vec::new();
    for src in [
        "abs(x)",
        "10^-12*abs(x)",
        "abs(x-3)",
        "x*abs(x)",
        "abs(x^2-1)",
        "max(x,0)",
        "min(x^2,1)",
        "abs(sin(x))",
    ] {
        for unit in [TrigUnit::Radians, TrigUnit::Degrees] {
            let d = one(src, unit);
            let Some(Ok(r)) = d.report else {
                fails.push(format!("{src} [{unit:?}]: no report"));
                continue;
            };
            for c in &r.claims {
                let simplifier = matches!(c.claim, replay::Claim::Simplifier(_));
                if !(c.outcome.class == Class::Strong
                    || simplifier && c.outcome.class == Class::Weak)
                {
                    fails.push(format!(
                        "{src} [{unit:?}]: {:?} {:?}: {}",
                        c.outcome.class, c.claim, c.outcome.note
                    ));
                }
            }
            for row in &r.rows {
                for p in &row.problems {
                    fails.push(format!("{src} [{unit:?}]: {}: {p}", row.name));
                }
            }
        }
    }
    assert!(fails.is_empty(), "{fails:#?}");
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
