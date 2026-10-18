//! The certified analysis' certificates, replayed by an independent
//! checker (`tests/replay/`, MPFR, `--features mpfr-oracle`).
//!
//! For each function the certifier (`graphing::certify`) runs as the app
//! would; its analysis goes through JSON to the replay, which shares none
//! of the certifier's code (only graphing's parser and expression tree,
//! for reading the source): every claim is checked with the replay's own
//! MPFR interval arithmetic, and every row against its claims. A claim is
//! *strong* when the replay proves it on the source's own tree, and *weak*
//! when it can only prove it on a tree the certifier supplied (the
//! simplifier's form, the derivatives) or check it at sample points: a
//! weak claim rests on the simplifier being right. A claim the replay
//! proves false, a claim it can neither prove nor refute (unconfirmed) or
//! doesn't model (unsupported), or a row that doesn't follow from its
//! claims fails the test, on the certify corpus and the pool alike: the
//! standalone `replay` example's rule (since review 17, Question 2).
//!
//! `cargo test -p graphing --features mpfr-oracle --test certify_replay`
//! replays the certify corpus (its fixtures and review rounds 9–11);
//! `-- --ignored` adds the kgf set in degrees and grads and a few hundred
//! functions of the analysis sweep's pool. `REPLAY_FILTER=text` keeps the
//! functions whose source contains `text`; `REPLAY_VERBOSE=1` prints every
//! claim that isn't strong.
#![cfg(feature = "mpfr-oracle")]

mod certificates;
mod replay;

use certificates::{CORPUS, corpus, pool, review, units};
use graphing::certify::{DEFAULT_BUDGET, certify_text};
use graphing::compile::CompileOptions;
use graphing::functions::TrigUnit;
use replay::{Class, Report};
use std::collections::BTreeMap;
use std::time::Instant;

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
/// what fails: a claim refuted, unconfirmed or unsupported, a row that
/// doesn't follow, a certifier's tree shown wrong, a missing binding.
fn run(fs: Vec<(String, TrigUnit)>) -> Vec<String> {
    let want = fs.len();
    let filter = std::env::var("REPLAY_FILTER").ok();
    let verbose = std::env::var_os("REPLAY_VERBOSE").is_some();
    let fs: Vec<(String, TrigUnit)> = fs
        .into_iter()
        .filter(|(s, _)| filter.as_ref().is_none_or(|f| s.contains(f.as_str())))
        .collect();
    let threads = certificates::threads();
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
    // The slowest replays, so a regression in the checker's cost shows up.
    let mut slow: Vec<(f64, f64, String)> = done
        .iter()
        .map(|(_, d)| {
            (
                d.replay_ms,
                d.certify_ms,
                format!("{} [{:?}]", d.src, d.unit),
            )
        })
        .collect();
    slow.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (r, c, label) in slow.iter().take(5) {
        println!("slowest: {label}: replay {r:.0} ms, certify {c:.0} ms");
    }
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
            let bad = match c.outcome.class {
                Class::Refuted | Class::Unconfirmed | Class::Unsupported => true,
                Class::Strong | Class::Weak => false,
            };
            if bad {
                fails.push(format!(
                    "{label}: {} {:?} ({}): {}",
                    c.outcome.class.name(),
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
    // Every claim of the corpus proved (none is left open).
    let fails = run(fs);
    assert!(fails.is_empty(), "{} failures", fails.len());
}

#[test]
#[ignore = "long: the kgf set in degrees and grads, and the sweep's pool"]
fn pool_certificates_replay() {
    let mut fs = units();
    fs.extend(pool());
    // Every claim of the pool proved too: the standalone replay exits 0 on
    // each of these genuine certificates (review 17, Question 2).
    let fails = run(fs);
    assert!(fails.is_empty(), "{} failures", fails.len());
}

// ------------------------------------------------------------ self-test

/// The certificate of `src` as JSON.
fn certificate(src: &str) -> serde_json::Value {
    certificate_with(src, None)
}

/// The certificate of `src` with each number typed rounded to `digits`
/// significant digits (`ParseOptions::literal_digits`), as JSON.
fn certificate_with(src: &str, digits: Option<u8>) -> serde_json::Value {
    certificate_in(src, digits, TrigUnit::Radians)
}

/// The same in an angle unit.
fn certificate_in(src: &str, digits: Option<u8>, unit: TrigUnit) -> serde_json::Value {
    let parse = graphing::lexer::ParseOptions {
        literal_digits: digits,
        ..Default::default()
    };
    let opts = CompileOptions {
        trig_unit: unit,
        ..CompileOptions::default()
    };
    let a = graphing::certify::certify_text_with(src, parse, opts, DEFAULT_BUDGET, None)
        .expect("certified");
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

/// The y-intercept row made up as Certified `y`, its one claim f(0) ∈
/// [y, y], over a certificate whose rows are all Unknown (review 17,
/// R17-M-02: so the full checker, not one helper, must refuse it). The
/// certifier's simplified form of f dropped too, as in the review's plants
/// (`keep`: left in).
fn made_up_y_intercept(v: &mut serde_json::Value, y: f64, keep: bool) {
    if !keep {
        v["evaluated"] = serde_json::Value::Null;
    }
    v["y_intercept"] = serde_json::json!({"Certified": {
        "value": {"lo": y, "hi": y},
        "cert": {"covers": "Points", "claims": [{"Value": {
            "x": {"a": 0.0, "b": 0.0}, "of": {"F": "F0"}, "lo": y, "hi": y}}]}
    }});
}

/// The horizontal row made up as Certified, y ∈ `y` at both tails, each
/// limit by the fact "f = N/D exactly: f → `limit` as x → ±∞" (a rational
/// function's limit, which the replay decides exactly), with the producer's
/// own point values of f from the row `values_from` showing f is no
/// constant: nothing else in the certificate changes (review 18, R18-M-02).
fn rational_horizontal(v: &mut serde_json::Value, values_from: &str, y: (f64, f64), limit: &str) {
    let values: Vec<serde_json::Value> = claims_of(v, values_from)
        .iter()
        .filter(|c| c.get("Value").is_some_and(|c| c["of"]["F"] == "F0"))
        .cloned()
        .collect();
    assert!(
        values.len() >= 2,
        "the plant needs two point values of f in {values_from}, which the certifier no longer \
         writes for this function: retarget the plant"
    );
    let item = |side: &str| {
        serde_json::json!({"side": side, "y": {"lo": y.0, "hi": y.1},
        "exact": null, "looks_like": null})
    };
    let mut claims: Vec<serde_json::Value> = ["−∞", "+∞"]
        .iter()
        .map(|at| {
            serde_json::json!({"Simplifier": {"fact":
            format!("f = N/D exactly: f → {limit} as x → {at}")}})
        })
        .collect();
    claims.extend(values);
    v["horizontal"] = serde_json::json!({"Certified": {
        "value": [item("Left"), item("Right")],
        "cert": {"covers": "Line", "claims": claims}
    }});
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
    // A factor exactly 0 inside a gap clears it only where f is undefined
    // there (review 17, Question 2): x/(x² + 1) is 0 at 0.
    ("x/(x^2+1)", "a gap's clearness about a zero", |v| {
        claims_of(v, "x_intercepts").push(serde_json::json!({"GapClear": {
            "x": {"a": -5e-324, "b": 5e-324}, "of": {"F": "F0"}}}));
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
    // A period of 2π·10⁻¹⁰: the minima are no repeat of the maxima.
    (
        "cos(10^10*x)",
        "the minima dropped as repeats of the maxima",
        |v| {
            value_of(v, "extrema")
                .as_array_mut()
                .unwrap()
                .retain(|e| e["kind"] != "Min");
        },
    ),
    // The domain undecided (sign(x) in a divisor): items only inside boxes
    // where f's own tree is shown defined.
    (
        "sin(x)*sign(x)/sign(x)",
        "the boxes f is defined on dropped",
        |v| {
            claims_of(v, "x_intercepts").retain(|c| c.get("Defined").is_none());
        },
    ),
    (
        "sin(x)*sign(x)/sign(x)",
        "an x-intercept where f is undefined, from the simplified form",
        |v| {
            value_of(v, "x_intercepts")
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"At": {"lo": 0.0, "hi": 0.0}}));
            claims_of(v, "x_intercepts").push(serde_json::json!({"ExactAt": {
                "x": {"a": -0.5, "b": 0.5}, "of": {"F": "F0"}, "c": 0.0, "at": 0.0}}));
        },
    ),
    (
        "sin(x)*sign(x)/sign(x)",
        "extrema called complete with the domain undecided",
        |v| {
            let r = v["extrema"].as_object_mut().unwrap();
            let mut body = r.remove("Partial").unwrap();
            body["cert"]["covers"] = serde_json::json!({"Window": {"a": -16.0, "b": 16.0}});
            r.insert("Certified".into(), body);
        },
    ),
    (
        "sin(x)*sign(x)/sign(x)",
        "monotone pieces with the domain undecided",
        |v| {
            v["monotonicity"] = serde_json::json!({"Certified": {
                "value": [{"on": {"lo": "NegInf", "hi": "PosInf"}, "dir": "Increasing"}],
                "cert": {"covers": "Line", "claims": []}}});
        },
    ),
    // Review 15, R15-M-01: a written ratio of 10⁶ or more read with the
    // positive-base rule (x ≥ 0 only), each of its rows on its own.
    ("x^(1000001/3)", "the domain cut to [0, ∞)", |v| {
        value_of(v, "domain")["pieces"][0]["lo"] =
            serde_json::json!({"At": {"x": {"lo": 0.0, "hi": 0.0}, "closed": true}});
    }),
    ("x^(1000001/3)", "odd called neither", |v| {
        *value_of(v, "parity") = serde_json::json!("Neither");
    }),
    ("x^(1000001/3)", "the range cut to [0, ∞)", |v| {
        value_of(v, "range")[0]["lo"] =
            serde_json::json!({"At": {"x": {"lo": 0.0, "hi": 0.0}, "closed": true}});
    }),
    ("x^(1/1000001)", "the domain cut to [0, ∞)", |v| {
        value_of(v, "domain")["pieces"][0]["lo"] =
            serde_json::json!({"At": {"x": {"lo": 0.0, "hi": 0.0}, "closed": true}});
    }),
    ("x^(1000001/3000003)", "odd called even", |v| {
        *value_of(v, "parity") = serde_json::json!("Even");
    }),
    // The follow-up: whole-number exponents of 10⁶ or more.
    ("x^1000001", "the domain cut to [0, ∞)", |v| {
        value_of(v, "domain")["pieces"][0]["lo"] =
            serde_json::json!({"At": {"x": {"lo": 0.0, "hi": 0.0}, "closed": true}});
    }),
    ("x^1000001", "odd called even", |v| {
        *value_of(v, "parity") = serde_json::json!("Even");
    }),
    ("x^(-1000002)", "even called odd", |v| {
        *value_of(v, "parity") = serde_json::json!("Odd");
    }),
    ("x^(-1000002)", "the vertical asymptote left out", |v| {
        value_of(v, "vertical").as_array_mut().unwrap().clear();
    }),
    // Review 17, Question 2: no least period for f′ ≡ 0 on both tails, and
    // a min decided on a tail by its difference's slope.
    (
        "sign(abs(x)+1)",
        "no least period, with f′ ≡ 0 on the left tail only",
        |v| {
            // This row is now Unknown in the producer: keep the exact
            // premise-completeness plant instead of editing a missing cert.
            v["period"] = serde_json::json!({"Certified": {"value": null,
                "cert": {"covers": "Line", "claims": [{"Value": {
                    "x": {"a": "-inf", "b": -1.0}, "of": {"F": "F1"},
                    "lo": 0.0, "hi": 0.0}}]}}});
        },
    ),
    ("min(x,x+1+abs(x-3))", "f′ < 0 on the left tail", |v| {
        // Continuity of the unused corner no longer supplies a complete
        // monotone row; the false tail claim itself must still be caught.
        v["monotonicity"] = serde_json::json!({"Certified": {"value": [{
            "on": {"lo": "NegInf", "hi": "PosInf"}, "dir": "Increasing"}],
            "cert": {"covers": "Line", "claims": [{"TailBeyond": {
                "side": "Left", "from": -16.0, "of": {"F": "F1"}, "c": 0.0,
                "above": false}}]}}});
    }),
    // Review 17, R17-M-02: an operand set aside by a fast path, its
    // definedness or angle unit dropped with it. sin²4 + cos²4 − 1 − 10⁻¹⁰⁰
    // is −10⁻¹⁰⁰, so ⌊√(…)⌋ is defined nowhere, though at 160 bits its
    // enclosure is the point 0; and the language refuses ° outside degrees
    // mode. Each function is defined nowhere (or refused): its certificate
    // has every row Unknown, and a value made up for it must be caught.
    (
        "(x+1)^floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-100)))",
        "a y-intercept for an exponent defined nowhere",
        |v| made_up_y_intercept(v, 1.0, false),
    ),
    (
        "(x+1)^floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-100)))",
        "a y-intercept for an exponent defined nowhere, its simplified form kept",
        |v| made_up_y_intercept(v, 1.0, true),
    ),
    (
        "root(4+x,2+floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-100))))",
        "a y-intercept for a root's degree defined nowhere",
        |v| made_up_y_intercept(v, 2.0, false),
    ),
    (
        "(x+1)^(1°)",
        "a y-intercept for ° outside degrees mode",
        |v| made_up_y_intercept(v, 1.0, false),
    ),
    // The audit's: a non-integer degree whose reciprocal is an integer
    // point (the interval power's own shortcut), ° in a sum (the
    // evaluator's), and a structural rule reading through °.
    (
        "root(4+x,0.5+floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-100))))",
        "a y-intercept for a non-integer root degree defined nowhere",
        |v| made_up_y_intercept(v, 16.0, false),
    ),
    (
        "x+1°",
        "a y-intercept for ° in a sum outside degrees mode",
        |v| made_up_y_intercept(v, 1.0, false),
    ),
    (
        "sin(x°)",
        "odd by structure through ° outside degrees mode",
        |v| {
            v["parity"] = serde_json::json!({"Certified": {"value": "Odd",
            "cert": {"covers": "Line", "claims": [{"Simplifier": {"fact": "f is odd"}}]}}});
        },
    ),
    // Review 18, R18-M-02: a root's degree past u32 cut to its low 32 bits
    // in the exact algebra (2³² + 3 read as 3), so the two roots were one
    // atom and cancelled: f read as the rational function 0, and false
    // limits at ±∞ were proved "exactly". (The cube root grows faster: f →
    // −∞ at +∞ and +∞ at −∞, no horizontal asymptote.) Only the horizontal
    // row is changed, as in the review's variant.
    (
        "root(x+1,4294967299)-root(x+1,3)",
        "a horizontal asymptote y = 0 at both tails, as a rational function's exact limits",
        |v| rational_horizontal(v, "parity", (0.0, 0.0), "0"),
    ),
    // The same reader declared a zero tree supplied as f's simplified form
    // identical to f exactly (any claim proved on it would then stand as
    // proved on f).
    (
        "root(x+1,4294967299)-root(x+1,3)",
        "the zero function supplied as f's simplified form",
        |v| v["evaluated"] = serde_json::json!("f = 0"),
    ),
    // The audit's (the same family: a number narrowed on its way into a
    // proof). A rational limit that is no double, listed as a point: the
    // double a written integer rounds to (2⁵³ + 1 read as 2⁵³) …
    (
        "(9007199254740993x+1)/(x+1)",
        "a horizontal asymptote one below the exact limit 2⁵³ + 1",
        |v| {
            rational_horizontal(
                v,
                "horizontal",
                (9007199254740992.0, 9007199254740992.0),
                "9007199254740993",
            )
        },
    ),
    // … or the double the exact limit rounds to (1/3 as 0.333…3148).
    (
        "(x+1)/(3x)",
        "a horizontal asymptote at the double nearest the exact limit 1/3",
        |v| {
            let d = 1.0 / 3.0;
            rational_horizontal(v, "horizontal", (d, d), &format!("in [{d:?}, {d:?}]"))
        },
    ),
    (
        "(x+1)/(3x)",
        "a horizontal asymptote at the double nearest 1/3, the limit written exactly",
        |v| {
            let d = 1.0 / 3.0;
            rational_horizontal(v, "horizontal", (d, d), "1/3")
        },
    ),
];

/// A plant into a certificate made in an angle unit.
type PlantIn = (
    TrigUnit,
    &'static str,
    &'static str,
    fn(&mut serde_json::Value),
);

/// False facts planted into real certificates made in another angle unit.
const PLANTS_IN: &[PlantIn] = &[
    // Review 17's audit: tan at an even quarter turn is 0, but only where
    // its argument is defined (a meet with that enclosure keeps the better
    // decoration).
    (
        TrigUnit::Degrees,
        "tan(floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-100))))+x+1",
        "a y-intercept for tan of an argument defined nowhere",
        |v| made_up_y_intercept(v, 1.0, false),
    ),
    (
        TrigUnit::Grads,
        "x+1°",
        "a y-intercept for ° in grads",
        |v| made_up_y_intercept(v, 1.0, false),
    ),
    // Review 17, Question 2: 1 − cos x evaluated as 2·sin²(x/2) beside the
    // hole at 0.
    (
        TrigUnit::Degrees,
        "(sin(x))^2/(1-cos(x))",
        "f < 0 beside the hole at 0",
        |v| {
            first(v, "x_intercepts", "Beyond")["above"] = serde_json::json!(false);
        },
    ),
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
    let mut cache: BTreeMap<(&str, String), serde_json::Value> = BTreeMap::new();
    let plants = PLANTS
        .iter()
        .map(|&(src, what, plant)| (TrigUnit::Radians, src, what, plant))
        .chain(PLANTS_IN.iter().copied());
    for (unit, src, what, plant) in plants {
        let base = cache
            .entry((src, format!("{unit:?}")))
            .or_insert_with(|| certificate_in(src, None, unit))
            .clone();
        if let Some(why) = rejected(&base) {
            panic!("{src} [{unit:?}]: the true certificate is rejected: {why}");
        }
        let mut v = base.clone();
        plant(&mut v);
        match rejected(&v) {
            Some(why) => println!("{src} [{unit:?}]: {what}: caught ({why})"),
            None => missed.push(format!("{src} [{unit:?}]: {what}")),
        }
    }
    assert!(missed.is_empty(), "planted errors not caught: {missed:#?}");
}

/// Review 12, R12-M-01: certificates that took a typed decimal for the
/// integer its double is (1.0000000000000001 for 1). The replay passed
/// them strong, making the same rounding; it must refute them on their
/// claims (not refuse them on their binding).
#[test]
fn rounded_literals_are_refuted() {
    let fixture = |text: &str| -> serde_json::Value { serde_json::from_str(text).unwrap() };
    let mut square = certificate("x^2");
    square["source"] = serde_json::json!("y=x^2.0000000000000001");
    let cases = [
        (
            "the unaltered certificate of x^1.0000000000000001 (made at 278be22): ℝ, odd, no minimum",
            fixture(include_str!(
                "fixtures/certify/review12/power-rounded-exponent.json"
            )),
        ),
        (
            "x^2's certificate, its source made x^2.0000000000000001",
            square,
        ),
        (
            "2/(1.0000000000000001-cos(x))'s certificate (made at 278be22), its y-intercept withheld: \
             a domain without 2kπ",
            fixture(include_str!(
                "fixtures/certify/review12/cosine-family-domain-only.json"
            )),
        ),
        // Review 13, R13-M-01: the reviewer's certificates (made at 1bddb79),
        // unaltered. 10^16·(1.0000000000000001 − 1) + x is x + 1 (not odd,
        // its zero −1); 1.0000000000000001·x − 1·x is 10⁻¹⁶·x (not 0).
        (
            "10^16*(1.0000000000000001-1)+x's certificate (made at 1bddb79): odd, \
             0 at 0, the simplifier's form x",
            fixture(include_str!(
                "fixtures/certify/review13/literal-collision.json"
            )),
        ),
        (
            "1.0000000000000001*x-1*x's certificate (made at 1bddb79): 0 everywhere, even",
            fixture(include_str!(
                "fixtures/certify/review13/literal-collision-zero.json"
            )),
        ),
        // Review 14, R14-M-04: the reviewer's certificate (made at dc8ddb5),
        // unaltered. root(−8, −(10³⁰¹ + 1)) is defined at 0 (the degree is
        // odd as typed), not undefined; the replay left that unconfirmed
        // while it read degrees past i64 as unknown.
        (
            "root(-8,-(10^301+1))'s certificate (made at dc8ddb5): no y-intercept",
            fixture(include_str!(
                "fixtures/certify/review14/root-negative-huge.json"
            )),
        ),
        // Review 15, R15-M-01: the reviewer's certificate (made at 80fe9c6,
        // to 14 digits), unaltered. x^(1000001/3) is the real odd root's
        // power, defined on ℝ and odd; it was read with the positive-base
        // rule (domain [0, ∞), neither, a minimum at 0), and the replay's
        // own reading of written ratios shared the cutoff: 18 claims
        // strong.
        (
            "x^(1000001/3)'s certificate (made at 80fe9c6, to 14 digits): domain [0, ∞), \
             neither, minimum (0, 0)",
            fixture(include_str!(
                "fixtures/certify/review15/written-ratio-above.json"
            )),
        ),
    ];
    for (what, v) in cases {
        let r = replay::replay(&v).unwrap_or_else(|e| {
            panic!("{what}: refused, not replayed ({e}): the claims must fail")
        });
        let refuted: Vec<String> = r
            .claims
            .iter()
            .filter(|c| c.outcome.class == Class::Refuted)
            .map(|c| format!("{:?}: {}", c.claim, c.outcome.note))
            .collect();
        assert!(
            !refuted.is_empty() && !r.row_problems().is_empty(),
            "{what}: not refuted"
        );
        println!("{what}: refuted ({})", refuted[0]);
    }
}

/// Review 13: a certificate is bound to the digit limit it was made
/// under, and the replay reads the source so (rounding each number itself,
/// apart from the lexer). 1.0000000000000001·x − 1·x is 0 to 14 digits and
/// 10⁻¹⁶·x off: each certificate replays under its own binding and is
/// refuted under the other; so is x + 1's, made off, bound to 14 digits.
#[test]
fn certificates_are_bound_to_their_digit_limit() {
    let zero = "1.0000000000000001*x-1*x";
    let at14 = certificate_with(zero, Some(14));
    assert_eq!(at14["binding"]["literal_digits"], serde_json::json!(14));
    if let Some(why) = rejected(&at14) {
        panic!("{zero} to 14 digits: {why}");
    }
    let off = certificate_with(zero, None);
    assert_eq!(off["binding"]["literal_digits"], serde_json::Value::Null);
    if let Some(why) = rejected(&off) {
        panic!("{zero} off: {why}");
    }
    let mut swapped = at14.clone();
    swapped["binding"]["literal_digits"] = serde_json::Value::Null;
    let mut first = certificate_with("10^16*(1.0000000000000001-1)+x", None);
    first["binding"]["literal_digits"] = serde_json::json!(14);
    for (what, v) in [
        ("0 everywhere (made to 14 digits), bound to none", swapped),
        ("x + 1 (made off), bound to 14 digits", first),
    ] {
        let r = replay::replay(&v).unwrap_or_else(|e| panic!("{what}: refused ({e})"));
        assert!(
            r.claims.iter().any(|c| c.outcome.class == Class::Refuted)
                && !r.row_problems().is_empty(),
            "{what}: not refuted"
        );
    }
}

/// Review 13: the corpus's functions under a digit limit
/// (`certify_corpus.rs`'s `DIGITS`, sliders aside), and review 12's and
/// 13's literal cases to 14, 15 and 17 digits and off: every claim proven.
#[test]
fn digit_limits_replay() {
    let start = CORPUS
        .find("const DIGITS")
        .expect("DIGITS in certify_corpus.rs");
    let body = &CORPUS[start..];
    let body = &body[..body.find("\n];").expect("end of DIGITS")];
    let lines: Vec<&str> = body.lines().map(str::trim).collect();
    let mut cases: Vec<(String, Option<u8>)> = Vec::new();
    for w in lines.windows(3) {
        if w[0] == "("
            && let Some(src) = w[1].strip_prefix('"').and_then(|s| s.strip_suffix("\","))
            && let Some(d) = w[2].strip_suffix(',').and_then(|d| d.parse().ok())
        {
            cases.push((src.to_string(), Some(d)));
        }
    }
    assert!(cases.len() >= 8, "{cases:?}");
    for src in [
        "10^16*(1.0000000000000001-1)+x",
        "1.0000000000000001*x-1*x",
        "x^1.0000000000000001",
        "1/(x-1.0000000000000001)",
    ] {
        for d in [Some(14), Some(15), Some(17), None] {
            cases.push((src.to_string(), d));
        }
    }
    // (To 15 digits or fewer this is 2/(1 − cos x), whose poles the
    // replay's 160 bits can't resolve within 10⁻⁹⁶ of 0: unconfirmed, a
    // limit of its precision, not of the literals.)
    for d in [Some(17), None] {
        cases.push(("2/(1.0000000000000001-cos(x))".to_string(), d));
    }
    let mut fails = Vec::new();
    for (src, d) in &cases {
        let v = certificate_with(src, *d);
        assert_eq!(v["binding"]["literal_digits"], serde_json::json!(d));
        if let Some(why) = rejected(&v) {
            fails.push(format!("{src} ({d:?} digits): {why}"));
        }
    }
    assert!(fails.is_empty(), "{fails:#?}");
}

/// The replay reads each number of the source's tree as the parser
/// recorded it, checked against its own reading of the text: a tree that
/// took a typed decimal for its double (as every layer did before review
/// 13) is refused.
#[test]
fn a_tree_taking_a_decimal_for_its_double_is_refused() {
    use graphing::ast::{BinOp, Expr, Lit};
    let text = "y=1.0000000000000001*x-1*x";
    let lits = replay::eval::Lits::of(text);
    let eq = graphing::Equation::parse(text).unwrap();
    let tree = eq.explicit().unwrap().1.clone();
    assert_eq!(lits.check(&tree), Ok(()));
    // Both 1s exactly 1.
    let mul = |c: Expr| Expr::bin(BinOp::Mul, c, Expr::X);
    let aliased = Expr::bin(BinOp::Sub, mul(Expr::exact(1.0)), mul(Expr::exact(1.0)));
    assert!(lits.check(&aliased).is_err());
    // A decimal the source doesn't have.
    let made_up = Expr::bin(
        BinOp::Sub,
        mul(Expr::Num(1.0, Lit::Decimal("1.00000000000000011".into()))),
        mul(Expr::exact(1.0)),
    );
    assert!(lits.check(&made_up).is_err());
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

/// Each of `srcs` in radians and degrees: every claim replays strong (or
/// weak, for the simplifier's own facts only) and every row follows from
/// its claims. What doesn't, as failures.
fn all_strong(srcs: &[&str]) -> Vec<String> {
    let mut fails = Vec::new();
    for &src in srcs {
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
    fails
}

/// Kinks (replay issue 2): f′ and f″ are claimed only off them, each kink
/// a box f is continuous on; every claim replays (strong, or weak only for
/// the simplifier's own facts) and every row follows from its claims.
#[test]
fn kinks_replay() {
    let fails = all_strong(&[
        "abs(x)",
        "10^-12*abs(x)",
        "abs(x-3)",
        "x*abs(x)",
        "abs(x^2-1)",
        "max(x,0)",
        "min(x^2,1)",
        "abs(sin(x))",
        // Review 17, Question 2: f = x near 0, a step of a kink beside it
        // (its ExactAt claims at 0 were unconfirmed).
        "min(x,floor(abs(x)+0.5)+5)",
        // f = x, its min or max decided on each tail by the difference's
        // slope: x + 1 + |x − 3| against x on (−∞, 3) is ∞ − ∞, and x − 2
        // against x overlaps on any box over 2 wide (f′ > 0 and f″ ≡ 0 on
        // the tails were unconfirmed).
        "min(x,x+1+abs(x-3))",
        "max(x,x-1-abs(x-3),x-2)",
        // f ≡ 1, f′ ≡ 0 claimed on the two tails beside the kink's box:
        // no least period (the period row didn't follow).
        "sign(abs(x)+1)",
    ]);
    assert!(fails.is_empty(), "{fails:#?}");
}

/// Beside an excluded 0 (review 17, Question 2). 1 − cos u and eᵘ − 1
/// cancel near u = 0 past the working precision (sin²x/(1 − cos x) in
/// degrees: at x = 10⁻¹⁵³, 1 − cos x is 10⁻³¹⁰): the replay evaluates them
/// as 2·sin²(u/2) and 2·e^(u/2)·sinh(u/2), the same functions on the same
/// domain (f > 0 beside the hole, and (eˣ − 1)/(eˣ − 1) ≡ 1 on the tails,
/// were unconfirmed). sin x/(1 − cos x)'s gap about its pole at 0 is clear:
/// its factor sin x is 0 in it only at 0, where f is undefined (the gap's
/// clearness was unconfirmed).
#[test]
fn beside_an_excluded_zero_replays() {
    let fails = all_strong(&[
        "(sin(x))^2/(1-cos(x))",
        "(e^x-1)/(e^x-1)",
        "sin(x)/(1-cos(x))",
    ]);
    assert!(fails.is_empty(), "{fails:#?}");
}

/// Under a digit limit a slider is its value rounded like a number typed
/// (the binding names the decimal; the replay checks the rounding itself).
/// To 14 digits, a·x − x with a set to 1.0000000000000002 is 0 everywhere:
/// its certificate replays, and is refuted when read as the double the
/// slider was set to (2.2·10⁻¹⁶·x). 10^17·(a − 0.3) + x with a at
/// 0.30000000000000004 replays with a the decimal 0.3 (the certifier
/// encloses that subtree term by term, so its claims hold either way); a
/// decimal dropped from its binding, or not the slider's, is refused.
#[test]
fn sliders_are_their_rounded_decimals_under_a_digit_limit() {
    let make = |src: &str, a: f64| -> serde_json::Value {
        let vars: BTreeMap<String, f64> = [("a".to_string(), a)].into();
        let opts = CompileOptions {
            variables: &vars,
            ..CompileOptions::default()
        };
        let parse = graphing::lexer::ParseOptions {
            literal_digits: Some(14),
            ..Default::default()
        };
        let c = graphing::certify::certify_text_with(src, parse, opts, DEFAULT_BUDGET, None)
            .expect("certified");
        serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap()
    };
    let line = make("10^17*(a-0.3)+x", 0.1 + 0.2);
    assert_eq!(line["binding"]["sliders"]["a"], serde_json::json!(0.3));
    assert_eq!(
        line["binding"]["slider_decimals"]["a"],
        serde_json::json!("0.3")
    );
    let zero = make("a*x-x", 1.0000000000000002);
    assert_eq!(zero["binding"]["sliders"]["a"], serde_json::json!(1.0));
    for (what, v) in [("the line", &line), ("a·x − x", &zero)] {
        if let Some(why) = rejected(v) {
            panic!("{what} to 14 digits: {why}");
        }
    }
    // Read as the doubles the sliders were set to, digit limit off.
    let as_double = |v: &serde_json::Value, a: f64| {
        let mut v = v.clone();
        v["binding"]["literal_digits"] = serde_json::Value::Null;
        v["binding"]["sliders"]["a"] = serde_json::json!(a);
        if let Some(b) = v["binding"].as_object_mut() {
            b.remove("slider_decimals");
        }
        v
    };
    let what = "a*x-x at a = 1.0000000000000002";
    let r = replay::replay(&as_double(&zero, 1.0000000000000002))
        .unwrap_or_else(|e| panic!("{what}: refused ({e})"));
    let refuted: Vec<String> = r
        .claims
        .iter()
        .filter(|c| c.outcome.class == Class::Refuted)
        .map(|c| format!("{:?}: {}", c.claim, c.outcome.note))
        .collect();
    assert!(
        !refuted.is_empty() && !r.row_problems().is_empty(),
        "{what}: not refuted"
    );
    println!(
        "{what}, its 14-digit certificate read off: refuted ({})",
        refuted[0]
    );
    let mut dropped = line.clone();
    if let Some(b) = dropped["binding"].as_object_mut() {
        b.remove("slider_decimals");
    }
    let e = replay::replay(&dropped).expect_err("refused");
    assert!(e.contains("not rounded to 14 digits"), "{e}");
    let mut other = line.clone();
    other["binding"]["slider_decimals"]["a"] = serde_json::json!("0.30000000000001");
    let e = replay::replay(&other).expect_err("refused");
    assert!(e.contains("no 14-digit decimal held as"), "{e}");
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

/// The replay's own series of a value constant wherever it is defined
/// (u⁰, a step clear of its jumps), at a point where it is defined but
/// not just left of it, or where its argument jumps (review 16, R16-L-01:
/// ⌊x⌋⁰ at 1 is 0⁰ just left of 1): no first coefficient, so no winner's
/// coefficients for min and max beside it either. Where the argument stays
/// clear beside the box, the derivatives are there: a step's argument
/// with no derivative of its own (|x| at 0) too, when its enclosure over
/// a neighbourhood of the box is continuous and clear of the jumps.
#[test]
fn a_value_alone_has_no_derivatives_at_a_domain_end() {
    replay::iv::init();
    let series = |src: &str, x: f64| {
        let text = format!("y={src}");
        let eq = graphing::Equation::parse(&text).unwrap();
        let (_, e) = eq.explicit().unwrap();
        let lits = replay::eval::Lits::of(&text);
        let ctx = replay::eval::Ctx {
            unit: replay::iv::Unit::Radians,
            lits: &lits,
            vars: &[],
        };
        replay::eval::series(&replay::eval::canonical(e), x, x, 3, &ctx)
    };
    for (src, x) in [
        ("floor(x)^0", 1.0),
        ("min(x,floor(x)^0+100)", 1.0),
        ("max(x,-floor(x)^0-100)", 1.0),
        ("min(floor(x)^0+100,x)", 1.0),
        ("floor(x)^(1-1)", 1.0),
        ("(sqrt(x)+1)^0", 0.0),
        ("min(x,(sqrt(x)+1)^0+100)", 0.0),
        ("floor(sqrt(x)+0.5)", 0.0),
        ("round(sqrt(x))", 0.0),
        ("sign(sqrt(x)+1)", 0.0),
        ("min(x,floor(sqrt(x)+0.5)+5)", 0.0),
        ("floor(floor(x)+0.5)", 1.0),
        ("min(x,2*floor(floor(x)+0.5)-0.5)", 1.0),
    ] {
        let s = series(src, x);
        assert!(!s[0].empty && s[0].def, "{src} at {x}: {:?}", s[0]);
        assert!(!s[1].def, "{src} at {x}: f′ {:?}", s[1]);
    }
    for (src, x, d1) in [
        ("floor(x)^0", 1.5, 0.0),
        ("x^0", 1.0, 0.0),
        ("min(x,floor(x)^0+100)", 1.5, 1.0),
        ("floor(sqrt(x)+0.5)", 1.0, 0.0),
        ("floor(floor(x)+0.5)", 1.5, 0.0),
        ("min(x,floor(x)+5)", 0.5, 1.0),
        // A step's argument with a kink but clear of the jumps beside it.
        ("floor(abs(x)+0.5)", 0.0, 0.0),
        ("min(x,floor(abs(x)+0.5)+5)", 0.0, 1.0),
        ("round(abs(x))", 0.0, 0.0),
        ("sign(abs(x)+1)", 0.0, 0.0),
    ] {
        let s = series(src, x);
        assert!(
            s[1..].iter().all(|c| c.def) && s[1].is_exactly(d1),
            "{src} at {x}: {s:?}"
        );
    }
}

/// Review 17, R17-M-02: an operand a fast path sets aside (an integer
/// power's exponent, a root's degree, tan's argument at an even quarter
/// turn) keeps its definedness, and a degree mark has a value only in
/// degrees mode. ⌊√(sin²4 + cos²4 − 1 − 10⁻¹⁰⁰)⌋ is defined nowhere: at
/// 160 bits its enclosure is the point 0, possibly undefined, so f(0) is
/// not shown defined there; at 640 bits it is shown defined nowhere, and
/// f with it. The same functions with a defined operand keep their values
/// and derivatives.
#[test]
fn operands_set_aside_keep_their_definedness() {
    use replay::iv::Unit::{Degrees, Grads, Radians};
    replay::iv::init();
    let series = |src: &str, unit: replay::iv::Unit, prec: u32| {
        let text = format!("y={src}");
        let eq = graphing::Equation::parse(&text).unwrap();
        let (_, e) = eq.explicit().unwrap();
        let lits = replay::eval::Lits::of(&text);
        let ctx = replay::eval::Ctx {
            unit,
            lits: &lits,
            vars: &[],
        };
        replay::iv::set_prec(prec);
        let s = replay::eval::series(&replay::eval::canonical(e), 0.0, 0.0, 2, &ctx);
        replay::iv::set_prec(160);
        s
    };
    let b = "floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-100)))";
    for (src, unit) in [
        (format!("(x+1)^{b}"), Radians),
        (format!("(x+1)^(1+{b})"), Radians),
        (format!("root(4+x,2+{b})"), Radians),
        (format!("root(4+x,0.5+{b})"), Radians),
        (format!("tan({b})+x+1"), Degrees),
        (format!("tan(200+{b})+x+1"), Grads),
    ] {
        let s = series(&src, unit, 160);
        assert!(s[0].empty || !s[0].def, "{src} at 160 bits: {:?}", s[0]);
        assert!(s[1..].iter().all(|c| !c.def), "{src} at 160 bits: {s:?}");
        let s = series(&src, unit, 640);
        assert!(s[0].empty, "{src} at 640 bits: {:?}", s[0]);
    }
    for (src, unit) in [
        ("(x+1)^(1°)", Radians),
        ("x+1°", Radians),
        ("x+1°", Grads),
        ("sin(x°)", Radians),
    ] {
        assert!(series(src, unit, 160)[0].empty, "{src} [{unit:?}]");
    }
    // Defined operands: the values and derivatives stay.
    for (src, unit, v, d1) in [
        ("(x+1)^floor(sqrt(2))", Radians, 1.0, Some(1.0)),
        ("root(8+x,2+floor(sqrt(2)))", Radians, 2.0, None),
        ("root(4+x,1/floor(sqrt(4)))", Radians, 16.0, Some(8.0)),
        ("tan(floor(sqrt(2))*180)+x+1", Degrees, 1.0, Some(1.0)),
        ("(x+1)^(1°)", Degrees, 1.0, Some(1.0)),
        ("x+1°", Degrees, 1.0, Some(1.0)),
    ] {
        let s = series(src, unit, 160);
        assert!(
            !s[0].empty && s[0].def && s[0].lo <= v && s[0].hi >= v,
            "{src} [{unit:?}]: {:?}",
            s[0]
        );
        assert!(s[1].def, "{src} [{unit:?}]: f′ {:?}", s[1]);
        if let Some(d1) = d1 {
            assert!(s[1].is_exactly(d1), "{src} [{unit:?}]: f′ {:?}", s[1]);
        }
    }
}

// ------------------------------------------------------------ review 18

/// The replay's own reading of `src`: its canonical tree and literals.
fn read(src: &str) -> (graphing::ast::Expr, replay::eval::Lits) {
    let text = format!("y={src}");
    let eq = graphing::Equation::parse(&text).unwrap();
    let (_, e) = eq.explicit().unwrap();
    (replay::eval::canonical(e), replay::eval::Lits::of(&text))
}

/// Review 18, R18-M-02: the exact algebra keeps a root's degree whole. A
/// degree past u32 was cut to its low 32 bits (2³² + 3 read as 3), so
/// ᵏ√u and ∛u were one atom: their difference read as the rational
/// function 0, their trees as identical. Past its own bounds (a monomial's
/// exponent, the atoms it can name) the algebra reads nothing, rather than
/// wrap: x^(2³⁶) − x^(2³⁵) was 0 too in a release build.
#[test]
fn the_exact_algebra_keeps_degrees_and_exponents_whole() {
    use replay::algebra::{Rat, same_trees};
    use replay::iv::Unit::Radians;
    let rational = |src: &str| {
        let (e, lits) = read(src);
        Rat::of(&e, &lits, &[], Radians).map(|r| (r.n.len(), r.d.len()))
    };
    let same = |a: &str, b: &str| {
        let ((ea, lits), (eb, _)) = (read(a), read(b));
        same_trees(&ea, &eb, false, &lits, &[], Radians)
    };
    // Roots of different degrees are different atoms, however long.
    for (a, b) in [
        ("root(x+1,4294967299)", "root(x+1,3)"),
        ("root(x+1,4294967298)", "sqrt(x+1)"),
        ("root(x+1,-4294967299)", "root(x+1,-3)"),
        ("root(x+1,18446744073709551619)", "root(x+1,3)"),
        ("(x+1)^(1/4294967299)", "(x+1)^(1/3)"),
        ("(x+1)^(2/8589934598)", "(x+1)^(1/3)"),
        ("(x+1)^(1/(4294967296+3))", "(x+1)^(1/3)"),
    ] {
        assert!(!same(a, b), "{a} read as {b}");
        assert_eq!(rational(&format!("{a}-({b})")), None, "{a} − {b}");
    }
    // The same degree, however written, is one atom still.
    for (a, b) in [
        ("root(x+1,4294967299)", "root(x+1,4294967299)"),
        ("root(x+1,4294967299)", "(x+1)^(1/4294967299)"),
        ("root(x+1,4294967299)", "(x+1)^(1/(4294967296+3))"),
        ("root(x+1,4294967299)^2", "(x+1)^(2/4294967299)"),
        ("root(x+1,-4294967299)", "1/root(x+1,4294967299)"),
        (
            "root(x+1,18446744073709551619)",
            "(x+1)^(1/18446744073709551619)",
        ),
        ("root(x+1,3)", "cbrt(x+1)"),
        ("root(x+1,3)", "(x+1)^(1/3)"),
        ("root(x+1,2)", "sqrt(x+1)"),
    ] {
        assert!(same(a, b), "{a} not read as {b}");
    }
    assert_eq!(
        rational("root(x+1,4294967299)-(x+1)^(1/4294967299)"),
        Some((0, 1))
    );
    // A monomial's exponent past u32: nothing read (x^(2³⁶) and x^(2³⁵)
    // wrapped to x⁰ both, and their difference to 0).
    let (hi, lo) = (
        "((((((x^64)^64)^64)^64)^64)^64)",
        "((((((x^64)^64)^64)^64)^64)^32)",
    );
    assert_eq!(rational(&format!("{hi}-{lo}")), None);
    assert!(!same(hi, lo));
    // Within the bounds the polynomial is read: (x⁶⁴)⁶⁴ is x⁴⁰⁹⁶.
    assert_eq!(rational("(x^64)^64"), Some((4097, 1)));
}

/// Review 18, R18-L-01: the series of an integer power negated its
/// exponent to take the reciprocal, which i64::MIN has no negation for
/// (an overflow, or in a release build the same exponent again, for ever).
/// A typed −9223372036854775808 is that exponent; the y-intercept of
/// (x + 1) to it is 1.
#[test]
fn the_least_integer_exponent_replays() {
    replay::iv::init();
    for src in [
        "(x+1)^(-9223372036854775808)",
        "(x+1)^(0-9223372036854775808)",
        "(x+1)^(-9223372036854775807)",
        "(x+1)^9223372036854775807",
    ] {
        let (e, lits) = read(src);
        let ctx = replay::eval::Ctx {
            unit: replay::iv::Unit::Radians,
            lits: &lits,
            vars: &[],
        };
        let s = replay::eval::series(&e, 0.0, 0.0, 2, &ctx);
        assert!(s[0].def && s[0].is_exactly(1.0), "{src} at 0: {:?}", s[0]);
        assert!(s[1].def && s[1].bounded(), "{src} at 0: f′ {:?}", s[1]);
        // At −1 the base is 0: a negative power is undefined there.
        let s = replay::eval::series(&e, -1.0, -1.0, 2, &ctx);
        assert_eq!(s[0].empty, src.contains('-'), "{src} at −1: {:?}", s[0]);
    }
    // The certificate itself replays in full: the y-intercept 1 is proved
    // on f's own tree, and a false y-intercept is caught.
    let src = "(x+1)^(-9223372036854775808)";
    let v = certificate(src);
    assert_eq!(rejected(&v), None, "{src}");
    let r = replay::replay(&v).expect("replayed");
    let y = r
        .claims
        .iter()
        .find(|c| c.rows.iter().any(|r| r == "y_intercept"))
        .expect("the y-intercept's claim");
    assert_eq!(y.outcome.class, Class::Strong, "{:?}", y.claim);
    let mut planted = v.clone();
    let c = first(&mut planted, "y_intercept", "Value");
    c["lo"] = serde_json::json!(2.0);
    c["hi"] = serde_json::json!(2.0);
    *value_of(&mut planted, "y_intercept") = serde_json::json!({"lo": 2.0, "hi": 2.0});
    assert!(rejected(&planted).is_some(), "{src}: f(0) = 2 not caught");
}

/// The audit of review 18: tan at a whole number of quarter turns (in
/// degrees and grads) decided its parity from a double cast to i64. Past
/// 2⁵³ the double is the wrong whole number (90·(2⁵³ + 1)° is an odd
/// quarter turn, a pole, read as an even one: tan = 0, defined); past 2⁶³
/// the cast saturates to an odd number (9·10²¹° is 10²⁰ quarter turns, tan
/// = 0, read as a pole); and past the working precision a quotient that is
/// no whole number rounds to one (2²⁰⁰° is no quarter turn).
#[test]
fn tan_at_a_quarter_turn_past_the_doubles() {
    use replay::iv::{self, Iv, Unit};
    use rug::{Float, Integer};
    iv::init();
    let at = |n: Integer| Iv::point(Float::with_val(iv::prec(), n));
    let two53 = Integer::from(1) << 53u32;
    for (unit, quarter) in [(Unit::Degrees, 90), (Unit::Grads, 100)] {
        iv::set_prec(160);
        // Odd quarter turns: poles.
        for k in [
            Integer::from(1),
            Integer::from(-3),
            two53.clone() + 1,
            -two53.clone() - 1,
            (Integer::from(1) << 80u32) + 1,
        ] {
            let v = iv::tan(&at(k.clone() * quarter), unit);
            assert!(v.empty, "tan at {k} quarter turns [{unit:?}]: {v:?}");
        }
        // Even quarter turns: 0 exactly.
        for k in [
            Integer::from(0),
            Integer::from(2),
            two53.clone() + 2,
            Integer::from(Integer::u_pow_u(10, 20)),
            -(Integer::from(1) << 80u32),
        ] {
            let v = iv::tan(&at(k.clone() * quarter), unit);
            assert!(
                v.def && v.is_exactly(0.0),
                "tan at {k} quarter turns [{unit:?}]: {v:?}"
            );
        }
        // No whole number of quarter turns, though the quotient rounds to
        // one at 160 bits (2²⁰⁰° is 256° and 176 grads, turns aside):
        // neither a pole nor 0 there, and decided with more bits.
        let v = iv::tan(&at(Integer::from(1) << 200u32), unit);
        assert!(
            !v.empty && !(v.def && v.is_exactly(0.0)),
            "tan at 2²⁰⁰ [{unit:?}]: {v:?}"
        );
        iv::set_prec(640);
        let v = iv::tan(&at(Integer::from(1) << 200u32), unit);
        assert!(v.def && v.ne0(), "tan at 2²⁰⁰ [{unit:?}], 640 bits: {v:?}");
    }
    iv::set_prec(160);
    // And through the evaluator, from a typed number.
    let (e, lits) = read("tan(x+810647932926689370)");
    let ctx = replay::eval::Ctx {
        unit: Unit::Degrees,
        lits: &lits,
        vars: &[],
    };
    let s = replay::eval::series(&e, 0.0, 0.0, 0, &ctx);
    assert!(s[0].empty, "tan(90·(2⁵³ + 1)°): {:?}", s[0]);
}

/// The audit of review 18: what a certificate writes is read whole or
/// refused. A path names a child by a byte (the 257th argument of a max was
/// read as the first), a Taylor order is one the certifier writes, a power
/// of π in a fact is a small one, and a whole number in a fact is the one
/// written (not the double it rounds to).
#[test]
fn certificate_fields_out_of_range_are_refused() {
    let beyond = |path: serde_json::Value| {
        replay::claim(&serde_json::json!({"Beyond": {
            "x": {"a": 0.0, "b": 1.0},
            "of": {"G": {"path": path, "via": "Itself"}},
            "c": 0.0, "above": true}}))
    };
    assert!(beyond(serde_json::json!([0, 255])).is_ok());
    for bad in [
        serde_json::json!([256]),
        serde_json::json!([0, 4294967296u64]),
        serde_json::json!([-1]),
        serde_json::json!([0.5]),
    ] {
        assert!(beyond(bad.clone()).is_err(), "path {bad} read");
    }
    let touch = |order: serde_json::Value| {
        replay::claim(&serde_json::json!({"Touch": {
            "x": {"a": 0.0, "b": 1.0}, "of": {"F": "F0"},
            "c": 0.0, "at": 0.5, "order": order, "above": true}}))
    };
    let chain = |order: serde_json::Value| {
        replay::claim(&serde_json::json!({"TailChain": {
            "side": "Right", "from": 1.0, "of": {"F": "F0"},
            "c": 0.0, "above": true, "order": order}}))
    };
    assert!(touch(serde_json::json!(2)).is_ok() && chain(serde_json::json!(3)).is_ok());
    for bad in [
        serde_json::json!(256),
        serde_json::json!(1u64 << 40),
        serde_json::json!(-1),
    ] {
        assert!(touch(bad.clone()).is_err(), "Touch order {bad} read");
        assert!(chain(bad.clone()).is_err(), "TailChain order {bad} read");
    }
    replay::iv::init();
    replay::iv::set_prec(160);
    assert!(replay::claims::pi_q("1/2·π^2").is_some());
    assert!(replay::claims::pi_q("1/2·π^2147483647").is_none());
    assert!(replay::claims::pi_q("1/2·π^-2147483648").is_none());
    // A whole number past 2⁵³ in a fact is enclosed, not rounded to a
    // double; a tree of it for the exact algebra is not made at all.
    let v = replay::claims::pi_q("9007199254740993").unwrap();
    assert!(v.lo == v.hi && v.lo > 9007199254740992.0 && v.lo < 9007199254740994.0);
    let v = replay::claims::pi_q("9007199254740993/3").unwrap();
    assert!(v.lo == 3002399751580331.0 && v.hi == 3002399751580331.0);
    let v = replay::claims::pi_q("-1/3").unwrap();
    assert!(v.lo < v.hi && v.hi < -0.333 && v.lo > -0.334);
    assert!(replay::claims::pi_q("1/0").is_none_or(|v| v.empty));
    assert!(replay::algebra::piq_expr(9007199254740992, 1, 0).is_some());
    assert!(replay::algebra::piq_expr(9007199254740993, 1, 0).is_none());
    assert!(replay::algebra::piq_expr(1, -9007199254740993, 1).is_none());
    assert!(replay::algebra::piq_expr(i64::MIN, 1, 0).is_none());
}

/// The audit of review 18: the least period's proof tries P/q for each
/// prime q up to a bound read off the certificate (P over a family's
/// period, or over f′'s bound). A family period of 10⁻³⁰⁰ made that bound
/// 3·10³⁰⁰, cast to u64::MAX primes to sieve: the replay never returned.
/// Past what the certifier itself tries the row is not confirmed.
#[test]
fn a_period_bound_past_any_sieve_is_refused() {
    let mut v = certificate("tan(x)");
    assert_eq!(rejected(&v), None);
    let fam = &mut value_of(&mut v, "domain")["excluded"][0]["period"];
    assert!(
        fam["lo"].is_number(),
        "tan(x)'s domain lists no family: retarget"
    );
    fam["lo"] = serde_json::json!(1e-300);
    let r = replay::replay(&v).expect("replayed");
    let period = r.rows.iter().find(|r| r.name == "period").unwrap();
    assert!(
        period.problems.iter().any(|p| p.contains("past any sieve")),
        "{:?}",
        period.problems
    );
    // A family period not shown positive bounds nothing either (P over a
    // negative low end was a negative bound: no prime to try, and the
    // least period stood unchecked), nor does a period not shown positive.
    for (lo, row_lo) in [
        (-1.0, None),
        (0.0, None),
        (f64::NAN, None),
        (1.0, Some(-4.0)),
    ] {
        let mut v = certificate("tan(x)");
        value_of(&mut v, "domain")["excluded"][0]["period"]["lo"] = if lo.is_nan() {
            serde_json::json!("nan")
        } else {
            serde_json::json!(lo)
        };
        if let Some(l) = row_lo {
            value_of(&mut v, "period")["lo"] = serde_json::json!(l);
        }
        let r = replay::replay(&v).expect("replayed");
        let period = r.rows.iter().find(|r| r.name == "period").unwrap();
        assert!(
            !period.problems.is_empty(),
            "a family period from {lo}, the period from {row_lo:?}: the least period stands"
        );
    }
}

// ------------------------------------------------- review 18: listings

/// The double below 1/3 (the nearest): 0.333…3148.
const THIRD: f64 = 1.0 / 3.0;

/// The enclosure [d, d].
fn point(d: f64) -> serde_json::Value {
    serde_json::json!({"lo": d, "hi": d})
}

/// The body of a certified or partial row (mutable).
fn body_of<'v>(v: &'v mut serde_json::Value, row: &str) -> &'v mut serde_json::Value {
    v[row].as_object_mut().unwrap().values_mut().next().unwrap()
}

/// The claims of a kind in a row (copies).
fn claims_of_kind(v: &mut serde_json::Value, row: &str, kind: &str) -> Vec<serde_json::Value> {
    let found: Vec<serde_json::Value> = claims_of(v, row)
        .iter()
        .filter(|c| c.get(kind).is_some())
        .cloned()
        .collect();
    assert!(
        !found.is_empty(),
        "the plant needs a {kind} claim in {row}, which the certifier no longer writes for this \
         function: retarget the plant"
    );
    found
}

/// The range row made up as Certified, (−∞, e) ∪ (e, ∞) with e listed as
/// `e` (open) or the same closed on its upper piece, each tail's limit the
/// source of an end, on the true claims `claims` and f unbounded at its
/// pole (the vertical row's claim).
fn range_about(v: &mut serde_json::Value, e: (f64, f64), mut claims: Vec<serde_json::Value>) {
    claims.extend(claims_of_kind(v, "vertical", "Unbounded"));
    let end = serde_json::json!({"At": {"x": {"lo": e.0, "hi": e.1}, "closed": false}});
    v["range"] = serde_json::json!({"Certified": {
        "value": [{"lo": "NegInf", "hi": end}, {"lo": end, "hi": "PosInf"}],
        "cert": {"covers": "Line", "claims": claims}
    }});
    v["range_ends"] = serde_json::json!([[["Other"], [{"Tail": "Left"}]],
        [[{"Tail": "Right"}], ["Other"]]]);
}

/// The two facts "f = N/D exactly: f → `limit` as x → ±∞".
fn rational_limits(limit: &str) -> Vec<serde_json::Value> {
    ["−∞", "+∞"]
        .iter()
        .map(|at| {
            serde_json::json!({"Simplifier": {"fact":
            format!("f = N/D exactly: f → {limit} as x → {at}")}})
        })
        .collect()
}

/// Review 18's follow-up: listings that miss the true value. Each row of a
/// certificate lists numbers (a zero, an end, a period, a slope) as
/// enclosures [lo, hi]; the claims and facts beside them are left true, and
/// the listing is made a point at a double the true value is not (1/3
/// listed at 0.333…3148, 2⁵³ + 1 at 2⁵³, π at 3.14159…793), or an interval
/// that leaves it out. Each must be refused: a listing holds the value its
/// claims prove, compared exactly where a fact gives it exactly.
const LISTINGS: &[Plant] = &[
    // ---- the range
    (
        "(9007199254740993x+1)/(x+1)",
        "range: 2⁵³ left out, not the limit 2⁵³ + 1 (the tails' structural limits)",
        |v| {
            let limits = claims_of_kind(v, "horizontal", "Limit");
            range_about(v, (9007199254740992.0, 9007199254740992.0), limits)
        },
    ),
    (
        "(9007199254740993x+1)/(x+1)",
        "range: 2⁵³ left out, not the limit 2⁵³ + 1 (a rational's exact limits)",
        |v| {
            range_about(
                v,
                (9007199254740992.0, 9007199254740992.0),
                rational_limits("9007199254740993"),
            )
        },
    ),
    (
        "(x+1)/(3x)",
        "range: the double nearest 1/3 left out (the tails' structural limits)",
        |v| {
            let limits = claims_of_kind(v, "horizontal", "Limit");
            range_about(v, (THIRD, THIRD), limits)
        },
    ),
    (
        "(x+1)/(3x)",
        "range: the double nearest 1/3 left out (a rational's exact limits)",
        |v| range_about(v, (THIRD, THIRD), rational_limits("1/3")),
    ),
    (
        "(x+1)/(3x)",
        "range: an interval beside 1/3 left out (a rational's exact limits)",
        |v| {
            range_about(
                v,
                (THIRD.next_up(), THIRD.next_up().next_up()),
                rational_limits("1/3"),
            )
        },
    ),
    (
        "atan(x)",
        "range: the ends at the doubles nearest ∓π/2",
        |v| {
            let p = &mut value_of(v, "range")[0];
            let d = std::f64::consts::FRAC_PI_2;
            p["lo"]["At"]["x"] = point(-d);
            p["hi"]["At"]["x"] = point(d);
        },
    ),
    (
        "(3x-1)^2+1/3",
        "range: the least value 1/3 listed at the double nearest it",
        |v| value_of(v, "range")[0]["lo"]["At"]["x"] = point(THIRD),
    ),
    (
        "(3x-1)^2+1/3",
        "range: the least value listed above 1/3",
        |v| value_of(v, "range")[0]["lo"]["At"]["x"] = point(THIRD.next_up()),
    ),
    (
        "((x-1)^2+1/3)*(x-1)/(x-1)",
        "range: the hole's value 1/3, the open end, listed at the double nearest it",
        |v| value_of(v, "range")[0]["lo"]["At"]["x"] = point(THIRD),
    ),
    (
        "((x-1)^2+1/3)*(x-1)/(x-1)",
        "range: the hole's value 1/3 called attained",
        |v| value_of(v, "range")[0]["lo"]["At"]["closed"] = serde_json::json!(true),
    ),
    // An end on the wrong side: closed where the value is only approached,
    // open where it is taken; with the ends' sources, and without them.
    ("1/(x^2+1)", "range: the limit 0 called attained", |v| {
        value_of(v, "range")[0]["lo"]["At"]["closed"] = serde_json::json!(true)
    }),
    (
        "1/(x^2+1)",
        "range: the greatest value 1 called not attained",
        |v| value_of(v, "range")[0]["hi"]["At"]["closed"] = serde_json::json!(false),
    ),
    (
        "1/(x^2+1)",
        "range: the limit 0 called attained, the ends' sources left out",
        |v| {
            value_of(v, "range")[0]["lo"]["At"]["closed"] = serde_json::json!(true);
            v.as_object_mut().unwrap().remove("range_ends");
        },
    ),
    (
        "1/(x^2+1)",
        "range: the greatest value 1 called not attained, the ends' sources left out",
        |v| {
            value_of(v, "range")[0]["hi"]["At"]["closed"] = serde_json::json!(false);
            v.as_object_mut().unwrap().remove("range_ends");
        },
    ),
    (
        "1/(x^2+1)",
        "range: the limit 0 called attained, its source called another",
        |v| {
            value_of(v, "range")[0]["lo"]["At"]["closed"] = serde_json::json!(true);
            v["range_ends"][0][0] = serde_json::json!(["Other"]);
        },
    ),
    // ---- the horizontal asymptotes
    (
        "(x+1)/(3x)",
        "horizontal: the double nearest 1/3, beside a (true) fact that the limit is 1/3",
        |v| {
            for item in value_of(v, "horizontal").as_array_mut().unwrap() {
                item["y"] = point(THIRD);
            }
            for at in ["−∞", "+∞"] {
                claims_of(v, "horizontal").push(serde_json::json!({"Simplifier": {"fact":
                    format!("f → 1/3 as x → {at}")}}));
            }
        },
    ),
    ("atan(x)", "horizontal: the doubles nearest ∓π/2", |v| {
        for item in value_of(v, "horizontal").as_array_mut().unwrap() {
            let d = std::f64::consts::FRAC_PI_2;
            item["y"] = point(if item["side"] == "Left" { -d } else { d });
        }
    }),
    // (The exact form beside the enclosure is what the panel shows as the
    // limit; "looks like" a number inside the enclosure, no more.)
    (
        "(x+1)/(3x)",
        "horizontal: the limit written exactly as the double below 1/3",
        |v| {
            for item in value_of(v, "horizontal").as_array_mut().unwrap() {
                item["exact"] = serde_json::json!("6004799503160661/18014398509481984");
            }
        },
    ),
    (
        "atan(x)",
        "horizontal: the limits written exactly as the doubles nearest ∓π/2",
        |v| {
            for item in value_of(v, "horizontal").as_array_mut().unwrap() {
                let d = "884279719003555/562949953421312";
                item["exact"] = serde_json::json!(if item["side"] == "Left" {
                    format!("-{d}")
                } else {
                    d.to_string()
                });
            }
        },
    ),
    (
        "(x+1)/(3x)",
        "horizontal: the limit said to look like 1/2",
        |v| {
            for item in value_of(v, "horizontal").as_array_mut().unwrap() {
                item["looks_like"] = serde_json::json!(0.5);
            }
        },
    ),
    // ---- the oblique asymptotes
    (
        "(x^2+x+1)/(3x)",
        "oblique: the slope 1/3 listed at the double nearest it",
        |v| {
            for item in value_of(v, "oblique").as_array_mut().unwrap() {
                item["m"] = point(THIRD);
            }
        },
    ),
    (
        "(x^2+x+1)/(3x)",
        "oblique: the intercept 1/3 listed at the double nearest it",
        |v| {
            for item in value_of(v, "oblique").as_array_mut().unwrap() {
                item["b"] = point(THIRD);
            }
        },
    ),
    ("(x^2+x+1)/(3x)", "oblique: the slope listed as 5", |v| {
        for item in value_of(v, "oblique").as_array_mut().unwrap() {
            item["m"] = point(5.0);
        }
    }),
    (
        "sqrt(x^2+1)/3",
        "oblique: the slopes ∓1/3 listed at the doubles nearest them",
        |v| {
            for item in value_of(v, "oblique").as_array_mut().unwrap() {
                item["m"] = point(if item["side"] == "Left" {
                    -THIRD
                } else {
                    THIRD
                });
            }
        },
    ),
    // ---- the x-intercepts
    (
        "3x-1",
        "x-intercepts: the zero 1/3 listed at the double nearest it",
        |v| value_of(v, "x_intercepts")[0]["At"] = point(THIRD),
    ),
    (
        "sin(x)",
        "x-intercepts: repeating every 6.28318…586, the double nearest 2π",
        |v| {
            for item in value_of(v, "x_intercepts").as_array_mut().unwrap() {
                item["Every"]["period"] = point(std::f64::consts::TAU);
            }
        },
    ),
    (
        "sin(x)",
        "x-intercepts: one period's window cut at 1, the zero at π left out",
        |v| {
            let b = body_of(v, "x_intercepts");
            b["cert"]["covers"]["Period"]["b"] = serde_json::json!(1.0);
            b["cert"]["claims"].as_array_mut().unwrap().retain(|c| {
                c.as_object()
                    .and_then(|o| o.values().next())
                    .and_then(|c| c.get("x"))
                    .is_none_or(|x| x["a"].as_f64().unwrap() <= 1.0)
            });
            b["value"]
                .as_array_mut()
                .unwrap()
                .retain(|i| i["Every"]["x0"]["lo"] == 0.0);
        },
    ),
    // ---- the vertical asymptotes
    (
        "1/(3x-1)",
        "vertical: the pole 1/3 listed at the double nearest it",
        |v| value_of(v, "vertical")[0]["At"] = point(THIRD),
    ),
    (
        "1/(3x-1)",
        "vertical: the pole 1/3 listed at the double nearest it, its claim's box too",
        |v| {
            value_of(v, "vertical")[0]["At"] = point(THIRD);
            first(v, "vertical", "Unbounded")["at"] = serde_json::json!({"a": THIRD, "b": THIRD});
        },
    ),
    (
        "tan(x)",
        "vertical: the poles from the double nearest π/2",
        |v| value_of(v, "vertical")[0]["Every"]["x0"] = point(std::f64::consts::FRAC_PI_2),
    ),
    (
        "tan(x)",
        "vertical: the poles every 3.14159…793, the double nearest π",
        |v| value_of(v, "vertical")[0]["Every"]["period"] = point(std::f64::consts::PI),
    ),
    // ---- the domain and its ends
    (
        "sqrt(3x-1)",
        "domain: the end 1/3 listed closed at the double above it",
        |v| value_of(v, "domain")["pieces"][0]["lo"]["At"]["x"] = point(THIRD.next_up()),
    ),
    (
        "sqrt(3x-1)",
        "domain: the end 1/3 listed closed at the double below it",
        |v| value_of(v, "domain")["pieces"][0]["lo"]["At"]["x"] = point(THIRD),
    ),
    (
        "sqrt(3x-1)",
        "domain: the end 1/3 listed in an interval beside it",
        |v| {
            value_of(v, "domain")["pieces"][0]["lo"]["At"]["x"] = serde_json::json!({
                "lo": THIRD.next_up(), "hi": THIRD.next_up().next_up()})
        },
    ),
    (
        "ln(3x-1)",
        "domain: the end 1/3 listed open at the double below it",
        |v| value_of(v, "domain")["pieces"][0]["lo"]["At"]["x"] = point(THIRD),
    ),
    (
        "ln(3x-1)",
        "domain: the end 1/3 listed open at the double above it",
        |v| value_of(v, "domain")["pieces"][0]["lo"]["At"]["x"] = point(THIRD.next_up()),
    ),
    (
        "1/(3x-1)",
        "domain: the hole 1/3 listed at the double nearest it",
        |v| {
            let ps = &mut value_of(v, "domain")["pieces"];
            ps[0]["hi"]["At"]["x"] = point(THIRD);
            ps[1]["lo"]["At"]["x"] = point(THIRD);
        },
    ),
    (
        "x^2+x",
        "domain: a hole made up, in an interval about 1/3",
        |v| {
            let end = serde_json::json!({"At": {
                "x": {"lo": THIRD, "hi": THIRD.next_up()}, "closed": false}});
            value_of(v, "domain")["pieces"] =
                serde_json::json!([{"lo": "NegInf", "hi": end}, {"lo": end, "hi": "PosInf"}]);
        },
    ),
    (
        "tan(x)",
        "domain: the excluded family every 3.14159…793, its claim's box too",
        |v| {
            let d = std::f64::consts::PI;
            value_of(v, "domain")["excluded"][0]["period"] = point(d);
            first(v, "domain", "Family")["period"] = serde_json::json!({"a": d, "b": d});
        },
    ),
    // ---- extrema, inflections and the monotone pieces
    (
        "(3x-1)^2+1/3",
        "extrema: the minimum's place 1/3 listed at the double nearest it",
        |v| value_of(v, "extrema")[0]["x"] = point(THIRD),
    ),
    (
        "(3x-1)^2+1/3",
        "extrema: the minimum's value 1/3 listed at the double nearest it",
        |v| value_of(v, "extrema")[0]["y"] = point(THIRD),
    ),
    (
        "sin(x)",
        "extrema: repeating every 6.28318…586, the double nearest 2π",
        |v| {
            for item in value_of(v, "extrema").as_array_mut().unwrap() {
                item["every"] = point(std::f64::consts::TAU);
            }
        },
    ),
    (
        "(3x-1)^3+x",
        "inflections: the place 1/3 listed at the double nearest it",
        |v| value_of(v, "inflections")[0]["x"] = point(THIRD),
    ),
    (
        "(3x-1)^3+x",
        "inflections: the value 1/3 listed at the double nearest it",
        |v| value_of(v, "inflections")[0]["y"] = point(THIRD),
    ),
    (
        "(3x-1)^2+1/3",
        "monotonicity: the pieces meeting at the double nearest 1/3",
        |v| {
            let ps = value_of(v, "monotonicity");
            ps[0]["on"]["hi"]["At"]["x"] = point(THIRD);
            ps[1]["on"]["lo"]["At"]["x"] = point(THIRD);
        },
    ),
    (
        "(3x-1)^2+1/3",
        "monotonicity: the pieces meeting at the double above 1/3",
        |v| {
            let ps = value_of(v, "monotonicity");
            ps[0]["on"]["hi"]["At"]["x"] = point(THIRD.next_up());
            ps[1]["on"]["lo"]["At"]["x"] = point(THIRD.next_up());
        },
    ),
    (
        "sqrt(3x-1)",
        "monotonicity: the piece from the double below the domain's end 1/3",
        |v| value_of(v, "monotonicity")[0]["on"]["lo"]["At"]["x"] = point(THIRD),
    ),
    (
        "sqrt(3x-1)",
        "monotonicity: the piece from −1, outside the domain",
        |v| value_of(v, "monotonicity")[0]["on"]["lo"]["At"]["x"] = point(-1.0),
    ),
    (
        "1/(3x-1)",
        "monotonicity: the pieces meeting at the double nearest the pole 1/3",
        |v| {
            let ps = value_of(v, "monotonicity");
            ps[0]["on"]["hi"]["At"]["x"] = point(THIRD);
            ps[1]["on"]["lo"]["At"]["x"] = point(THIRD);
        },
    ),
    (
        "(3x-1)^2+1/3",
        "monotonicity: the increasing piece left out",
        |v| {
            value_of(v, "monotonicity")
                .as_array_mut()
                .unwrap()
                .truncate(1)
        },
    ),
    // ---- what a row leaves out or lets in beside its listings
    (
        "sin(x)",
        "x-intercepts: the family through π from the double nearest it",
        |v| value_of(v, "x_intercepts")[1]["Every"]["x0"] = point(std::f64::consts::PI),
    ),
    (
        "1/(3x-1)",
        "domain: the pole 1/3 let in (an end closed where the pieces meet)",
        |v| value_of(v, "domain")["pieces"][0]["hi"]["At"]["closed"] = serde_json::json!(true),
    ),
    (
        "(x^2+x+1)/(3x)",
        "oblique: the line on the right left out",
        |v| {
            value_of(v, "oblique")
                .as_array_mut()
                .unwrap()
                .retain(|i| i["side"] == "Left")
        },
    ),
    ("tan(x)", "vertical: the poles left out", |v| {
        value_of(v, "vertical").as_array_mut().unwrap().clear()
    }),
    // ---- the period and the y-intercept
    (
        "sin(6*pi*x)",
        "period: 1/3 listed at the double nearest it",
        |v| *value_of(v, "period") = point(THIRD),
    ),
    (
        "sin(x)",
        "period: 2π listed at the double nearest it",
        |v| *value_of(v, "period") = point(std::f64::consts::TAU),
    ),
    (
        "x+1/3",
        "y-intercept: 1/3 listed at the double nearest it",
        |v| *value_of(v, "y_intercept") = point(THIRD),
    ),
    (
        "x+1/3",
        "y-intercept: 1/3 listed at the double nearest it, its claim's value too",
        |v| {
            *value_of(v, "y_intercept") = point(THIRD);
            let c = first(v, "y_intercept", "Value");
            c["lo"] = serde_json::json!(THIRD);
            c["hi"] = serde_json::json!(THIRD);
        },
    ),
];

#[test]
fn listings_that_miss_the_true_value_are_refused() {
    let mut missed = Vec::new();
    let mut cache: BTreeMap<&str, serde_json::Value> = BTreeMap::new();
    for &(src, what, plant) in LISTINGS {
        let base = cache.entry(src).or_insert_with(|| certificate(src)).clone();
        if let Some(why) = rejected(&base) {
            panic!("{src}: the true certificate is rejected: {why}");
        }
        let mut v = base.clone();
        plant(&mut v);
        match rejected(&v) {
            Some(why) => println!("{src}: {what}: caught ({why})"),
            None => {
                println!("{src}: {what}: MISSED");
                missed.push(format!("{src}: {what}"));
            }
        }
    }
    assert!(missed.is_empty(), "listings not refused: {missed:#?}");
}

// ------------------------------------------------------------ the tester's findings

/// The certificate of `src` typed to `digits` digits with the sliders
/// `vars`, as JSON.
fn certificate_at(src: &str, digits: Option<u8>, vars: &[(&str, f64)]) -> serde_json::Value {
    let parse = graphing::lexer::ParseOptions {
        literal_digits: digits,
        ..Default::default()
    };
    let vars: BTreeMap<String, f64> = vars.iter().map(|(n, v)| (n.to_string(), *v)).collect();
    let opts = CompileOptions {
        trig_unit: TrigUnit::Radians,
        variables: &vars,
    };
    let a = graphing::certify::certify_text_with(src, parse, opts, DEFAULT_BUDGET, None)
        .expect("certified");
    serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap()
}

/// The claims of a kind the replay judged, with their classes.
fn judged(v: &serde_json::Value, kind: &str) -> Vec<(String, Class)> {
    replay::replay(v)
        .expect("replayed")
        .claims
        .iter()
        .map(|c| (format!("{:?}", c.claim), c.outcome.class))
        .filter(|(c, _)| c.starts_with(kind))
        .collect()
}

/// `tests/crossfuzz.rs`, the replay's root: root(a, d) takes a negative a
/// for an odd whole degree d alone. A degree no whole number was read as
/// the power 1/d by the rule for a constant exponent, which takes a
/// negative base to a whole one: root(−8, 0.5) was (−8)² = 64, and the
/// certifier's true "undefined" refuted. The rule is the language's
/// (`graphing::functions::root`): d = 0 or infinite, nothing; d whole, the
/// real d-th root (odd d: of any a; even d: of a ≥ 0); any other d, a ≥ 0
/// only; a = 0 only for d > 0; a degree that varies with x, the same at
/// each x.
#[test]
fn a_root_takes_a_negative_number_to_an_odd_whole_degree_alone() {
    replay::iv::init();
    let at = |src: &str, lo: f64, hi: f64, vars: &[(String, f64)]| {
        let (e, lits) = read(src);
        let ctx = replay::eval::Ctx {
            unit: replay::iv::Unit::Radians,
            lits: &lits,
            vars,
        };
        replay::eval::series(&e, lo, hi, 1, &ctx)
    };
    // (source, x, f(x): None where f is undefined.)
    let table: &[(&str, f64, Option<f64>)] = &[
        // No whole degree: a ≥ 0 only, though 1/d is whole.
        ("root(x,0.5)", -8.0, None),
        ("root(x,0.5)", 4.0, Some(16.0)),
        ("root(x,0.5)", 0.0, Some(0.0)),
        ("root(x-6,0.5)", 0.0, None),
        ("root(-8,0.5)", 0.0, None),
        ("root(ln(x),0.5)", 0.5, None),
        ("root(x,0.25)", -2.0, None),
        ("root(x,0.25)", 2.0, Some(16.0)),
        ("root(x,-0.5)", -2.0, None),
        ("root(x,-0.5)", 2.0, Some(0.25)),
        ("root(x,-0.5)", 0.0, None),
        ("root(x,1/4)", -2.0, None),
        ("root(x,1/4)", 2.0, Some(16.0)),
        ("root(x,2.5)", -32.0, None),
        ("root(x,2.5)", 32.0, Some(4.0)),
        // (Read from its enclosure, a point: √¼.)
        ("root(x,sqrt(1/4))", -2.0, None),
        ("root(x,sqrt(1/4))", 2.0, Some(4.0)),
        // A whole degree: odd of any a, even of a ≥ 0.
        ("root(x,3)", -8.0, Some(-2.0)),
        ("root(x,-3)", -8.0, Some(-0.5)),
        ("root(x,2)", -4.0, None),
        ("root(x,2)", 4.0, Some(2.0)),
        ("root(x,-2)", 4.0, Some(0.5)),
        ("root(x,-2)", 0.0, None),
        ("root(x,-3)", 0.0, None),
        ("root(x,0.5*6)", -8.0, Some(-2.0)),
        ("root(x,sqrt(4))", 4.0, Some(2.0)),
        ("root(x,sqrt(4))", -4.0, None),
        ("root(x,sqrt(9))", -8.0, Some(-2.0)),
        // No degree.
        ("root(x,0)", 4.0, None),
        ("root(x,0.1+0.2-0.3)", 4.0, None),
        // A degree that varies: by its value at each x.
        ("root(x,x)", -1.0, Some(-1.0)),
        ("root(x,x)", -0.5, None),
        ("root(x,x)", -2.0, None),
        ("root(x,x)", 0.0, None),
        ("root(x,x)", 0.5, Some(0.25)),
        ("root(x,x)", 2.0, Some(std::f64::consts::SQRT_2)),
        ("root(-8,x)", 3.0, Some(-2.0)),
        ("root(-8,x)", 2.0, None),
        ("root(-8,x)", 0.5, None),
        ("root(2,x)", 0.5, Some(4.0)),
        ("root(2,x)", -0.5, Some(0.25)),
        ("root(x,floor(x))", -0.5, Some(-2.0)),
        ("root(x,floor(x))", 0.5, None),
        ("root(x,1/x)", 0.5, Some(std::f64::consts::FRAC_1_SQRT_2)),
        ("root(x,1/x)", -0.5, None),
    ];
    for &(src, x, want) in table {
        let v = &at(src, x, x, &[])[0];
        match want {
            None => assert!(v.empty, "{src} at {x}: {v:?}, not undefined"),
            Some(y) => {
                let (lo, hi) = (v.lo.to_f64(), v.hi.to_f64());
                let near = lo <= y.next_up() && y.next_down() <= hi && hi - lo <= 1e-12;
                assert!(!v.empty && v.def && near, "{src} at {x}: {v:?}, not {y}");
            }
        }
    }
    // An infinite degree is none (a slider there).
    let inf = [("a".to_string(), f64::INFINITY)];
    assert!(at("root(x,a)", 4.0, 4.0, &inf)[0].empty);
    // A degree only enclosed (e): no whole number, so a ≥ 0 only.
    assert!(at("root(x,e)", -2.0, -2.0, &[])[0].empty);
    let v = &at("root(x,e)", 2.0, 2.0, &[])[0];
    assert!(v.def && v.gt(1.2904) && v.lt(1.2905), "root(2, e): {v:?}");
    // Derivatives: of the power where the degree is constant (root(x, 0.5)
    // is x² on x > 0: f′(4) = 8); of e^(ln a / d) where it varies over a
    // box with a > 0 and d ≠ 0 (x^(1/x) at 2.5: f·(1 − ln x)/x²); none
    // where a varying degree is whole on the box (it may be another
    // beside it), nor where the radicand may be negative and the degree
    // whole somewhere.
    let s = at("root(x,0.5)", 4.0, 4.0, &[]);
    assert!(s[1].def && s[1].gt(7.999) && s[1].lt(8.001), "{:?}", s[1]);
    let s = at("root(x,x)", 2.5, 2.5, &[]);
    assert!(s[1].def && s[1].gt(0.0193) && s[1].lt(0.0194), "{:?}", s[1]);
    let s = at("root(x,x)", 1.5, 2.5, &[]);
    assert!(
        s[0].def && s[0].gt(1.0) && s[0].lt(2.0) && s[1].def,
        "{s:?}"
    );
    let s = at("root(x,x)", 2.0, 2.0, &[]);
    assert!(s[0].def && !s[1].def, "{s:?}");
    let s = at("root(x,x)", -3.5, -2.5, &[]);
    assert!(!s[0].empty && !s[0].def, "{:?}", s[0]);
    // The certificates the tester's cases stand for replay in full, their
    // "undefined" proved.
    for src in [
        "root(x-6,0.5)",
        "root(-8,0.5)",
        "root(ln(x),0.5)",
        "exp(root(x-6,0.5))",
        "root(x^2-3,0.5)",
        "root(x,x)",
        "root(2,x)",
        "root(x,e)",
    ] {
        assert_eq!(rejected(&certificate(src)), None, "{src}");
    }
    let v = certificate("root(-8,0.5)");
    assert_eq!(
        judged(&v, "Undefined"),
        [("Undefined((0.0, 0.0))".to_string(), Class::Strong)]
    );
}

/// `tests/crossfuzz.rs`, the replay's `OneCross`: a crossing is refuted
/// only when neither way across holds. The low end's side was read off
/// "not shown above c", so an end the replay couldn't place counted as
/// below c, and a true crossing from above was refuted at its high end.
#[test]
fn a_crossing_from_an_end_not_placed_is_not_refuted() {
    // f″ = 2 − 6x on (0, ∞), through ∞ − ∞ at the first double: proved
    // (the series over the box cancel it), no longer refuted.
    let src = "log(x,2)-log(x,2)+(x^2-x^3+6)";
    let v = certificate(src);
    assert_eq!(rejected(&v), None, "{src}");
    let crossings = judged(&v, "OneCross");
    assert!(
        crossings.iter().any(|(c, _)| c.contains("x: (5e-324,")),
        "the crossing from the first double is no longer claimed: retarget ({crossings:?})"
    );
    for (c, class) in &crossings {
        assert!(
            matches!(class, Class::Strong | Class::Weak),
            "{c}: {class:?}"
        );
    }
    // f′ of the source's tree has no value at 1, the first point of the
    // domain (acosh′ there): the crossing on [1, 2.5] is left open.
    let src = "acosh(x)-acosh(x)+sin(x)";
    let v = certificate(src);
    let crossings = judged(&v, "OneCross");
    assert!(
        crossings
            .iter()
            .any(|(c, _)| c.contains("x: (1.0, 2.5), of: F(1)")),
        "the crossing from the domain's end is no longer claimed: retarget ({crossings:?})"
    );
    let r = replay::replay(&v).expect("replayed");
    for c in &r.claims {
        assert_ne!(c.outcome.class, Class::Refuted, "{src}: {:?}", c.claim);
    }
    // f(1) is (−∞, 0.3] to the replay (10^5000 + a − 10^5000 has no known
    // value), f(4) below 0: weak, by the certifier's tree.
    let src = "min((10^5000)+a-(10^5000)+ln(x),x-x^3+0.3)";
    let v = certificate_at(src, Some(16), &[("a", 1.0)]);
    let crossings = judged(&v, "OneCross");
    assert!(
        !crossings.is_empty(),
        "{src}: no crossing claimed: retarget"
    );
    for (c, class) in &crossings {
        assert_eq!(*class, Class::Weak, "{src}: {c}");
    }
    for c in &replay::replay(&v).expect("replayed").claims {
        assert_ne!(c.outcome.class, Class::Refuted, "{src}: {:?}", c.claim);
    }
}

/// `tests/crossfuzz.rs`, "f is periodic, so no oblique asymptote": it
/// follows from f's period (m·P = 0), as far as the certificate's fact of
/// one replays, and is never refuted by f's size at points far out.
#[test]
fn a_periodic_function_has_no_oblique_asymptote_whatever_its_size() {
    const FACT: &str = "Simplifier(\"f is periodic";
    // About 10⁹³²⁶⁴⁰⁰⁰⁰ everywhere: f/x is nowhere near 0 at any double.
    let v = certificate_at("tan(x)+cosh(a)", None, &[("a", 2147483649.0)]);
    assert_eq!(rejected(&v), None);
    let facts = judged(&v, FACT);
    assert_eq!(facts.len(), 1, "{facts:?}");
    assert_eq!(facts[0].1, Class::Strong, "{facts:?}");
    // Past every double at each point tried; its period only agrees at
    // points (the algebra doesn't multiply the power out), so weak.
    let v = certificate("cos(x)^(-9007199254740993/1000001)");
    assert_eq!(rejected(&v), None);
    let facts = judged(&v, FACT);
    assert_eq!(facts.len(), 1, "{facts:?}");
    assert!(
        matches!(facts[0].1, Class::Strong | Class::Weak),
        "{facts:?}"
    );
    // Ordinary ones are proved by their period now (weak before, by f/x
    // at points).
    for src in ["sin(x)", "tan(x)", "sin(x)+cos(2x)", "1/(2+cos(x))"] {
        let v = certificate(src);
        assert_eq!(rejected(&v), None, "{src}");
        let facts = judged(&v, FACT);
        assert_eq!(facts.len(), 1, "{src}: {facts:?}");
        assert_eq!(facts[0].1, Class::Strong, "{src}");
    }
}

const PERIODIC: &str = "f is periodic: f − (m·x + b) does not tend to 0 for m ≠ 0";
const EVEN: &str = "f is even (simplifier): f(−x) = f(x), on a domain symmetric about 0";
const ODD: &str = "f is odd (simplifier): f(−x) = −f(x), on a domain symmetric about 0";

fn fact(text: &str) -> serde_json::Value {
    serde_json::json!({"Simplifier": {"fact": text}})
}

/// The oblique row made up as "none", f called periodic, with the facts
/// `extra` beside.
fn line_denied(v: &mut serde_json::Value, extra: &[&str]) {
    let mut claims = vec![fact(PERIODIC)];
    claims.extend(extra.iter().map(|f| fact(f)));
    v["oblique"] = serde_json::json!({"Certified": {
        "value": [], "cert": {"covers": "Line", "claims": claims}}});
}

/// The x-intercepts row made up as "none" for x² − 1: negative over the
/// window [−½, ½], said to be one period of f by `period`.
fn no_zeros_over_a_window(v: &mut serde_json::Value, period: &str) {
    v["x_intercepts"] = serde_json::json!({"Certified": {"value": [], "cert": {
        "covers": {"Period": {"a": -0.5, "b": 0.5}}, "claims": [
            {"Beyond": {"x": {"a": -0.5, "b": 0.5}, "of": {"F": "F0"},
                "c": 0.0, "above": false}},
            fact(period)]}}});
}

/// The extrema row made up as "none" for x²: f′ negative up to the last
/// double below 0 and positive from the first above it, 0 itself between
/// two gaps, each cleared of zeros (truly: none lies strictly inside);
/// `zero`: with f′(0) = 0 claimed too.
fn minimum_walled_off(v: &mut serde_json::Value, zero: bool) {
    let f1 = serde_json::json!({"F": "F1"});
    let tiny = 5e-324;
    let mut claims = vec![
        serde_json::json!({"TailBeyond": {"side": "Left", "from": -tiny, "of": f1,
            "c": 0.0, "above": false}}),
        serde_json::json!({"TailBeyond": {"side": "Right", "from": tiny, "of": f1,
            "c": 0.0, "above": true}}),
    ];
    for (a, b) in [(-tiny, 0.0), (0.0, tiny)] {
        claims.push(serde_json::json!({"Gap": {"x": {"a": a, "b": b}}}));
        claims.push(serde_json::json!({"GapClear": {"x": {"a": a, "b": b}, "of": f1}}));
    }
    if zero {
        claims.push(
            serde_json::json!({"Value": {"x": {"a": 0.0, "b": 0.0}, "of": f1,
            "lo": 0.0, "hi": 0.0}}),
        );
    }
    v["extrema"] = serde_json::json!({"Certified": {
        "value": [], "cert": {"covers": "Line", "claims": claims}}});
}

/// The parity row made up as `kind` by the simplifier's fact alone.
fn parity_called(v: &mut serde_json::Value, kind: &str, text: &str) {
    v["parity"] = serde_json::json!({"Certified": {
        "value": kind, "cert": {"covers": "Line", "claims": [fact(text)]}}});
}

/// Every row but `keep` withheld (unknown).
fn withhold_all_but(v: &mut serde_json::Value, keep: &str) {
    for row in [
        "domain",
        "x_intercepts",
        "y_intercept",
        "parity",
        "period",
        "extrema",
        "inflections",
        "monotonicity",
        "range",
        "vertical",
        "horizontal",
        "oblique",
    ] {
        if row != keep {
            v[row] = serde_json::json!({"Unknown": {"reason": "withheld"}});
        }
    }
}

/// Parity made up as `kind` by the simplifier's fact alone, every other
/// row withheld.
fn parity_alone(v: &mut serde_json::Value, kind: &str, text: &str) {
    withhold_all_but(v, "parity");
    parity_called(v, kind, text);
}

/// The false version of each claim the third pass stopped refuting, and
/// what its audit of unknowns read as answers found accepted: each must
/// still be (or now is) refused.
const FOUND: &[Plant] = &[
    // ---- a root of a degree no whole number, at a negative radicand
    ("root(x-6,0.5)", "root: f(0) = (−6)² claimed", |v| {
        made_up_y_intercept(v, 36.0, true);
    }),
    ("root(-8,0.5)", "root: f(0) = 64 claimed", |v| {
        made_up_y_intercept(v, 64.0, true);
    }),
    (
        "root(x-6,0.5)",
        "root: defined at a negative radicand",
        |v| {
            let cs = claims_of(v, "parity");
            let i = cs
                .iter()
                .position(|c| c.get("Undefined").is_some())
                .expect("an Undefined claim in parity: retarget the plant");
            cs[i] = serde_json::json!({"Defined": cs[i]["Undefined"].clone()});
        },
    ),
    ("root(x,0.5)", "root: x² on x ≥ 0 called even", |v| {
        parity_called(v, "Even", EVEN);
    }),
    // ---- a crossing
    (
        "log(x,2)-log(x,2)+(x^2-x^3+6)",
        "crossing: claimed where f″ is below 0 at both ends",
        |v| {
            first(v, "inflections", "OneCross")["x"] =
                serde_json::json!({"a": 0.5, "b": 0.9999999999999999});
        },
    ),
    (
        "log(x,2)-log(x,2)+(x^2-x^3+6)",
        "crossing: claimed from an end not placed to one f″ is above 0 at",
        |v| {
            // (f″ = 2 − 6x > 0 up to 1/3: no zero in (0, 1/4].)
            for c in claims_of(v, "inflections").iter_mut() {
                if let Some(o) = c.get_mut("OneCross") {
                    o["x"] = serde_json::json!({"a": 5e-324, "b": 0.25});
                }
            }
            *value_of(v, "inflections") = serde_json::json!([]);
        },
    ),
    // ---- no oblique asymptote, by a period
    (
        "x+1/x",
        "oblique: its line denied, f called periodic",
        |v| {
            line_denied(v, &[]);
        },
    ),
    ("x+1/x", "oblique: its line denied, by a period of 0", |v| {
        line_denied(v, &["f(x + 0) = f(x) wherever f is defined"]);
    }),
    (
        "x+1/x",
        "oblique: its line denied, by a period in [0, 0]",
        |v| {
            line_denied(
                v,
                &["f(x + P) = f(x) wherever f is defined, P ∈ [0e0, 0e0]"],
            );
        },
    ),
    (
        "x+1/x",
        "oblique: its line denied, by a period in [−1, 1]",
        |v| {
            line_denied(
                v,
                &["f(x + P) = f(x) wherever f is defined, P ∈ [-1e0, 1e0]"],
            );
        },
    ),
    ("x+1/x", "oblique: its line denied, by a period 2π", |v| {
        line_denied(v, &["f(x + 2·π) = f(x) wherever f is defined"]);
    }),
    (
        "x+1/x",
        "oblique: its line denied, the rest 1/x of period 0",
        |v| {
            let val = |x: f64, y: f64| {
                serde_json::json!({"Value": {"x": {"a": x, "b": x}, "of": {"F": "F0"},
                    "lo": y, "hi": y}})
            };
            v["oblique"] = serde_json::json!({"Certified": {"value": [], "cert": {
                "covers": "Line", "claims": [val(1.0, 2.0), val(2.0, 2.5),
                    fact("f − 1·x = Div(1,x) has period 0: no oblique asymptote")]}}});
        },
    ),
    // ---- a period that is none
    (
        "x^2-1",
        "period: no zeros, over a window clear of them and a period in [−1, 1]",
        |v| {
            no_zeros_over_a_window(v, "f(x + P) = f(x) wherever f is defined, P ∈ [-1e0, 1e0]");
        },
    ),
    (
        "x^2-1",
        "period: no zeros, over a window clear of them and a period of 0",
        |v| no_zeros_over_a_window(v, "f(x + 0) = f(x) wherever f is defined"),
    ),
    // ---- unbounded, by a root of a negative degree
    (
        "root(1/x,-2)",
        "pole: √x called unbounded beside 0",
        |v| {
            let cs = claims_of(v, "vertical");
            let i = cs
                .iter()
                .position(|c| c.get("Bounded").is_some())
                .expect("a Bounded claim in vertical: retarget the plant");
            let b = cs[i]["Bounded"].clone();
            cs[i] = serde_json::json!({"Unbounded": {"near": b["near"], "at": b["at"]}});
            *value_of(v, "vertical") = serde_json::json!([{"At": {"lo": 0.0, "hi": 0.0}}]);
        },
    ),
    // ---- a zero between two gaps
    (
        "x^2",
        "gaps: the minimum walled off, f′(0) = 0 claimed",
        |v| {
            minimum_walled_off(v, true);
        },
    ),
    (
        "x^2",
        "gaps: the minimum walled off, 0 left to the gaps",
        |v| {
            minimum_walled_off(v, false);
        },
    ),
    // ---- a piece of the domain beside an excluded family
    (
        "tan(x)*sqrt(x)",
        "domain: [0, ∞) less tan's poles extended to the line less them",
        |v| {
            value_of(v, "domain")["pieces"] = serde_json::json!([{"lo": "NegInf", "hi": "PosInf"}]);
        },
    ),
    // ---- a parity with no domain known symmetric
    (
        "ln(x)-ln(x)+x^2",
        "parity: x² on x > 0 called even, every other row withheld",
        |v| parity_alone(v, "Even", EVEN),
    ),
    (
        "ln(x)-ln(x)+x^3",
        "parity: x³ on x > 0 called odd, every other row withheld",
        |v| parity_alone(v, "Odd", ODD),
    ),
];

/// A fact tested at sample points is borne out only by values the replay
/// places there. (10^5000 + 1) − 10^5000 + x is x + 1 (the simplifier's
/// form), but the replay's own enclosure of it at 160 bits is 10⁴⁹⁵² wide:
/// it overlapped itself mirrored, and its negative, at every point, so f
/// was "even" and "odd" alike, weakly, and either parity made up for it
/// was accepted (the third pass's audit). Nothing placed, nothing
/// confirmed: such a fact is left open, the true ones of its certificate
/// (f is the line x + 1) with it.
#[test]
fn samples_that_place_nothing_bear_nothing_out() {
    let src = "(10^5000+1)-(10^5000)+x";
    let base = certificate(src);
    for (kind, text) in [("Even", EVEN), ("Odd", ODD)] {
        let mut v = base.clone();
        parity_called(&mut v, kind, text);
        let facts = judged(&v, "Simplifier(\"f is ");
        assert_eq!(facts.len(), 1, "{src}: {facts:?}");
        assert_eq!(facts[0].1, Class::Unconfirmed, "{src} called {kind}");
        assert!(rejected(&v).is_some(), "{src} called {kind}: accepted");
    }
    // Its own line is not refuted, and not confirmed by overlap either.
    let lines = judged(&base, "Simplifier(\"f = 1·x + 1");
    assert!(!lines.is_empty(), "{src}: no line fact: retarget");
    for (c, class) in &lines {
        assert_eq!(*class, Class::Unconfirmed, "{c}");
    }
    // Nor is a piece of a domain shown by not being refuted: √x typed so
    // that the replay places its radicand nowhere, "defined on all of ℝ",
    // every other row withheld, was let through with a note.
    let src = "sqrt((10^5000+1)-(10^5000)-1+x)";
    let mut v = certificate(src);
    withhold_all_but(&mut v, "domain");
    v["domain"] = serde_json::json!({"Certified": {
        "value": {"pieces": [{"lo": "NegInf", "hi": "PosInf"}], "excluded": []},
        "cert": {"covers": "Line", "claims": []}}});
    let why = rejected(&v).expect("a domain of ℝ for √x, not placed: accepted");
    assert!(why.contains("not shown defined"), "{why}");
    // Where f is placed, the same facts are judged as before: x² + 1 typed
    // so that it is, even (weakly, or by its tree) and not odd.
    let src = "x^2+(1-1+1)";
    let base = certificate(src);
    assert_eq!(rejected(&base), None, "{src}");
    let mut v = base.clone();
    parity_called(&mut v, "Odd", ODD);
    let facts = judged(&v, "Simplifier(\"f is odd");
    assert_eq!(facts.len(), 1, "{facts:?}");
    assert_eq!(facts[0].1, Class::Refuted, "{src} called odd");
}

#[test]
fn the_false_versions_of_what_the_third_pass_let_through_are_refused() {
    let mut missed = Vec::new();
    let mut cache: BTreeMap<&str, serde_json::Value> = BTreeMap::new();
    for &(src, what, plant) in FOUND {
        let base = cache.entry(src).or_insert_with(|| certificate(src)).clone();
        if let Some(why) = rejected(&base) {
            panic!("{src}: the true certificate is rejected: {why}");
        }
        let mut v = base.clone();
        plant(&mut v);
        match rejected(&v) {
            Some(why) => println!("{src}: {what}: caught ({why})"),
            None => {
                println!("{src}: {what}: MISSED");
                missed.push(format!("{src}: {what}"));
            }
        }
    }
    assert!(missed.is_empty(), "not refused: {missed:#?}");
    // What is refuted is proved false: the root's value at a negative
    // radicand, a crossing with one sign at both ends, a made-up period.
    let refuted = |src: &str, what: &str| {
        let (_, _, plant) = FOUND
            .iter()
            .find(|(s, w, _)| *s == src && *w == what)
            .expect("the plant");
        let mut v = certificate(src);
        plant(&mut v);
        let r = replay::replay(&v).expect("replayed");
        assert!(
            r.claims.iter().any(|c| c.outcome.class == Class::Refuted),
            "{src}: {what}: nothing refuted"
        );
    };
    refuted("root(x-6,0.5)", "root: f(0) = (−6)² claimed");
    refuted("root(x-6,0.5)", "root: defined at a negative radicand");
    refuted("root(x,0.5)", "root: x² on x ≥ 0 called even");
    refuted(
        "log(x,2)-log(x,2)+(x^2-x^3+6)",
        "crossing: claimed where f″ is below 0 at both ends",
    );
    refuted("x+1/x", "oblique: its line denied, by a period 2π");
}

/// mod(a, b) where b is 0 has no value (`graphing::functions::modulo`): at
/// a point where the divisor is 0 throughout, the replay's is undefined
/// too, not unknown (the tester's largest class of true claims it left
/// unconfirmed: mod(x, x) "undefined at 0").
#[test]
fn a_remainder_by_zero_is_undefined() {
    replay::iv::init();
    let at = |src: &str, x: f64| {
        let (e, lits) = read(src);
        let ctx = replay::eval::Ctx {
            unit: replay::iv::Unit::Radians,
            lits: &lits,
            vars: &[],
        };
        replay::eval::series(&e, x, x, 0, &ctx).swap_remove(0)
    };
    assert!(at("mod(x,x)", 0.0).empty);
    assert!(at("mod(5,x)", 0.0).empty);
    assert!(at("mod(x,x-x)", 3.0).empty);
    let v = at("mod(x,3)", 7.0);
    assert!(v.def && v.is_exactly(1.0), "{v:?}");
    let v = at("mod(x,x)", 2.5);
    assert!(v.def && v.is_exactly(0.0), "{v:?}");
    // A divisor that only may be 0 on the box leaves it open.
    let (e, lits) = read("mod(5,x)");
    let ctx = replay::eval::Ctx {
        unit: replay::iv::Unit::Radians,
        lits: &lits,
        vars: &[],
    };
    let v = replay::eval::series(&e, -1.0, 1.0, 0, &ctx).swap_remove(0);
    assert!(!v.empty && !v.def, "{v:?}");
    // The certifier's "undefined at 0" is proved.
    let v = certificate("mod(x,x)+x");
    for (c, class) in judged(&v, "Undefined") {
        assert_eq!(class, Class::Strong, "{c}");
    }
}

/// Each shown monotone piece needs a sign walk, and a complete periodic
/// listing covers its domain too. True claims cannot justify omitting a
/// half of sin's pieces or calling x² increasing on the whole line.
#[test]
fn monotone_listings_need_coverage_and_direction() {
    let base = certificate("sin(x)");
    assert_eq!(rejected(&base), None);
    for remove_all in [false, true] {
        let mut v = base.clone();
        let pieces = value_of(&mut v, "monotonicity").as_array_mut().unwrap();
        if remove_all {
            pieces.clear();
        } else {
            pieces.remove(0);
        }
        let why = rejected(&v).expect("an incomplete periodic monotone listing accepted");
        assert!(why.contains("pieces leave"), "{why}");
    }
    let mut shifted = base.clone();
    value_of(&mut shifted, "monotonicity").as_array_mut().unwrap().push(serde_json::json!({
        "on": {"lo": {"At": {"x": {"lo": 10.99557428756427, "hi": 10.995574287564285}, "closed": true}},
            "hi": {"At": {"x": {"lo": 14.137166941154058, "hi": 14.137166941154083}, "closed": true}}},
        "dir": "Decreasing"}));
    assert!(
        rejected(&shifted).is_some(),
        "the increasing piece shifted by 2P called decreasing"
    );
    assert_eq!(
        rejected(&certificate("x^3")),
        None,
        "a same-sign isolated derivative zero does not break a monotone piece"
    );
    assert_eq!(
        rejected(&certificate("sqrt(x-1000)*ln(x-1000)")),
        None,
        "the complete interval beside a shifted open end is decided"
    );
    let base = certificate("x^2");
    let mut v = base.clone();
    withhold_all_but(&mut v, "domain");
    v["monotonicity"] = serde_json::json!({"Partial": {"value": [{
        "on": {"lo": "NegInf", "hi": "PosInf"}, "dir": "Increasing"}],
        "cert": {"covers": "Points", "claims": []}}});
    let why = rejected(&v).expect("an unproven partial monotone piece accepted");
    assert!(why.contains("piece undecided"), "{why}");
    // A real partial piece keeps its original sign evidence.
    let mut v = base.clone();
    withhold_all_but(&mut v, "domain");
    let mono = &base["monotonicity"]["Certified"];
    v["monotonicity"] = serde_json::json!({"Partial": {
        "value": [mono["value"][1].clone()], "cert": {
            "covers": "Points", "claims": mono["cert"]["claims"].clone()}}});
    assert_eq!(rejected(&v), None);
    let mut hole = certificate("x/x");
    value_of(&mut hole, "monotonicity")[0]["on"]["hi"]["At"]["closed"] = serde_json::json!(true);
    let why = rejected(&hole).expect("the excluded endpoint 0 included in a monotone piece");
    assert!(why.contains("undefined endpoint"), "{why}");
    let mut v = certificate("1");
    value_of(&mut v, "monotonicity")[0]["dir"] = serde_json::json!("Increasing");
    assert!(
        rejected(&v).is_some(),
        "a constant called strictly increasing"
    );
}

/// Continuity in a finite-width kink box cannot rule out events inside.
/// The ordinary shifted dent has a true minimum and reaches zero between
/// the surrounding doubles, although both tails are the same plateau.
#[test]
fn continuity_in_a_kink_box_does_not_complete_a_derivative_walk() {
    let base = certificate_at("min((x-sin(1))^(2/1000003),0.01)", Some(14), &[]);
    let claims = serde_json::json!([
        {"Value": {"x": {"a": "-inf", "b": 0.8414709848078963},
            "of": {"F": "F1"}, "lo": 0.0, "hi": 0.0}},
        {"Value": {"x": {"a": 0.8414709848078967, "b": "inf"},
            "of": {"F": "F1"}, "lo": 0.0, "hi": 0.0}},
        {"Kink": {"x": {"a": 0.8414709848078963, "b": 0.8414709848078967}}}
    ]);
    for row in ["extrema", "monotonicity", "range"] {
        let mut v = base.clone();
        withhold_all_but(&mut v, "domain");
        let mut cs = claims.clone();
        let value = match row {
            "extrema" => serde_json::json!([]),
            "monotonicity" => serde_json::json!([{
                "on": {"lo": "NegInf", "hi": "PosInf"}, "dir": "Constant"}]),
            "range" => {
                let y = serde_json::json!({"lo": 0.009999999999999998, "hi": 0.01});
                cs.as_array_mut()
                    .unwrap()
                    .push(serde_json::json!({"Value": {
                    "x": {"a": 0.0, "b": 0.0}, "of": {"F": "F0"},
                    "lo": y["lo"], "hi": y["hi"]}}));
                v["range_ends"] = serde_json::json!([[[{"At": {"lo": 0.0, "hi": 0.0}}],
                    [{"At": {"lo": 0.0, "hi": 0.0}}]]]);
                serde_json::json!([{"lo": {"At": {"x": y, "closed": true}},
                    "hi": {"At": {"x": y, "closed": true}}}])
            }
            _ => unreachable!(),
        };
        v[row] = serde_json::json!({"Certified": {"value": value,
            "cert": {"covers": "Line", "claims": cs}}});
        let r = replay::replay(&v).expect("replayed");
        assert!(
            r.claims.iter().all(|c| c.outcome.class == Class::Strong),
            "{row}: these premises really are true"
        );
        let why = rejected(&v).expect("a false row derived from continuity alone");
        assert!(why.contains("undecided"), "{row}: {why}");
    }
}

#[test]
fn a_placed_kink_can_have_zero_slope_at_its_endpoint() {
    for src in ["x*abs(x)", "(x-1)*abs(x-1)"] {
        let v = certificate(src);
        assert_eq!(rejected(&v), None, "{src}");
        let facts = judged(&v, "KinkAt");
        assert!(!facts.is_empty(), "{src}: no exact one-sided kink proof");
        assert!(facts.iter().all(|(_, c)| *c == Class::Strong));
    }
}

// ------------------------------------------------------------ round 19

/// f′'s own exact form (`algebra::derivative_tree`: f read in the replay's
/// field, differentiated there, written back as a tree) is f′: wherever
/// both are enclosed at a point, the tree's value and the first
/// coefficient of f's series meet. The form decides a sign where the
/// series' sum of terms is ∞ − ∞; a tree that were not f′ would decide a
/// false one.
#[test]
fn the_derivatives_own_form_is_the_derivative() {
    use replay::iv::{Iv, Unit};
    replay::iv::init();
    replay::iv::set_prec(160);
    let srcs = [
        "sqrt(x-1000)*ln(x-1000)",
        "cbrt(x)*ln(x)",
        "x^3-2x+1/(x-1)",
        "(x^2+x+1)/(3x)",
        "sin(x)/x",
        "tan(x)+sec(x)",
        "x*e^(-x)",
        "e^(-1/x^2)",
        "x^x",
        "atan(x)+acot(x)",
        "asin(x/3)*acos(x/3)",
        "sinh(x)/cosh(x)^2",
        "ln(x)/log(x+1)",
        "(x+1)^(2/3)*root(x+2,5)",
        "asinh(x)-acosh(x+2)+atanh(x/4)",
        "pi*x^2-e*x",
        "sqrt(x^2+1)/(x^2+1)",
        "cos(x)^2*sin(3x)",
    ];
    let points = [0.3, 0.7, 1.3, 2.9, 11.0, 1003.5, -0.6, -2.3];
    let (mut formed, mut compared) = (0, 0);
    for src in srcs {
        let (f, lits) = read(src);
        let Some(d) = replay::algebra::derivative_tree(&f, &lits, &[], Unit::Radians) else {
            continue;
        };
        formed += 1;
        let ctx = replay::eval::Ctx {
            unit: Unit::Radians,
            lits: &lits,
            vars: &[],
        };
        for x in points {
            let s = replay::eval::eval(&f, &replay::series::var(Iv::of(x), 1), 1, &ctx);
            let t = replay::eval::eval(&d, &replay::series::var(Iv::of(x), 0), 0, &ctx);
            if s[1].empty || !s[1].def || t[0].empty || !t[0].def {
                continue;
            }
            assert!(
                !(s[1].hi < t[0].lo || t[0].hi < s[1].lo),
                "{src} at {x}: f′ in [{}, {}], its form in [{}, {}]",
                s[1].lo.to_f64(),
                s[1].hi.to_f64(),
                t[0].lo.to_f64(),
                t[0].hi.to_f64()
            );
            compared += 1;
        }
    }
    assert!(formed >= 15, "only {formed} of {} forms", srcs.len());
    assert!(compared >= 60, "only {compared} comparisons");
    // A step has no tree of its own: no form.
    let (f, lits) = read("floor(x)*x");
    assert!(replay::algebra::derivative_tree(&f, &lits, &[], Unit::Radians).is_none());
}

/// The first monotone piece of `v`, beside the open end at `end`.
fn piece_from(v: &mut serde_json::Value, end: f64) -> &mut serde_json::Value {
    value_of(v, "monotonicity")
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["on"]["lo"]["At"]["x"]["lo"] == serde_json::json!(end))
        .expect("a piece from the open end")
}

/// Review 19's fix kept √(x − 1000)·ln(x − 1000)'s first monotone piece
/// (falling, from the open end 1000, where the claims start one double in)
/// by the one identity (√u·ln u)′ = u′(ln u + 2)/(2√u), matched at the top
/// of the tree: twice that function, or its cube root's, was refused. The
/// sign beside the end is now read off f′'s own form, factor by factor;
/// and the same piece called rising is refused, for each.
#[test]
fn the_slope_beside_an_open_end_is_read_off_the_derivatives_factors() {
    for (src, end) in [
        ("sqrt(x-1000)*ln(x-1000)", 1000.0),
        ("2*sqrt(x-1000)*ln(x-1000)", 1000.0),
        ("cbrt(x-1000)*ln(x-1000)", 1000.0),
        ("(x-7)^(1/4)*ln(x-7)", 7.0),
    ] {
        let base = certificate(src);
        assert_eq!(rejected(&base), None, "{src}");
        let mut v = base.clone();
        let piece = piece_from(&mut v, end);
        assert_eq!(piece["dir"], "Decreasing", "{src}");
        // The claims start at the double after the end: the stretch
        // between is the replay's own to decide.
        let first = end.next_up();
        let from_the_end = claims_of(&mut v, "monotonicity").iter().any(|c| {
            c.to_string().contains("\"F1\"")
                && ["Beyond", "Factors", "NoCross"].iter().any(|k| {
                    c.get(k)
                        .is_some_and(|c| c["x"]["a"].as_f64().is_some_and(|a| a < first))
                })
        });
        assert!(!from_the_end, "{src}: a claim signs f′ from the end itself");
        piece_from(&mut v, end)["dir"] = serde_json::json!("Increasing");
        assert!(
            rejected(&v).is_some(),
            "{src}: falling from its open end, called rising"
        );
    }
}

/// The two plants review 19's fix rewrote (their rows are withheld by the
/// certifier now) are caught for what they plant, not for the row made up
/// around them: the false tail claim is refuted as a claim, and "no least
/// period" on f′ ≡ 0 over the left tail alone is refused as not derived.
#[test]
fn the_rewritten_plants_are_caught_for_what_they_plant() {
    let mut v = certificate("min(x,x+1+abs(x-3))");
    v["monotonicity"] = serde_json::json!({"Certified": {"value": [{
        "on": {"lo": "NegInf", "hi": "PosInf"}, "dir": "Increasing"}],
        "cert": {"covers": "Line", "claims": [{"TailBeyond": {
            "side": "Left", "from": -16.0, "of": {"F": "F1"}, "c": 0.0,
            "above": false}}]}}});
    let tails = judged(&v, "TailBeyond");
    assert!(
        tails.iter().any(|(_, c)| *c == Class::Refuted),
        "f′ < 0 on the left tail of an increasing f: {tails:?}"
    );
    // The same row with the claim true is still refused, for its coverage:
    // the claim's refutation above is what catches the plant.
    v["monotonicity"]["Certified"]["cert"]["claims"][0]["TailBeyond"]["above"] =
        serde_json::json!(true);
    assert!(
        judged(&v, "TailBeyond")
            .iter()
            .all(|(_, c)| *c == Class::Strong)
    );
    assert!(rejected(&v).is_some());

    let mut v = certificate("sign(abs(x)+1)");
    let left = serde_json::json!({"Value": {
        "x": {"a": "-inf", "b": -1.0}, "of": {"F": "F1"}, "lo": 0.0, "hi": 0.0}});
    let period = |claims: serde_json::Value| {
        serde_json::json!({"Certified": {"value": null,
            "cert": {"covers": "Line", "claims": claims}}})
    };
    v["period"] = period(serde_json::json!([left]));
    assert!(judged(&v, "Value").iter().all(|(_, c)| *c == Class::Strong));
    let why = rejected(&v).expect("no least period, on f′ ≡ 0 over the left tail alone");
    assert!(why.starts_with("period: "), "{why}");
    // With f′ ≡ 0 on the whole line, the row follows.
    let line = serde_json::json!({"Value": {
        "x": {"a": "-inf", "b": "inf"}, "of": {"F": "F1"}, "lo": 0.0, "hi": 0.0}});
    v["period"] = period(serde_json::json!([line]));
    assert_eq!(rejected(&v), None);
}

/// A generic kink box decides nothing of f″ either: the dent's two
/// plateaus (f″ ≡ 0, true) and the box between them make no complete list
/// of inflections. (Review 19's test asks this of the rows on f′.)
#[test]
fn a_kink_box_does_not_complete_the_walk_of_the_second_derivative() {
    let mut v = certificate_at("min((x-sin(1))^(2/1000003),0.01)", Some(14), &[]);
    withhold_all_but(&mut v, "domain");
    v["inflections"] = serde_json::json!({"Certified": {"value": [],
    "cert": {"covers": "Line", "claims": [
        {"Value": {"x": {"a": "-inf", "b": 0.8414709848078963},
            "of": {"F": "F2"}, "lo": 0.0, "hi": 0.0}},
        {"Value": {"x": {"a": 0.8414709848078967, "b": "inf"},
            "of": {"F": "F2"}, "lo": 0.0, "hi": 0.0}},
        {"Kink": {"x": {"a": 0.8414709848078963, "b": 0.8414709848078967}}}
    ]}}});
    let r = replay::replay(&v).expect("replayed");
    assert!(
        r.claims.iter().all(|c| c.outcome.class == Class::Strong),
        "these premises really are true"
    );
    let why = rejected(&v).expect("no inflections, by continuity in a kink box");
    assert!(why.contains("undecided"), "{why}");
}

/// A function whose formal derivative is 0 is constant on each stretch
/// where it is continuous, and no further: arctan x + arctan(1/x) is π/2
/// right of 0 and −π/2 left of it, |x|/x is ±1. "Constant on the line" is
/// refused for each; so is ⌊x⌋ across a whole number.
#[test]
fn a_zero_derivative_is_a_constant_on_one_stretch_only() {
    for src in ["atan(x)+atan(1/x)", "abs(x)/x", "floor(x)"] {
        let mut v = certificate(src);
        assert_eq!(rejected(&v), None, "{src}");
        withhold_all_but(&mut v, "domain");
        for (status, covers) in [("Certified", "Line"), ("Partial", "Points")] {
            v["monotonicity"] = serde_json::json!({status: {"value": [{
                "on": {"lo": "NegInf", "hi": "PosInf"}, "dir": "Constant"}],
                "cert": {"covers": covers, "claims": []}}});
            assert!(
                rejected(&v).is_some(),
                "{src}: constant on the line ({status} over {covers})"
            );
        }
    }
    // On one stretch it is: the replay's own algebra and f's continuity
    // there, with no claim.
    let mut v = certificate("atan(x)+atan(1/x)");
    withhold_all_but(&mut v, "domain");
    v["monotonicity"] = serde_json::json!({"Partial": {"value": [{
        "on": {"lo": {"At": {"x": {"lo": 0.0, "hi": 0.0}, "closed": false}}, "hi": "PosInf"},
        "dir": "Constant"}],
        "cert": {"covers": "Points", "claims": []}}});
    assert_eq!(rejected(&v), None);
}

/// A row's status and scope are part of what it says. "All of them"
/// (Certified) over points, a window presented as certified, a partial
/// domain, a y-intercept over the line: none is a form a row takes.
#[test]
fn a_row_off_its_form_is_refused() {
    let base = certificate("x^3-2x+1/(x-1)");
    assert_eq!(rejected(&base), None);
    let window = serde_json::json!({"Window": {"a": -8.0, "b": 8.0}});
    let cases: [(&str, &str, serde_json::Value); 9] = [
        ("x_intercepts", "Certified", serde_json::json!("Points")),
        ("extrema", "Certified", window.clone()),
        ("inflections", "Partial", serde_json::json!("Line")),
        ("monotonicity", "Certified", serde_json::json!("Points")),
        ("vertical", "Partial", serde_json::json!("Line")),
        ("range", "Certified", window.clone()),
        ("domain", "Partial", serde_json::json!("Line")),
        ("y_intercept", "Certified", serde_json::json!("Line")),
        ("oblique", "Certified", serde_json::json!("Points")),
    ];
    for (row, status, covers) in cases {
        let mut v = base.clone();
        let mut body = v[row].as_object().unwrap().values().next().unwrap().clone();
        body["cert"]["covers"] = covers.clone();
        v[row] = serde_json::json!({status: body});
        let why = rejected(&v).unwrap_or_else(|| panic!("{row} {status} over {covers}"));
        assert!(why.contains("status and scope"), "{row}: {why}");
    }
}

/// Bounds that "grow" show no limit. (x + 1)/(x² − 1) is above −10¹⁵,
/// then above −2252, on boxes closing in on its hole at −1, where it tends
/// to −1/2: those bounds passed for an asymptote there. And where a pole
/// is, the replay derives it: (x − 2⁴⁰)/(x − 2⁴⁰ − 1/16)'s certificate
/// carries bounds only.
#[test]
fn bounds_that_grow_are_no_pole() {
    let base = certificate("(x+1)/(x^2-1)");
    assert_eq!(rejected(&base), None);
    let mut v = base.clone();
    value_of(&mut v, "vertical")
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"At": {"lo": -1.0, "hi": -1.0}}));
    let why = rejected(&v).expect("an asymptote at the hole −1");
    assert!(why.contains("no claim it is one"), "{why}");
    // The pole left out of a complete list is refused too.
    let mut v = base.clone();
    value_of(&mut v, "vertical").as_array_mut().unwrap().clear();
    let why = rejected(&v).expect("the pole at 1 left out");
    assert!(why.contains("no asymptote is listed"), "{why}");

    let far = certificate("(x-2^40)/(x-2^40-1/16)");
    assert!(
        judged(&far, "Unbounded").is_empty(),
        "the certificate brings its own proof now: retarget this"
    );
    assert_eq!(rejected(&far), None);
}

/// The range is the images of f's continuous stretches, no more and no
/// less: the row was checked at its ends and at sample points only.
#[test]
fn the_range_is_the_images_of_the_stretches() {
    let neg_inf = serde_json::json!("NegInf");
    // √x takes no value below 0.
    let mut v = certificate("sqrt(x)");
    assert_eq!(rejected(&v), None);
    value_of(&mut v, "range")[0]["lo"] = neg_inf.clone();
    v["range_ends"][0][0] = serde_json::json!(["Other"]);
    let why = rejected(&v).expect("√x's range from −∞");
    assert!(why.contains("not shown to take"), "{why}");
    // x³ − 2x + 1/(x − 1): the gap between its two pieces closed.
    let mut v = certificate("x^3-2x+1/(x-1)");
    assert_eq!(rejected(&v), None);
    value_of(&mut v, "range")[1]["lo"] = neg_inf.clone();
    v["range_ends"][1][0] = serde_json::json!(["Other"]);
    let why = rejected(&v).expect("its range's upper piece from −∞");
    assert!(why.contains("not shown to take"), "{why}");
    // A piece left out: f takes values outside the list.
    let mut v = certificate("x^3-2x+1/(x-1)");
    value_of(&mut v, "range").as_array_mut().unwrap().remove(0);
    v["range_ends"].as_array_mut().unwrap().remove(0);
    let why = rejected(&v).expect("a piece of the range left out");
    assert!(why.contains("outside the listed range"), "{why}");
    // x²'s least value is taken: an open end there is refused whatever
    // source it names; tan x·cos x only tends to ±1.
    let mut v = certificate("x^2");
    assert_eq!(rejected(&v), None);
    value_of(&mut v, "range")[0]["lo"]["At"]["closed"] = serde_json::json!(false);
    v["range_ends"][0][0] = serde_json::json!([{"Tail": "Left"}]);
    assert!(rejected(&v).is_some(), "x²'s range open at 0");
    let mut v = certificate("tan(x)*cos(x)");
    assert_eq!(rejected(&v), None);
    value_of(&mut v, "range")[0]["hi"]["At"]["closed"] = serde_json::json!(true);
    assert!(
        rejected(&v).is_some(),
        "sin's range closed at 1, its turns excluded"
    );
}

/// A complete list decides what it leaves out, with whatever claims it is
/// given: each of these passed on the claims of the certificate's other
/// rows, or on none.
#[test]
fn a_complete_list_decides_what_it_leaves_out() {
    let empty = |claims: serde_json::Value| {
        serde_json::json!({"Certified": {"value": [],
            "cert": {"covers": "Line", "claims": claims}}})
    };
    // √x's minimum at its closed end, with the monotone row's claims.
    let mut v = certificate("sqrt(x)");
    let mono = claims_of(&mut v, "monotonicity").clone();
    v["extrema"] = empty(serde_json::json!(mono));
    let why = rejected(&v).expect("√x with no extremum");
    assert!(why.starts_with("extrema: "), "{why}");
    // arctan's two horizontal asymptotes.
    let mut v = certificate("atan(x)");
    let own = claims_of(&mut v, "horizontal").clone();
    v["horizontal"] = empty(serde_json::json!(own));
    let why = rejected(&v).expect("arctan with no horizontal asymptote");
    assert!(why.contains("nothing rules one out"), "{why}");
    // No vertical asymptote, on no claim.
    let mut v = certificate("x^3-2x+1/(x-1)");
    v["vertical"] = empty(serde_json::json!([]));
    assert!(rejected(&v).is_some(), "no vertical asymptote, on no claim");
    // tan's poles, one listed alone in a row over one period.
    let mut v = certificate("tan(x)");
    assert_eq!(rejected(&v), None);
    let item = &mut value_of(&mut v, "vertical")[0];
    let x0 = item["Every"]["x0"].clone();
    *item = serde_json::json!({"At": x0});
    let why = rejected(&v).expect("one pole of tan, alone");
    assert!(why.contains("listed alone"), "{why}");
    // sin is "not periodic" by its bounds far out.
    let mut v = certificate("sin(x)");
    v["period"] = serde_json::json!({"Certified": {"value": null,
    "cert": {"covers": "Line", "claims": [
        {"TailValue": {"side": "Right", "from": 16.0, "lo": -1.0, "hi": 1.0}},
        {"Value": {"x": {"a": 0.0, "b": 0.0}, "of": {"F": "F0"}, "lo": 0.0, "hi": 0.0}},
        {"Value": {"x": {"a": 1.0, "b": 1.0}, "of": {"F": "F0"},
            "lo": 0.8414709848078965, "hi": 0.8414709848078966}}
    ]}}});
    assert!(
        rejected(&v).is_some(),
        "sin not periodic, by a band and two values"
    );
}

/// A kink box (`Kink`) and what f⁽ᵏ⁾ does on each side of it
/// (`KinkSides`), as the certifier writes them, set into `row` of `v`
/// (Certified over the line).
fn kink_row(
    v: &mut serde_json::Value,
    row: &str,
    value: serde_json::Value,
    claims: serde_json::Value,
) {
    v[row] = serde_json::json!({"Certified": {"value": value,
        "cert": {"covers": "Line", "claims": claims}}});
}

fn sides(
    x: (f64, f64),
    at: Option<f64>,
    k: u8,
    l: &str,
    r: &str,
    joined: bool,
) -> serde_json::Value {
    serde_json::json!({"KinkSides": {"x": {"a": x.0, "b": x.1}, "at": at, "order": k,
        "left": l, "right": r, "joined": joined}})
}

/// The class of each `KinkSides` claim of `v`.
fn kink_sides_judged(v: &serde_json::Value) -> Vec<Class> {
    judged(v, "KinkSides").into_iter().map(|(_, c)| c).collect()
}

/// Round 19: `KinkSides`, what f′ or f″ does on each side of the one point
/// of a kink box where it need not exist, checked from its statement (f
/// continuous on the box; the kink arguments' signs on each side, or the
/// one that vanishes once inside an unplaced box; f⁽ᵏ⁾ on the one-sided
/// forms, the point left out; with `joined`, the branches' slopes equal
/// there). The rows the certifier gave up for a kink box it couldn't
/// place (abs(x − 0.1)'s minimum, monotone pieces and no inflections;
/// x·|x|'s inflection) come back on it, and the false versions are
/// refused. (This branch's certifier doesn't write the claim: these rows
/// are set in by hand, as the certifier's branch writes them.)
#[test]
fn the_sides_of_a_kink_decide_the_rows_on_its_derivatives() {
    let bx = (0.09999999999999998, 0.10000000000000002);
    let kink = serde_json::json!({"Kink": {"x": {"a": bx.0, "b": bx.1}}});
    let tails = |k: &str, left: bool, right: bool| {
        serde_json::json!([
            {"TailBeyond": {"side": "Left", "from": bx.0, "of": {"F": k}, "c": 0.0, "above": left}},
            {"TailBeyond": {"side": "Right", "from": bx.1, "of": {"F": k}, "c": 0.0, "above": right}}
        ])
    };
    let with = |a: serde_json::Value, more: &[serde_json::Value]| {
        let mut a = a.as_array().unwrap().clone();
        a.extend(more.iter().cloned());
        serde_json::Value::Array(a)
    };
    let base = certificate("abs(x-0.1)");
    let min_at = serde_json::json!([{"x": {"lo": bx.0, "hi": bx.1},
        "y": {"lo": 0.0, "hi": 2.7755575615628914e-17}, "kind": "Min", "every": null}]);
    let value_there = serde_json::json!({"Value": {"x": {"a": bx.0, "b": bx.1}, "of": {"F": "F0"},
        "lo": 0.0, "hi": 2.7755575615628914e-17}});
    let end = serde_json::json!({"At": {"x": {"lo": bx.0, "hi": bx.1}, "closed": true}});
    let pieces = serde_json::json!([{"on": {"lo": "NegInf", "hi": end}, "dir": "Decreasing"},
        {"on": {"lo": end, "hi": "PosInf"}, "dir": "Increasing"}]);
    let flat = serde_json::json!([
        {"Value": {"x": {"a": "-inf", "b": bx.0}, "of": {"F": "F2"}, "lo": 0.0, "hi": 0.0}},
        {"Value": {"x": {"a": bx.1, "b": "inf"}, "of": {"F": "F2"}, "lo": 0.0, "hi": 0.0}}
    ]);
    let turn = sides(bx, None, 1, "Neg", "Pos", false);
    let mut v = base.clone();
    kink_row(
        &mut v,
        "extrema",
        min_at.clone(),
        with(
            tails("F1", false, true),
            &[value_there.clone(), kink.clone(), turn.clone()],
        ),
    );
    kink_row(
        &mut v,
        "monotonicity",
        pieces.clone(),
        with(tails("F1", false, true), &[kink.clone(), turn.clone()]),
    );
    kink_row(
        &mut v,
        "inflections",
        serde_json::json!([]),
        with(
            flat.clone(),
            &[kink.clone(), sides(bx, None, 2, "Zero", "Zero", false)],
        ),
    );
    assert_eq!(rejected(&v), None, "abs(x − 0.1)'s rows on its kink box");
    assert!(kink_sides_judged(&v).iter().all(|c| *c == Class::Strong));
    // The box alone decides nothing (review 19's rule stands).
    let mut w = v.clone();
    claims_of(&mut w, "extrema").retain(|c| c.get("KinkSides").is_none());
    assert!(
        rejected(&w)
            .expect("a minimum on the kink box alone")
            .contains("undecided")
    );
    // The sides swapped: the claim is refuted, whatever the row says.
    let mut w = v.clone();
    *claims_of(&mut w, "extrema").last_mut().unwrap() = sides(bx, None, 1, "Pos", "Neg", false);
    value_of(&mut w, "extrema")[0]["kind"] = serde_json::json!("Max");
    let judged = kink_sides_judged(&w);
    assert_eq!(
        judged.iter().filter(|c| **c == Class::Refuted).count(),
        1,
        "{judged:?}"
    );
    // Placed at the double 0.1, which is past the kink (1/10 lies below
    // it): x − 0.1 is not of one sign on the left of it. Not shown.
    let mut w = v.clone();
    *claims_of(&mut w, "extrema").last_mut().unwrap() =
        sides(bx, Some(0.1), 1, "Neg", "Pos", false);
    assert!(rejected(&w).is_some(), "the kink placed at the double 0.1");
    // f″ is 0 on each branch: a change of its sign there is refuted.
    let mut w = v.clone();
    *claims_of(&mut w, "inflections").last_mut().unwrap() = sides(bx, None, 2, "Neg", "Pos", false);
    assert!(kink_sides_judged(&w).contains(&Class::Refuted));
    // The statement's own bounds: the point inside the box, an order of 1
    // or 2, joined only at a point.
    for bad in [
        sides(bx, Some(0.2), 1, "Neg", "Pos", false),
        sides(bx, None, 3, "Neg", "Pos", false),
        sides(bx, None, 1, "Neg", "Pos", true),
    ] {
        let mut w = v.clone();
        *claims_of(&mut w, "extrema").last_mut().unwrap() = bad.clone();
        assert!(kink_sides_judged(&w).contains(&Class::Refuted), "{bad}");
    }

    // x·|x| at 0: f″ = −2 then 2, f′ = 2|x| continuous: an inflection,
    // because joined. Not joined, it is a corner no complete row passes.
    let zero = (-5e-324, 5e-324);
    let mut v = certificate("x*abs(x)");
    let at0 = serde_json::json!([{"x": {"lo": 0.0, "hi": 0.0}, "y": {"lo": 0.0, "hi": 0.0}, "every": null}]);
    let claims = |joined: bool| {
        serde_json::json!([
            {"TailBeyond": {"side": "Left", "from": zero.0, "of": {"F": "F2"}, "c": 0.0, "above": false}},
            {"TailBeyond": {"side": "Right", "from": zero.1, "of": {"F": "F2"}, "c": 0.0, "above": true}},
            {"Value": {"x": {"a": 0.0, "b": 0.0}, "of": {"F": "F0"}, "lo": 0.0, "hi": 0.0}},
            {"Kink": {"x": {"a": zero.0, "b": zero.1}}},
            sides(zero, Some(0.0), 2, "Neg", "Pos", joined)
        ])
    };
    kink_row(&mut v, "inflections", at0.clone(), claims(true));
    assert_eq!(rejected(&v), None, "x·|x|'s inflection at 0");
    assert!(kink_sides_judged(&v).iter().all(|c| *c == Class::Strong));
    kink_row(&mut v, "inflections", at0, claims(false));
    assert!(
        rejected(&v)
            .expect("a corner taken for an inflection")
            .contains("kink")
    );
    // |x| is no inflection however joined it is said to be: its branches'
    // slopes at 0 are −1 and 1.
    let mut v = certificate("abs(x)");
    kink_row(
        &mut v,
        "inflections",
        serde_json::json!([]),
        serde_json::json!([
            {"Value": {"x": {"a": "-inf", "b": zero.0}, "of": {"F": "F2"}, "lo": 0.0, "hi": 0.0}},
            {"Value": {"x": {"a": zero.1, "b": "inf"}, "of": {"F": "F2"}, "lo": 0.0, "hi": 0.0}},
            {"Kink": {"x": {"a": zero.0, "b": zero.1}}},
            sides(zero, Some(0.0), 2, "Zero", "Zero", false)
        ]),
    );
    assert_eq!(rejected(&v), None, "|x| has no inflection");
    *claims_of(&mut v, "inflections").last_mut().unwrap() =
        sides(zero, Some(0.0), 2, "Zero", "Zero", true);
    assert!(
        kink_sides_judged(&v).contains(&Class::Refuted),
        "|x|'s branches joined at 0"
    );

    // A vertical tangent no double holds, (x − 0.1)^(1/3): f′ > 0 on both
    // sides, read off f′'s own form ∛u/(3u), each factor of u's sign. The
    // other sign is refuted.
    let mut v = certificate("(x-0.1)^(1/3)");
    let rising = serde_json::json!([{"on": {"lo": "NegInf", "hi": "PosInf"}, "dir": "Increasing"}]);
    kink_row(
        &mut v,
        "monotonicity",
        rising.clone(),
        with(
            tails("F1", true, true),
            &[kink.clone(), sides(bx, None, 1, "Pos", "Pos", false)],
        ),
    );
    assert_eq!(rejected(&v), None, "(x − 0.1)^(1/3) increasing");
    assert!(kink_sides_judged(&v).iter().all(|c| *c == Class::Strong));
    *claims_of(&mut v, "monotonicity").last_mut().unwrap() =
        sides(bx, None, 1, "Neg", "Neg", false);
    assert!(kink_sides_judged(&v).contains(&Class::Refuted));

    // max(x², 2 − x², −5): its kinks at ±1 are corners of a max of three:
    // the argument it picks on each side, from every pair's difference.
    let mut v = certificate("max(x^2,2-x^2,-5)");
    withhold_all_but(&mut v, "domain");
    let one = (-1.0000000000000002, -0.9999999999999999);
    for (l, r, class) in [
        ("Neg", "Pos", Class::Strong),
        ("Pos", "Neg", Class::Refuted),
    ] {
        kink_row(
            &mut v,
            "extrema",
            serde_json::json!([]),
            serde_json::json!([
                {"Kink": {"x": {"a": one.0, "b": one.1}}},
                sides(one, Some(-1.0), 1, l, r, false)
            ]),
        );
        assert_eq!(kink_sides_judged(&v), [class], "{l}, {r}");
    }
}

/// FUZZ's seed 1409: x^(1/(0.1 + 0.2 − 0.3 + 10^(−400))) is x^(10^400),
/// the typed decimals adding up exactly. The replay worked the constant
/// term by term in intervals, which straddle 0 about 10^(−400), and
/// refuted the true f(0) = 0. A constant the exact reader works out is that
/// rational, enclosed.
#[test]
fn a_constant_is_worked_exactly_as_typed() {
    for src in [
        "x^(1/(0.1+0.2-0.3+10^(-400)))",
        "x^(1/(10^(-400)))",
        "(0.1+0.2-0.3+10^(-400))*10^400*x",
    ] {
        let mut v = certificate(src);
        assert_eq!(rejected(&v), None, "{src}");
        v["y_intercept"] = serde_json::json!({"Certified": {"value": {"lo": 0.0, "hi": 0.0},
        "cert": {"covers": "Points", "claims": [
            {"Value": {"x": {"a": 0.0, "b": 0.0}, "of": {"F": "F0"}, "lo": 0.0, "hi": 0.0}}
        ]}}});
        let at0 = judged(&v, "Value { x: (0.0, 0.0), of: F(0), lo: 0.0, hi: 0.0 }");
        assert!(
            !at0.is_empty() && at0.iter().all(|(_, c)| *c == Class::Strong),
            "{src}: f(0) = 0: {at0:?}"
        );
        assert_eq!(rejected(&v), None, "{src}");
    }
}

/// FUZZ's seed 1409, cases 1079 and 3120: a constant f's range is its one
/// value. asin x − asin x is 0 where defined, but its enclosure at a point
/// is ±10⁻⁴⁸ (the two terms rounded apart), which no listed [0, 0] holds;
/// round(1000001) at 5 digits is a step of a constant, no step function.
/// The range reads f's value through its reduced form (the replay's field:
/// 0 and 10⁶ exactly).
#[test]
fn a_constant_ranges_over_its_one_value() {
    for (src, digits) in [
        ("asin(x)-asin(x)", None),
        ("asin(x)-asin(x)", Some(14)),
        ("round(1000001)-(asin(x)-asin(x))", Some(5)),
        ("round(1000001)-(asin(x)-asin(x))", None),
    ] {
        let v = certificate_with(src, digits);
        // (Its range row; f″ at the domain's ends, which asin's form has
        // no value for, is another matter: a claim left open.)
        let r = replay::replay(&v).expect("replayed");
        let range = r
            .rows
            .iter()
            .find(|row| row.name == "range")
            .expect("a range row");
        assert!(
            range.status != "Unknown" && range.problems.is_empty(),
            "{src} at {digits:?} digits: {:?}",
            range.problems
        );
    }
}

/// FUZZ's seed 1451, case 66: 0.3·x is no periodic function, and its
/// period row says so on the fact "f = N/D exactly: f → −∞ as x → −∞",
/// which the replay's algebra decides exactly. Bounds were taken out of
/// "f goes to ±∞ on a tail" this round, and this exact fact with them;
/// it is a proof, and is read as one.
#[test]
fn a_rational_f_going_to_infinity_is_not_periodic() {
    for src in ["0.30000000000000004*(x^2-x^2+x)", "x^3-x"] {
        let v = certificate(src);
        let r = replay::replay(&v).expect("replayed");
        let period = r
            .rows
            .iter()
            .find(|row| row.name == "period")
            .expect("a period row");
        assert!(period.problems.is_empty(), "{src}: {:?}", period.problems);
    }
}

/// An exact label is equality, including when no double can distinguish
/// it from the true limit. A true Limit claim used to let any rational
/// meeting the replay's 160-bit enclosure of π/2 stand for π/2 itself.
#[test]
fn an_exact_asymptote_is_not_decided_by_overlapping_enclosures() {
    // Genuine composed and scaled limits need an exact derivation too:
    // their general limit claims contain only enclosures, not exact facts.
    for src in [
        "(x/1000)^2/((x/1000)^2-1)",
        "((x/1000)^2+1)/((x/1000)^2-1)",
        "atan(ln(abs(x)))",
        "atan(ln(abs((x-1000))))",
        "-3*(atan(ln(abs(x))))",
        "atan(ln(abs((x/1000))))",
    ] {
        for unit in [TrigUnit::Radians, TrigUnit::Degrees, TrigUnit::Grads] {
            let mut v = certificate_in(src, None, unit);
            // This control tests horizontal exactness. Other rows can
            // have independent availability limitations beside a hole.
            let domain = v["domain"].clone();
            withhold_all_but(&mut v, "horizontal");
            v["domain"] = domain;
            assert_eq!(
                rejected(&v),
                None,
                "the exact composed limit: {src} {unit:?}"
            );
        }
    }
    let mut v = certificate("atan(x)");
    let y = value_of(&mut v, "horizontal")
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["side"] == "Right")
        .unwrap()["y"]
        .clone();
    claims_of(&mut v, "horizontal").push(serde_json::json!({"Limit": {
        "at": "PosInf", "over_x": false, "to": {"In": y}
    }}));
    assert_eq!(
        rejected(&v),
        None,
        "the original row and added premise are true"
    );
    let right = |v: &mut serde_json::Value, text: &str| {
        let h = value_of(v, "horizontal")
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|h| h["side"] == "Right")
            .unwrap();
        h["exact"] = serde_json::json!(text);
    };
    let mut equivalent = v.clone();
    right(&mut equivalent, "2/4·π");
    assert_eq!(rejected(&equivalent), None, "equal normalized exact forms");
    for false_exact in [
        // This rational is unequal to the irrational π/2, but within 10⁻⁶⁰.
        "1570796326794896619231321691639751442098584699687552910487472296/1000000000000000000000000000000000000000000000000000000000000000",
        // A different rational coefficient of the same power of π.
        "500000000000000000000000000000000000000000000000000000000000000001/1000000000000000000000000000000000000000000000000000000000000000000·π",
    ] {
        let mut wrong = v.clone();
        right(&mut wrong, false_exact);
        let r = replay::replay(&wrong).expect("replayed");
        assert!(
            r.claims
                .iter()
                .all(|c| matches!(c.outcome.class, Class::Strong | Class::Weak)),
            "the mathematical premises were left unchanged"
        );
        assert!(
            r.rows
                .iter()
                .any(|r| r.name == "horizontal" && !r.problems.is_empty()),
            "a different exact number must not stand for the limit: {false_exact}"
        );
    }
}

/// No double lies between 0 and 10⁻⁴⁰⁰, but each is a pole. The genuine
/// domain lists both families; a complete vertical row listing only the
/// integers used to pass because its gap beside undefined 0 was ignored.
#[test]
fn a_gap_beside_an_undefined_double_can_hold_another_pole() {
    let tiny = f64::from_bits(1);
    for (src, poles, correct) in [
        ("cot(180*x)", true, true),
        ("cot(180*x)+cot(180*(x-10^(-400)))", true, false),
        ("sin(180*x)/sin(180*x)", false, true),
    ] {
        let mut v = certificate_in(src, None, TrigUnit::Degrees);
        withhold_all_but(&mut v, "domain");
        let mut claims = vec![
            serde_json::json!({"Value": {"x": {"a": -0.5, "b": -tiny},
                "of": {"F": "F0"}, "lo": "-inf", "hi": "inf"}}),
            serde_json::json!({"Value": {"x": {"a": 2.0*tiny, "b": 0.5},
                "of": {"F": "F0"}, "lo": "-inf", "hi": "inf"}}),
            serde_json::json!({"Undefined": {"x": {"a": 0.0, "b": 0.0}}}),
            serde_json::json!({"Gap": {"x": {"a": -tiny, "b": 0.0}}}),
            serde_json::json!({"Gap": {"x": {"a": 0.0, "b": 2.0*tiny}}}),
            serde_json::json!({"Simplifier": {"fact": "f(x + 1) = f(x) on its domain"}}),
        ];
        let value = if poles {
            claims.push(serde_json::json!({"Unbounded": {
                "near": {"a": -tiny, "b": 0.0}, "at": {"a": 0.0, "b": 0.0}}}));
            serde_json::json!([{"Every": {"x0": {"lo": 0.0, "hi": 0.0},
                "period": {"lo": 1.0, "hi": 1.0}}}])
        } else {
            serde_json::json!([])
        };
        v["vertical"] = serde_json::json!({"Certified": {"value": value,
            "cert": {"covers": {"Period": {"a": -0.5, "b": 0.5}}, "claims": claims}}});
        let r = replay::replay(&v).expect("replayed");
        assert!(
            r.claims
                .iter()
                .all(|c| matches!(c.outcome.class, Class::Strong | Class::Weak)),
            "{src}: every premise is true"
        );
        let why = rejected(&v);
        assert_eq!(why.is_none(), correct, "{src}: {why:?}");
    }
    // Horner's value-preserving rearrangement changes u²+u⁰ to u²+1
    // where defined, but removes the u=0 hole. Its bounded value may
    // rule out an asymptote, never prove the original function defined.
    let mut v = certificate("1/x+((x-10^(-400))^2+(x-10^(-400))^0)");
    withhold_all_but(&mut v, "domain");
    let at = serde_json::json!({"At": {"x": {"lo": 0.0, "hi": 0.0}, "closed": false}});
    v["domain"] = serde_json::json!({"Certified": {"value": {"pieces": [
        {"lo": "NegInf", "hi": at.clone()}, {"lo": at, "hi": "PosInf"}], "excluded": []},
        "cert": {"covers": "Line", "claims": []}}});
    assert!(
        rejected(&v).is_some(),
        "the domain must also exclude 10⁻⁴⁰⁰"
    );
}

/// `src`'s certificate (radians) with only its domain and the row `row`
/// borrowed from cot x's: the same boxes and facts, the Family claims
/// those of `src`'s own domain, and f unbounded at 0 claimed from 0 itself
/// (the second pole, at 10⁻⁴⁰⁰, lies in any neighbourhood of 0).
fn cot_row_on(src: &str, row: &str) -> serde_json::Value {
    let mut v = certificate(src);
    let donor = certificate("cot(x)");
    let fams: Vec<serde_json::Value> = claims_of(&mut v, "domain").clone();
    withhold_all_but(&mut v, "domain");
    v[row] = donor[row].clone();
    if row == "range" {
        v["range_ends"] = donor["range_ends"].clone();
    }
    let claims = claims_of(&mut v, row);
    claims.retain(|c| c.get("Family").is_none());
    for c in claims.iter_mut() {
        if c.get("Unbounded").is_some() {
            *c = serde_json::json!({"Unbounded": {
                "near": {"a": 0.0, "b": 0.0}, "at": {"a": 0.0, "b": 0.0}}});
        }
    }
    claims.extend(fams);
    if row == "monotonicity" {
        // (Up to π: the piece's far end, beside the next pole.)
        let (pl, ph) = (std::f64::consts::PI, std::f64::consts::PI.next_up());
        let g = serde_json::json!({"a": pl.next_down(), "b": ph});
        claims.push(serde_json::json!({"Beyond": {
            "x": {"a": 1.97920337176157, "b": pl.next_down()},
            "of": {"F": "F1"}, "c": 0.0, "above": false}}));
        claims.push(serde_json::json!({"Gap": {"x": g}}));
        claims.push(serde_json::json!({"GapClear": {"x": g, "of": {"F": "F1"}}}));
    }
    v
}

/// Why the replay refuses `v`, every claim of which it proves (strongly or
/// weakly): the rows alone, not following from true premises.
fn refused_on_true_premises(v: &serde_json::Value, what: &str) -> Option<String> {
    let r = replay::replay(v).expect("replayed");
    assert!(
        r.claims
            .iter()
            .all(|c| matches!(c.outcome.class, Class::Strong | Class::Weak)),
        "{what}: every premise is true"
    );
    r.row_problems().into_iter().next()
}

/// A gap about or beside a point f is undefined at, or one a pole is
/// placed in, stood for that one point. cot x + cot(x − 10⁻⁴⁰⁰) has a pole
/// at 10⁻⁴⁰⁰ + kπ too, in the gap about 0: cot x's vertical row passed for
/// it (round 20's checker audit, after R20-M-03's gap beside 0). With
/// holes at 10⁻⁴⁰⁰ + kπ instead (cot x times sin(x − 10⁻⁴⁰⁰) over itself),
/// cot x's monotone piece (0, π) holds the hole, and its range ℝ takes
/// every value but cot 10⁻⁴⁰⁰: both passed, the gap about 0 cut as one
/// excluded point. The genuine domains list both families.
#[test]
fn a_gap_holds_one_excluded_point_only_where_shown() {
    let v = cot_row_on("cot(x)+cot(x-10^(-400))", "vertical");
    assert!(
        refused_on_true_premises(&v, "two poles").is_some(),
        "a second pole in the gap about 0"
    );
    for row in ["monotonicity", "range"] {
        let v = cot_row_on("cot(x)*(sin(x-10^(-400))/sin(x-10^(-400)))", row);
        assert!(
            refused_on_true_premises(&v, row).is_some(),
            "a hole in the gap about 0: {row}"
        );
    }
    // (cot x's own rows stand: the pool's certificate of it.)
}

/// A gap is no excluded point by itself: f = x listed as ℝ ∖ {0}, with a
/// gap about 0 and its value there claimed as a removable point's, passed
/// (the stretch was cut at the gap, the value 0 taken for a limit only).
/// Round 20's checker audit.
#[test]
fn a_range_is_cut_only_where_f_is_excluded() {
    let mut v = certificate("x");
    let domain = v["domain"].clone();
    withhold_all_but(&mut v, "range");
    v["domain"] = domain;
    let g = serde_json::json!({"a": -5e-324, "b": 5e-324});
    claims_of(&mut v, "range").extend([
        serde_json::json!({"Gap": {"x": g}}),
        serde_json::json!({"GapClear": {"x": g, "of": {"F": "F1"}}}),
        serde_json::json!({"Removable": {"near": g, "at": {"a": 0.0, "b": 0.0},
            "lo": 0.0, "hi": 0.0}}),
    ]);
    assert_eq!(rejected(&v), None, "the genuine range, a true gap added");
    let open = serde_json::json!({"At": {"x": {"lo": 0.0, "hi": 0.0}, "closed": false}});
    *value_of(&mut v, "range") = serde_json::json!([
        {"lo": "NegInf", "hi": open.clone()}, {"lo": open, "hi": "PosInf"}]);
    let hole = serde_json::json!([{"Hole": {"lo": 0.0, "hi": 0.0}}]);
    v["range_ends"] = serde_json::json!([[[], hole], [hole, []]]);
    assert!(
        refused_on_true_premises(&v, "x as ℝ ∖ {0}").is_some(),
        "0 is f(0)"
    );
}

/// Horner's form of (x − c)² + (x − c)⁰ read the 0th power as 1, and f's
/// definedness on it stood for f's: a domain of all ℝ passed, with no
/// claims, though 0⁰ is undefined at c = 10⁻⁴⁰⁰. Round 20's checker audit.
#[test]
fn a_zeroth_power_keeps_its_hole() {
    let mut v = certificate("(x-10^(-400))^2+(x-10^(-400))^0");
    withhold_all_but(&mut v, "domain");
    v["domain"] = serde_json::json!({"Certified": {"value": {"pieces": [
        {"lo": "NegInf", "hi": "PosInf"}], "excluded": []},
        "cert": {"covers": "Line", "claims": []}}});
    assert!(rejected(&v).is_some(), "f is undefined at 10⁻⁴⁰⁰");
}

/// A family is its own mirror when shown so, not when −x₀'s enclosure
/// meets one of its members: x + 0·cot(180(x − 10⁻⁴⁰⁰)) is undefined at
/// 10⁻⁴⁰⁰ + k only, x₀ enclosed in [0, 10⁻³²³]; called odd (a fact true
/// where both sides are defined), it passed. Round 20's checker audit.
#[test]
fn a_family_is_symmetric_only_when_shown() {
    let mut v = certificate_in("x+0*cot(180*(x-10^(-400)))", None, TrigUnit::Degrees);
    withhold_all_but(&mut v, "domain");
    parity_called(&mut v, "Odd", ODD);
    assert!(
        refused_on_true_premises(&v, "the domain and the fact").is_some(),
        "the domain is not symmetric"
    );
    // tan's poles are: cos x is even.
    let mut v = certificate("tan(x)");
    withhold_all_but(&mut v, "domain");
    parity_called(&mut v, "Odd", ODD);
    assert_eq!(rejected(&v), None, "tan x is odd");
}

/// A monotone piece's open high end is left out of the bridge to it, as a
/// low one was: atan(ln|x|) in degrees and grads, its partial piece
/// (−∞, 0) undecided on [−5·10⁻³²⁴, 0] (review 20, Questions), replays
/// whole. Called increasing there, it is refused.
#[test]
fn an_open_high_end_is_left_out_of_the_bridge() {
    for unit in [TrigUnit::Degrees, TrigUnit::Grads] {
        let mut v = certificate_in("atan(ln(abs(x)))", None, unit);
        assert_eq!(rejected(&v), None, "{unit:?}");
        let pieces = value_of(&mut v, "monotonicity").as_array_mut().unwrap();
        let p = pieces
            .iter_mut()
            .find(|p| p["on"]["lo"] == "NegInf")
            .expect("the piece from −∞");
        p["dir"] = serde_json::json!("Increasing");
        assert!(rejected(&v).is_some(), "{unit:?}: it decreases");
    }
}

/// An exact limit through a step is derived too: atan(ceil x) → ±π/2, ceil
/// x within 1 of x (crossfuzz's seed 2081, case 3598, and seed 2063, case
/// 1859, atan(floor(x·1.00000000000000000001)): their genuine labels were
/// refused once overlap stopped standing for equality). A coefficient
/// 10⁻⁶³ off is not that limit.
#[test]
fn an_exact_limit_through_a_step_is_derived() {
    for (src, unit) in [
        ("atan(floor(x*1.00000000000000000001))", TrigUnit::Radians),
        ("atan(floor(x*1.00000000000000000001))", TrigUnit::Degrees),
    ] {
        let mut v = certificate_in(src, None, unit);
        let domain = v["domain"].clone();
        withhold_all_but(&mut v, "horizontal");
        v["domain"] = domain;
        assert_eq!(rejected(&v), None, "{src} {unit:?}: the genuine labels");
    }
    let mut v = certificate("atan(ceil(x))");
    let domain = v["domain"].clone();
    withhold_all_but(&mut v, "horizontal");
    v["domain"] = domain;
    assert_eq!(rejected(&v), None, "the genuine labels");
    let h = value_of(&mut v, "horizontal")
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|h| h["side"] == "Right")
        .unwrap();
    h["exact"] = serde_json::json!(
        "500000000000000000000000000000000000000000000000000000000000001/\
         1000000000000000000000000000000000000000000000000000000000000000·π"
    );
    assert!(rejected(&v).is_some(), "a nearby coefficient");
}

/// One listed item is one number. Zeros at 0.4 and 0.6 listed as one item
/// over both, and monotone pieces meeting at one enclosure that holds both
/// turns of (x − 0.5)³ − 0.01(x − 0.5) (its decreasing piece left out),
/// passed as complete rows, every claim the genuine one. Round 20's
/// checker audit.
#[test]
fn one_listed_item_stands_for_one_place() {
    let mut v = certificate("(x-0.4)*(x-0.6)");
    let xs = value_of(&mut v, "x_intercepts").as_array().unwrap().clone();
    assert_eq!(xs.len(), 2);
    let (a, b) = (&xs[0]["At"], &xs[1]["At"]);
    *value_of(&mut v, "x_intercepts") = serde_json::json!([{"At": {"lo": a["lo"], "hi": b["hi"]}}]);
    assert!(rejected(&v).is_some(), "two zeros listed as one");
    let mut v = certificate("(x-0.5)^3-0.01*(x-0.5)");
    let ps = value_of(&mut v, "monotonicity").as_array().unwrap().clone();
    assert_eq!(ps.len(), 3);
    let e = serde_json::json!({"lo": ps[0]["on"]["hi"]["At"]["x"]["lo"],
        "hi": ps[2]["on"]["lo"]["At"]["x"]["hi"]});
    let end = serde_json::json!({"At": {"x": e, "closed": true}});
    *value_of(&mut v, "monotonicity") = serde_json::json!([
        {"on": {"lo": "NegInf", "hi": end}, "dir": "Increasing"},
        {"on": {"lo": end, "hi": "PosInf"}, "dir": "Increasing"}]);
    assert!(rejected(&v).is_some(), "two turns in one end");
}
