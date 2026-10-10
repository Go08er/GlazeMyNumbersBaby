//! A random cross-checking tester of the graphing engine's views of one
//! function: the compiled point evaluator, the typed reference evaluator,
//! the interval form with its decorations, the certifier and the panel,
//! the plotter, and (with `--features mpfr-oracle`) the independent MPFR
//! replay of the certificate and that checker's own evaluation of f.
//!
//! The broad tests (`examples/sweep.rs`, `examples/oracle.rs`, the certify
//! corpus and its replay) take ordinary functions, shifted and scaled. The
//! review rounds 12–18 found bugs in a few families they don't reach;
//! this generates expressions biased toward those (`crossfuzz/generate.rs`):
//!
//! 1. **typed-literal semantics**: decimals near an integer and very long
//!    ones, different numbers sharing one double, literal arithmetic that
//!    folds exactly, integers past 2⁵³ as exponents, root degrees and
//!    count arguments, written ratios `p/q` of any size, parity after the
//!    digit limit's rounding, sliders read as decimals;
//! 2. **definedness lost in shortcuts**: a sub-expression undefined
//!    everywhere or at points inside an integer power, a root, min/max, a
//!    step, `u^0`, `nCr(u, 0)`, a product with 0;
//! 3. **counting functions and roots at extremes**;
//! 4. **display**: numbers a hair apart;
//! 5. **two spellings of one function** (`p/q` and the ratio reduced,
//!    `x^(a+b)` and `x^c`, `u − u + f` and `f`, literal arithmetic folded
//!    by hand),
//!
//! each inside ordinary blocks, under a random number-precision setting
//! (5, 14, 15, 16, 20 digits or off), with and without sliders, mostly in
//! radians. Then every pair of views that should agree is checked
//! (`crossfuzz/views.rs` lists the checks); the certified panel against f
//! sampled by the oracle's own checks (`examples/oracle/checks.rs`).
//!
//! ```text
//! cargo test -p graphing --test crossfuzz                         # the default run
//! cargo test --release -p graphing --features mpfr-oracle --test crossfuzz -- \
//!     --ignored long_run --nocapture                               # the long run
//! ```
//!
//! Environment: `CROSSFUZZ_SEED` (a number), `CROSSFUZZ_CASES` (how many;
//! the long run's default is 20 000), `CROSSFUZZ_START` (the first case's
//! index), `CROSSFUZZ_THREADS`, `CROSSFUZZ_LOG` (a file: one line per
//! finding, tab separated), `CROSSFUZZ_MINIMISE=0` (don't shrink failing
//! cases). A run is a function of the seed and the range of cases alone.
//! `-- --ignored one` checks one expression: `CROSSFUZZ_EXPR='x^(1/3)'`,
//! with `CROSSFUZZ_DIGITS` (5–20 or `off`), `CROSSFUZZ_SLIDERS='a=0.3,b=2'`,
//! `CROSSFUZZ_UNIT` (`deg`, `grad`), `CROSSFUZZ_PAIR` (an equal spelling).
//!
//! A failure prints the seed, the case and its setting, each finding, and
//! the expression shrunk to a small one that still shows it.

#[path = "../examples/oracle/checks.rs"]
mod checks;
#[path = "crossfuzz/generate.rs"]
mod generate;
#[cfg(feature = "mpfr-oracle")]
mod replay;
#[path = "crossfuzz/views.rs"]
mod views;

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use generate::{Case, T};
use graphing::TrigUnit;
use views::{Finding, Outcome, Setup, group};

/// The default run: small enough for `cargo test --workspace`.
const DEFAULT_SEED: u64 = 10;
const DEFAULT_CASES: u64 = 40;
/// The long run (`--ignored long_run`).
const LONG_CASES: u64 = 20_000;

fn env_u64(name: &str) -> Option<u64> {
    let v = std::env::var(name).ok()?;
    Some(
        v.trim()
            .parse()
            .unwrap_or_else(|_| panic!("{name}={v} is not a number")),
    )
}

fn setup(case: &Case) -> Setup {
    Setup {
        digits: case.digits,
        sliders: case
            .sliders
            .iter()
            .map(|(n, v)| (n.to_string(), *v))
            .collect(),
        unit: case.unit,
    }
}

fn run_case(case: &Case, groups: u32) -> Outcome {
    let pair = case.pair.as_ref().map(T::text);
    views::run(&case.expr.text(), pair.as_deref(), &setup(case), groups)
}

/// `case` shrunk while it still shows a finding of `class`: a node
/// replaced by one of its operands, by x or by a small number; the
/// setting made plainer (off, radians, no sliders). At most `budget`
/// tries.
fn minimise(case: &Case, class: &str, budget: usize) -> Case {
    let groups = group::of(class);
    let mut best = case.clone();
    let mut tries = 0;
    let still = |c: &Case, tries: &mut usize| {
        *tries += 1;
        run_case(c, groups).has(class)
    };
    // The setting first: fewer things to look at.
    for plainer in [
        Case {
            unit: TrigUnit::Radians,
            ..best.clone()
        },
        Case {
            sliders: Vec::new(),
            ..best.clone()
        },
        Case {
            digits: None,
            ..best.clone()
        },
    ] {
        let same = plainer.unit == best.unit
            && plainer.sliders == best.sliders
            && plainer.digits == best.digits;
        if !same && still(&plainer, &mut tries) {
            best = Case {
                unit: plainer.unit,
                sliders: plainer.sliders,
                digits: plainer.digits,
                ..best
            };
        }
    }
    // (Two spellings are shrunk no further: they must stay equal.)
    if best.pair.is_some() && groups == group::PAIR {
        return best;
    }
    best.pair = None;
    'again: loop {
        if tries >= budget {
            break;
        }
        for path in best.expr.paths() {
            let node = best.expr.at(&path).clone();
            let mut with: Vec<T> = node.children().into_iter().cloned().collect();
            for leaf in [
                T::X,
                generate::num("1"),
                generate::num("2"),
                generate::num("0"),
            ] {
                if node.size() > 1 || matches!(node, T::Raw(_) | T::Var(_)) {
                    with.push(leaf);
                }
            }
            for w in with {
                if w == node || w.size() >= node.size() && !matches!(node, T::Raw(_) | T::Var(_)) {
                    continue;
                }
                if tries >= budget {
                    break 'again;
                }
                let cand = Case {
                    expr: best.expr.replaced(&path, &w),
                    ..best.clone()
                };
                if cand.expr.text().len() < best.expr.text().len() && still(&cand, &mut tries) {
                    best = cand;
                    continue 'again;
                }
            }
        }
        break;
    }
    best
}

struct Done {
    case: Case,
    outcome: Outcome,
    /// Findings of a kind found before and left open ([`known`]), with
    /// why: listed, not failures.
    open: Vec<(Finding, &'static str)>,
    ms: f64,
    /// Per finding class, the case shrunk.
    minimised: Vec<(String, Case)>,
}

/// The groups of checks a case gets: every one, but one function typed
/// under two settings (two more analyses) only for the literal family and
/// a third of the rest.
fn groups_of(case: &Case) -> u32 {
    if case.family == "literal" || case.index.is_multiple_of(3) {
        group::ALL
    } else {
        group::ALL & !group::DIGITS
    }
}

/// The x a finding names (`x=1e0: …`).
fn position(detail: &str) -> Option<f64> {
    detail.strip_prefix("x=")?.split(':').next()?.parse().ok()
}

/// Why a finding is one found before and left open, if it is: a real
/// disagreement the report of this tester lists, with its cause, that
/// isn't fixed (some are documented limits, some another's to fix). They
/// are counted and listed in every run, so that a run fails on what is
/// new. Nothing here is a tolerance: each entry names a cause, and a
/// finding with another cause that happens to match is a risk taken
/// knowingly, kept small by matching the case's shape too.
fn known(case: &Case, f: &Finding) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|k| k.0 == f.class && (k.1)(case, f))
        .map(|k| k.2)
}

type Matches = fn(&Case, &Finding) -> bool;

/// Both spellings of a case, as typed.
fn texts(case: &Case) -> String {
    format!(
        "{} {}",
        case.expr.text(),
        case.pair.as_ref().map(T::text).unwrap_or_default()
    )
}

/// A count (n!, n!!, nCr, nPr) in the case.
fn counts(case: &Case, _: &Finding) -> bool {
    let t = texts(case);
    t.contains("nCr(") || t.contains("nPr(") || t.contains('!')
}

/// A number typed with more than 15 significant digits, under a setting
/// that keeps them (16 digits or more, or off).
fn long_literal(case: &Case, _: &Finding) -> bool {
    case.digits.is_none_or(|d| d >= 16)
        && generate::literals(&texts(case)).iter().any(|l| {
            l.trim_start_matches(['0', '.'])
                .chars()
                .filter(char::is_ascii_digit)
                .count()
                > 15
        })
}

/// A circular function of a whole number of 16 digits or more as typed.
fn trig_of_a_long_number(case: &Case, _: &Finding) -> bool {
    let t = texts(case);
    ["sin(", "cos(", "tan(", "cot(", "sec(", "csc("]
        .iter()
        .any(|name| {
            t.match_indices(name).any(|(i, _)| {
                let arg: String = t[i + name.len()..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                arg.len() >= 16 && t[i + name.len() + arg.len()..].starts_with(')')
            })
        })
}

/// At a subnormal x.
fn at_a_subnormal(_: &Case, f: &Finding) -> bool {
    position(&f.detail).is_some_and(|x| x != 0.0 && x.abs() < f64::MIN_POSITIVE)
}

fn any(_: &Case, _: &Finding) -> bool {
    true
}

/// A row that doesn't follow only because claims are left open.
fn not_refuted(_: &Case, f: &Finding) -> bool {
    !f.detail.contains("refuted")
}

const KNOWN: &[(&str, Matches, &str)] = &[
    // The replay (tests/replay) is another's to change; what it leaves
    // open is listed by kind in this tester's report.
    (
        "replay/unconfirmed",
        any,
        "the replay leaves a claim behind a shown row open (its own limits: a point between \
         two doubles, exact literal arithmetic, a root of a varying degree, a number past i64 \
         in a simplifier fact, a gap over 64 doubles)",
    ),
    (
        "replay/row-problem",
        not_refuted,
        "a row the replay can't derive because it leaves its claims open",
    ),
    // functions::ncr and npr (Γ's quotient) in doubles: ∞·0 far out and
    // an underflow to 0 give NaN where the quotient has a value, which the
    // interval form encloses.
    (
        "pt/undefined-where-enclosure-defined",
        counts,
        "nCr and nPr through Γ in doubles are NaN (∞·0, or a quotient that underflows) where \
         the count has a value: functions.rs ncr/npr",
    ),
    (
        "pt/box-defined-but-undefined",
        counts,
        "nCr and nPr through Γ in doubles are NaN (∞·0, or a quotient that underflows) where \
         the count has a value: functions.rs ncr/npr",
    ),
    // docs/ti-conventions.md: anything but +, −, ×, ÷, whole powers, |·|
    // and the counts is computed in floating point, from the double
    // nearest a literal.
    (
        "mp/scalar-off-value",
        trig_of_a_long_number,
        "a circular function of a typed whole number no double holds is computed at its \
         double (documented: docs/ti-conventions.md), a different angle",
    ),
    (
        "mp/defined-at-most-points-where-undefined",
        long_literal,
        "off or at 16 digits and more, a literal the doubles can't tell from its neighbour is \
         its double to the point evaluator (documented: docs/ti-conventions.md, review 13 \
         'Known and left'): a difference only that decides can make f defined in doubles",
    ),
    (
        "mp/undefined-at-most-points-where-defined",
        long_literal,
        "off or at 16 digits and more, a literal the doubles can't tell from its neighbour is \
         its double to the point evaluator (documented: docs/ti-conventions.md, review 13 \
         'Known and left'): a difference only that decides can make f undefined in doubles",
    ),
    // examples/oracle.rs, "Known false alarms".
    (
        "pt/scalar-vs-reference",
        at_a_subnormal,
        "the reference evaluator's accuracy on subnormal operands (examples/oracle.rs, Known \
         false alarms): the compiled value is the closer one",
    ),
    (
        "panel/evaluator-disagrees",
        at_a_subnormal,
        "the reference evaluator's accuracy on subnormal operands (examples/oracle.rs, Known \
         false alarms): the compiled value is the closer one",
    ),
];

struct Report {
    seed: u64,
    start: u64,
    cases: u64,
    done: Vec<Done>,
    seconds: f64,
}

fn describe(case: &Case) -> String {
    format!(
        "y={}{}  [{}]",
        case.expr.text(),
        case.pair
            .as_ref()
            .map_or(String::new(), |p| format!("  ≡  y={}", p.text())),
        case.setting()
    )
}

/// How to check one case again by hand.
fn reproduce(case: &Case) -> String {
    let mut s = format!("CROSSFUZZ_EXPR='{}'", case.expr.text());
    if let Some(p) = &case.pair {
        s.push_str(&format!(" CROSSFUZZ_PAIR='{}'", p.text()));
    }
    if let Some(d) = case.digits {
        s.push_str(&format!(" CROSSFUZZ_DIGITS={d}"));
    }
    if !case.sliders.is_empty() {
        let v: Vec<String> = case
            .sliders
            .iter()
            .map(|(n, v)| format!("{n}={v:?}"))
            .collect();
        s.push_str(&format!(" CROSSFUZZ_SLIDERS='{}'", v.join(",")));
    }
    match case.unit {
        TrigUnit::Radians => {}
        TrigUnit::Degrees => s.push_str(" CROSSFUZZ_UNIT=deg"),
        TrigUnit::Grads => s.push_str(" CROSSFUZZ_UNIT=grad"),
    }
    s
}

/// Runs cases `start .. start + cases` of `seed` on every core.
fn fuzz(seed: u64, start: u64, cases: u64) -> Report {
    let threads = env_u64("CROSSFUZZ_THREADS").map_or_else(
        || {
            std::thread::available_parallelism()
                .map_or(4, |n| n.get())
                .min(16)
        },
        |n| n.max(1) as usize,
    );
    let shrink = std::env::var("CROSSFUZZ_MINIMISE").map_or(true, |v| v != "0");
    println!(
        "crossfuzz: seed {seed}, cases {start}..{} ({cases}), {threads} threads, MPFR replay {}",
        start + cases,
        if cfg!(feature = "mpfr-oracle") {
            "on"
        } else {
            "off (--features mpfr-oracle)"
        }
    );
    let started = Instant::now();
    let next = AtomicU64::new(0);
    let finished = AtomicU64::new(0);
    let done: Mutex<Vec<Done>> = Mutex::new(Vec::new());
    // What each worker is on, for the watchdog.
    let running: Mutex<Vec<Option<(u64, Instant, bool)>>> = Mutex::new(vec![None; threads]);
    // Each class is shrunk for its first few cases only.
    let shrunk: Mutex<BTreeMap<String, usize>> = Mutex::new(BTreeMap::new());
    std::thread::scope(|sc| {
        for t in 0..threads {
            let (next, finished, done, running, shrunk) =
                (&next, &finished, &done, &running, &shrunk);
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(sc, move || {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= cases {
                            break;
                        }
                        let case = generate::case(seed, start + i);
                        running.lock().unwrap()[t] = Some((case.index, Instant::now(), false));
                        let t0 = Instant::now();
                        let mut outcome = run_case(&case, groups_of(&case));
                        let ms = t0.elapsed().as_secs_f64() * 1e3;
                        let mut open = Vec::new();
                        outcome.findings.retain(|f| match known(&case, f) {
                            Some(why) => {
                                open.push((f.clone(), why));
                                false
                            }
                            None => true,
                        });
                        let mut minimised = Vec::new();
                        if shrink {
                            let mut classes: Vec<String> =
                                outcome.findings.iter().map(|f| f.class.clone()).collect();
                            classes.dedup();
                            for class in classes {
                                let n = {
                                    let mut m = shrunk.lock().unwrap();
                                    let n = m.entry(class.clone()).or_insert(0);
                                    *n += 1;
                                    *n
                                };
                                if n <= 4 && !class.starts_with("panic/") {
                                    minimised.push((class.clone(), minimise(&case, &class, 60)));
                                }
                            }
                        }
                        running.lock().unwrap()[t] = None;
                        done.lock().unwrap().push(Done {
                            case,
                            outcome,
                            open,
                            ms,
                            minimised,
                        });
                        let n = finished.fetch_add(1, Ordering::Relaxed) + 1;
                        if cases >= 2000 && n.is_multiple_of(1000) {
                            eprintln!(
                                "crossfuzz: {n} of {cases} cases, {:.0} s",
                                started.elapsed().as_secs_f64()
                            );
                        }
                    }
                })
                .expect("a worker thread");
        }
        // The watchdog: a case that runs for a minute is named, once.
        let (running, finished) = (&running, &finished);
        sc.spawn(move || {
            while finished.load(Ordering::Relaxed) < cases {
                std::thread::sleep(Duration::from_millis(200));
                for slot in running.lock().unwrap().iter_mut() {
                    if let Some((i, since, told)) = slot
                        && !*told
                        && since.elapsed() > Duration::from_secs(60)
                    {
                        *told = true;
                        eprintln!(
                            "crossfuzz: case {i} still running after 60 s: {}",
                            describe(&generate::case(seed, *i))
                        );
                    }
                }
            }
        });
    });
    let mut done = done.into_inner().unwrap();
    done.sort_by_key(|d| d.case.index);
    Report {
        seed,
        start,
        cases,
        done,
        seconds: started.elapsed().as_secs_f64(),
    }
}

impl Report {
    fn failing(&self) -> Vec<&Done> {
        self.done
            .iter()
            .filter(|d| !d.outcome.findings.is_empty())
            .collect()
    }

    /// The tallies, the notes, the findings by class, the slowest cases.
    fn print(&self) {
        let mut families: BTreeMap<&str, usize> = BTreeMap::new();
        let mut settings: BTreeMap<String, usize> = BTreeMap::new();
        let mut tally: BTreeMap<String, usize> = BTreeMap::new();
        let mut notes: BTreeMap<String, (usize, usize, String)> = BTreeMap::new();
        let mut sliders = 0;
        for d in &self.done {
            *families.entry(d.case.family).or_insert(0) += 1;
            let digits = d.case.digits.map_or("off".to_string(), |n| n.to_string());
            *settings.entry(digits).or_insert(0) += 1;
            sliders += usize::from(!d.case.sliders.is_empty());
            for (k, n) in &d.outcome.tally {
                *tally.entry(k.clone()).or_insert(0) += n;
            }
            for (k, n) in &d.outcome.notes {
                let e = notes.entry(k.clone()).or_insert_with(|| {
                    (
                        0,
                        0,
                        format!(
                            "{}: {}",
                            describe(&d.case),
                            d.outcome.note_samples.get(k).cloned().unwrap_or_default()
                        ),
                    )
                });
                e.0 += n;
                e.1 += 1;
            }
        }
        println!(
            "crossfuzz: {} cases in {:.1} s; families {families:?}; digits {settings:?}; {sliders} with sliders",
            self.done.len(),
            self.seconds
        );
        println!("checked:");
        for (k, n) in &tally {
            println!("  {n:>10}  {k}");
        }
        if !notes.is_empty() {
            println!("counted, not failures:");
            for (k, (n, fs, sample)) in &notes {
                println!("  {n:>10}  {k} (in {fs} cases), e.g. {sample}");
            }
        }
        // Found before and left open: by class and cause.
        let mut open: BTreeMap<(&str, &str), Vec<(&Done, &Finding)>> = BTreeMap::new();
        for d in &self.done {
            for (f, why) in &d.open {
                let e = open.entry((&f.class, why)).or_default();
                if e.last()
                    .is_none_or(|(last, _)| last.case.index != d.case.index)
                {
                    e.push((d, f));
                }
            }
        }
        if !open.is_empty() {
            println!("found before and left open (not failures):");
            for ((class, why), ds) in &open {
                println!("  {:>6} cases  {class}: {why}", ds.len());
                for (d, f) in ds.iter().take(3) {
                    println!("      case {} {}", d.case.index, describe(&d.case));
                    println!("          {}", f.detail);
                }
            }
        }
        let mut slow: Vec<&Done> = self.done.iter().collect();
        slow.sort_by(|a, b| b.ms.total_cmp(&a.ms));
        for d in slow.iter().take(5) {
            println!(
                "slowest: case {}: {:.1} s: {}",
                d.case.index,
                d.ms / 1e3,
                describe(&d.case)
            );
        }
        let failing = self.failing();
        let mut classes: BTreeMap<&str, Vec<&Done>> = BTreeMap::new();
        for d in &failing {
            let mut seen: Vec<&str> = Vec::new();
            for f in &d.outcome.findings {
                if !seen.contains(&f.class.as_str()) {
                    seen.push(&f.class);
                    classes.entry(&f.class).or_default().push(d);
                }
            }
        }
        println!(
            "crossfuzz: seed {}, cases {}..{}: {} cases with findings, {} classes",
            self.seed,
            self.start,
            self.start + self.cases,
            failing.len(),
            classes.len()
        );
        for (class, ds) in &classes {
            println!("\n## {class}: {} cases", ds.len());
            for d in ds.iter().take(12) {
                println!(
                    "  case {} [{}] {}",
                    d.case.index,
                    d.case.family,
                    describe(&d.case)
                );
                for f in d
                    .outcome
                    .findings
                    .iter()
                    .filter(|f| f.class == *class)
                    .take(2)
                {
                    println!("      {}", f.detail);
                }
                for (c, m) in &d.minimised {
                    if c == class {
                        println!("      minimised: {}", describe(m));
                        println!("      again: {}", reproduce(m));
                    }
                }
            }
            if ds.len() > 12 {
                println!(
                    "  … and {} more (CROSSFUZZ_LOG=file lists every one)",
                    ds.len() - 12
                );
            }
        }
        if let Ok(path) = std::env::var("CROSSFUZZ_LOG") {
            let mut text = String::new();
            for d in &self.done {
                let findings = d.outcome.findings.iter().map(|f| (f, ""));
                for (f, why) in findings.chain(d.open.iter().map(|(f, why)| (f, *why))) {
                    let min = d
                        .minimised
                        .iter()
                        .find(|(c, _)| *c == f.class)
                        .map_or(String::new(), |(_, m)| describe(m));
                    text.push_str(&format!(
                        "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                        d.case.index,
                        d.case.family,
                        f.class,
                        describe(&d.case),
                        f.detail.replace(['\t', '\n'], " "),
                        min,
                        why
                    ));
                }
            }
            std::fs::write(&path, text).unwrap_or_else(|e| panic!("CROSSFUZZ_LOG={path}: {e}"));
            println!("findings written to {path}");
        }
    }

    fn assert_clean(&self) {
        let failing = self.failing();
        assert!(
            failing.is_empty(),
            "crossfuzz: seed {}, cases {}..{}: {} cases with findings (listed above); the first: case {}: {}\n  {}: {}\n  again: {}",
            self.seed,
            self.start,
            self.start + self.cases,
            failing.len(),
            failing[0].case.index,
            describe(&failing[0].case),
            failing[0].outcome.findings[0].class,
            failing[0].outcome.findings[0].detail,
            reproduce(
                failing[0]
                    .minimised
                    .first()
                    .map_or(&failing[0].case, |(_, m)| m)
            )
        );
    }
}

/// The default run: a few seconds, in the normal suite.
#[test]
fn default_run() {
    let seed = env_u64("CROSSFUZZ_SEED").unwrap_or(DEFAULT_SEED);
    let cases = env_u64("CROSSFUZZ_CASES").unwrap_or(DEFAULT_CASES);
    let r = fuzz(seed, env_u64("CROSSFUZZ_START").unwrap_or(0), cases);
    r.print();
    r.assert_clean();
}

/// The long run: tens of thousands of cases (release, every core).
#[test]
#[ignore = "the long run: minutes to hours"]
fn long_run() {
    let seed = env_u64("CROSSFUZZ_SEED").unwrap_or(DEFAULT_SEED);
    let cases = env_u64("CROSSFUZZ_CASES").unwrap_or(LONG_CASES);
    let r = fuzz(seed, env_u64("CROSSFUZZ_START").unwrap_or(0), cases);
    r.print();
    r.assert_clean();
}

/// One expression through every check (`CROSSFUZZ_EXPR`, …).
#[test]
#[ignore = "checks the expression in CROSSFUZZ_EXPR"]
fn one() {
    let text = std::env::var("CROSSFUZZ_EXPR").expect("CROSSFUZZ_EXPR='an expression of x'");
    let digits = match std::env::var("CROSSFUZZ_DIGITS").ok().as_deref() {
        None | Some("off") => None,
        Some(d) => Some(d.parse::<u8>().expect("CROSSFUZZ_DIGITS: 5 to 20, or off")),
    };
    let sliders: Vec<(String, f64)> = std::env::var("CROSSFUZZ_SLIDERS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            let (n, v) = s.split_once('=').expect("CROSSFUZZ_SLIDERS: a=1,b=2");
            (
                n.trim().to_string(),
                v.trim().parse().expect("a slider's value"),
            )
        })
        .collect();
    let unit = match std::env::var("CROSSFUZZ_UNIT").ok().as_deref() {
        Some("deg") => TrigUnit::Degrees,
        Some("grad") => TrigUnit::Grads,
        _ => TrigUnit::Radians,
    };
    let pair = std::env::var("CROSSFUZZ_PAIR").ok();
    let set = Setup {
        digits,
        sliders,
        unit,
    };
    // CROSSFUZZ_GROUPS=points,panel,replay,digits,pair,plot: only those.
    let groups = std::env::var("CROSSFUZZ_GROUPS").map_or(group::ALL, |g| {
        g.split(',')
            .map(|name| match name.trim() {
                "points" => group::POINTS,
                "panel" => group::PANEL,
                "replay" => group::REPLAY,
                "digits" => group::DIGITS,
                "pair" => group::PAIR,
                "plot" => group::PLOT,
                other => panic!("CROSSFUZZ_GROUPS: no group {other}"),
            })
            .fold(0, |a, b| a | b)
    });
    let t0 = Instant::now();
    let out = views::run(&text, pair.as_deref(), &set, groups);
    println!(
        "y={text} under {set:?}: {:.2} s",
        t0.elapsed().as_secs_f64()
    );
    for (k, n) in &out.tally {
        println!("  checked {n:>8}  {k}");
    }
    for (k, n) in &out.notes {
        println!(
            "  note    {n:>8}  {k}, e.g. {}",
            out.note_samples.get(k).cloned().unwrap_or_default()
        );
    }
    for f in &out.findings {
        println!("  FINDING {}: {}", f.class, f.detail);
    }
    assert!(out.findings.is_empty(), "{} findings", out.findings.len());
}

/// Prints the cases of a run without checking them.
#[test]
#[ignore = "lists the cases CROSSFUZZ_SEED, CROSSFUZZ_START and CROSSFUZZ_CASES name"]
fn list_cases() {
    let seed = env_u64("CROSSFUZZ_SEED").unwrap_or(DEFAULT_SEED);
    let start = env_u64("CROSSFUZZ_START").unwrap_or(0);
    for i in start..start + env_u64("CROSSFUZZ_CASES").unwrap_or(DEFAULT_CASES) {
        let c = generate::case(seed, i);
        println!("{i}\t{}\t{}", c.family, describe(&c));
    }
}

/// The generator is a function of the seed and the index, its texts
/// parse, and every family and setting turns up.
#[test]
fn the_generator_is_deterministic_and_parses() {
    let mut families: BTreeMap<&str, usize> = BTreeMap::new();
    let mut digits: BTreeMap<Option<u8>, usize> = BTreeMap::new();
    let (mut sliders, mut pairs) = (0, 0);
    for i in 0..600 {
        let (a, b) = (generate::case(7, i), generate::case(7, i));
        assert_eq!(a.expr, b.expr, "case {i} differs between two calls");
        assert_eq!(a.setting(), b.setting());
        assert!(a.expr.size() <= generate::MAX_SIZE && a.expr.text().len() <= generate::MAX_TEXT);
        for text in std::iter::once(a.expr.text()).chain(a.pair.as_ref().map(T::text)) {
            let opts = graphing::lexer::ParseOptions {
                decimal_comma: false,
                literal_digits: a.digits,
            };
            let eq = graphing::Equation::parse_with(&format!("y={text}"), opts)
                .unwrap_or_else(|e| panic!("case {i}: y={text} doesn't parse: {e:?}"));
            assert!(
                eq.explicit().is_some(),
                "case {i}: y={text} is no function of x"
            );
        }
        *families.entry(a.family).or_insert(0) += 1;
        *digits.entry(a.digits).or_insert(0) += 1;
        sliders += usize::from(!a.sliders.is_empty());
        pairs += usize::from(a.pair.is_some());
    }
    assert_ne!(generate::case(7, 3).expr, generate::case(8, 3).expr);
    for f in ["literal", "definedness", "counting", "display", "pair"] {
        assert!(
            families.get(f).copied().unwrap_or(0) >= 20,
            "{f}: {families:?}"
        );
    }
    assert_eq!(digits.len(), 6, "{digits:?}");
    assert!(
        sliders >= 30 && pairs >= 60,
        "{sliders} with sliders, {pairs} pairs"
    );
}

/// The checks have teeth: two spellings that aren't equal, and text that
/// is no expression, are findings; an ordinary function has none.
#[test]
fn planted_disagreements_are_found() {
    let set = Setup {
        digits: None,
        sliders: Vec::new(),
        unit: TrigUnit::Radians,
    };
    let out = views::run("x^2+1", Some("x^2+1.5"), &set, group::PAIR);
    assert!(
        out.findings.iter().any(|f| f.class.starts_with("pair/")),
        "{:?}",
        out.findings
    );
    let out = views::run("x^(1/3)", Some("x^0.2"), &set, group::PAIR);
    assert!(
        out.findings.iter().any(|f| f.class.starts_with("pair/")),
        "{:?}",
        out.findings
    );
    let out = views::run("x^^2", None, &set, group::ALL);
    assert!(out.has("gen/does-not-parse"), "{:?}", out.findings);
    let out = views::run("x^3-2*x+1/(x-1)", None, &set, group::ALL);
    assert!(out.findings.is_empty(), "{:?}", out.findings);
}
