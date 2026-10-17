//! A random cross-checking tester of the graphing engine's views of one
//! function: the compiled point evaluator, the typed reference evaluator,
//! the interval form with its decorations, the certifier and the panel,
//! the plotter, and (with `--features mpfr-oracle`) the independent MPFR
//! replay of the certificate and that checker's own evaluation of f.
//!
//! The broad tests (`examples/sweep.rs`, `examples/oracle.rs`, the certify
//! corpus and its replay) take ordinary functions, shifted and scaled. The
//! review rounds 12–19 found bugs in a few families they don't reach;
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
//!    by hand);
//! 6. **steps of exact literals a hair off a jump**
//!    (`5 + ceil(1.00000000000000000001)`, `round(2.49999999999999999999)`,
//!    `sign(10^(−400))`, `mod(x, 1.00000000000000000001)`), mostly off, as
//!    a term, a factor, an exponent, a root's degree;
//! 7. **narrow kinks and dents at a point no double holds**
//!    (`min((x − sin 1)^(2/1000003), 0.01)` and with x added, |x − 1/3|,
//!    (x − √2)·|x − √2|, max(x − c, c − x, k), (x − π/7)^(2/3));
//! 8. **exact values on the edge of where a function turns**, whose
//!    doubles are on the other side (a count beside a pole of Γ, an
//!    exponent's or a root degree's parity, a quarter turn in degrees or
//!    grads, ln and log beside 1, arcsin, arccos, arcosh, artanh, arcsec
//!    beside ±1, a divisor exactly tiny, ties of min and max, mod at a
//!    multiple),
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
//! cases), `CROSSFUZZ_REPLAY_SECONDS` (how long a replay may take: 30;
//! each runs in a process of its own, this binary's `replay_child`, and
//! is stopped then). A run is a function of the seed and the range of
//! cases alone, but for the replays given up on, which are counted,
//! listed in the log and never fail it.
//! `-- --ignored one` checks one expression: `CROSSFUZZ_EXPR='x^(1/3)'`,
//! with `CROSSFUZZ_DIGITS` (5–20 or `off`), `CROSSFUZZ_SLIDERS='a=0.3,b=2'`,
//! `CROSSFUZZ_UNIT` (`deg`, `grad`), `CROSSFUZZ_PAIR` (an equal spelling)
//! and `CROSSFUZZ_GROUPS` (`points,panel,replay,digits,pair,plot`: only
//! those checks). `-- --ignored list_cases` prints a run's cases.
//!
//! A failure prints the seed, the case and its setting, each finding, and
//! the expression shrunk to a small one that still shows it.
//!
//! # What a run reports
//!
//! * **Findings**: two views disagree. They fail the run. A verdict is
//!   decided by a view that proves things: f's interval enclosure over
//!   the box in question, MPFR (the replay's evaluator), or the replay's
//!   own proofs. What a finite set of samples or a fixed step suggests
//!   (the oracle's checks judge f sampled in doubles: a pole where f
//!   grows, no turn where f is lower beside one, P/2 a period, a zero
//!   missing where f changes sign) is a finding only where such a view
//!   bears it out over the stretch it is about, and counted otherwise
//!   (`views::Proofs::stands`).
//! * **Found before and left open** ([`KNOWN`]): disagreements of a cause
//!   already found and written down, that isn't fixed here (a documented
//!   limit of the doubles, the replay's own limits, which are another's
//!   to change). Listed with their cause in every run, so that a run
//!   fails on what is new.
//! * **Counted, not failures**: what is no disagreement between views
//!   that prove things, though two evaluations differ. The compiled
//!   program and the reference compute in doubles: an operand that rounds
//!   onto a pole makes f "undefined" there (1 − cos x at a tiny x), a
//!   sum that absorbs a term makes f another function far out, and a NaN
//!   can say "can't tell". Such a point is judged by f's enclosure, which
//!   says "maybe" there, and by MPFR; where the scalar is inside f's
//!   enclosure it is "all that is known within a wide one"
//!   (`plot::drawn_value`).

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
const DEFAULT_CASES: u64 = 16;
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
/// under two settings (two more analyses) only for the families of exact
/// literals and a third of the rest.
fn groups_of(case: &Case) -> u32 {
    if matches!(case.family, "literal" | "steps" | "edges") || case.index.is_multiple_of(3) {
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

/// A circular function whose argument doesn't vary with x and holds a
/// whole number of 16 digits or more as typed: the number itself, or
/// arithmetic on numbers alone around it (cos(|n|), which is cos(n)).
fn trig_of_a_long_number(case: &Case, _: &Finding) -> bool {
    let t = texts(case);
    ["sin(", "cos(", "tan(", "cot(", "sec(", "csc("]
        .iter()
        .any(|name| {
            t.match_indices(name).any(|(i, _)| {
                // The argument: up to the parenthesis that closes the call.
                let mut depth = 1;
                let arg: String = t[i + name.len()..]
                    .chars()
                    .take_while(|c| {
                        depth += i32::from(*c == '(') - i32::from(*c == ')');
                        depth > 0
                    })
                    .collect();
                let mut run = 0;
                let long = arg.chars().any(|c| {
                    run = if c.is_ascii_digit() { run + 1 } else { 0 };
                    run >= 16
                });
                long && !arg.contains('x')
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

/// A count in the case, and two finite values in the finding.
fn counts_and_two_values(case: &Case, f: &Finding) -> bool {
    counts(case, f) && !f.detail.contains("NaN") && !f.detail.contains("Undef")
}

/// A ratio of long integers written in the case, and a compiled NaN.
fn nan_of_a_long_ratio(case: &Case, f: &Finding) -> bool {
    f.detail.contains("compiled NaN") && {
        let t = texts(case);
        let cs = t.as_bytes();
        (0..cs.len()).any(|i| {
            cs[i] == b'/'
                && (cs[..i]
                    .iter()
                    .rev()
                    .take_while(|c| c.is_ascii_digit())
                    .count()
                    >= 7
                    || cs[i + 1..]
                        .iter()
                        .take_while(|c| c.is_ascii_digit())
                        .count()
                        >= 7)
        })
    }
}

/// A root in the case, two finite values in the finding, x at the
/// doubles' small end.
fn root_far_in(case: &Case, f: &Finding) -> bool {
    texts(case).contains("root(")
        && !f.detail.contains("NaN")
        && !f.detail.contains("Undef")
        && position(&f.detail).is_some_and(|x| x != 0.0 && x.abs() <= 1e-300)
}

const KNOWN: &[(&str, Matches, &str)] = &[
    // The replay (tests/replay) is another's to change; what it leaves
    // open is listed by kind in this tester's report. (A claim it refutes
    // is a finding, `replay/refuted`, and so is a limit it leaves open
    // with f's values at points against it, `replay/values-against`.)
    (
        "replay/unconfirmed",
        any,
        "the replay leaves a claim behind a shown row open (its own limits: a point between \
         two doubles, exact literal arithmetic past what it carries (an f it places nowhere \
         bears out no fact tested at points), a root of a varying degree where the radicand \
         may be negative, a number past i64 in a simplifier fact, a gap over 64 doubles)",
    ),
    // (A row problem that says something against the row from what the
    // claims prove is a finding: `replay/row-against`, and
    // `replay/row-narrower` for a value listed more narrowly than they
    // show; one of a text not yet read, `replay/row-problem-unread`. Only
    // those that leave a row open are listed here: views.rs,
    // `mp::row_problem`.)
    (
        "replay/row-problem",
        not_refuted,
        "a row the replay can't derive: its claims are left open, or what it rests on is not \
         shown (a piece of the domain defined, the domain's symmetry under a parity; anything \
         resting on a KinkSides claim, which the replay doesn't read yet)",
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
        "pt/reference-undefined-where-enclosure-defined",
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
    // wide.rs: Γ in extended range keeps about 13 digits.
    (
        "pt/scalar-vs-reference",
        counts_and_two_values,
        "a factorial just past the doubles (Γ(171.5)) is kept to about 13 digits in extended \
         range: 1/(x − 0.5)! at 171 differs from the reference from there on (wide.rs)",
    ),
    (
        "panel/evaluator-disagrees",
        counts_and_two_values,
        "a factorial just past the doubles (Γ(171.5)) is kept to about 13 digits in extended \
         range: 1/(x − 0.5)! at 171 differs from the reference from there on (wide.rs)",
    ),
    // wide.rs pow_mag, functions.rs pow_ratio: |t·e| ≥ 2⁵³ > |t·(e + log₂ m)|.
    (
        "pt/scalar-vs-reference",
        nan_of_a_long_ratio,
        "x^(p/q) for a long p in a narrow band of x is NaN, untagged, where the power is \
         below every double (it should be 0): x^(9007199254740993/6) at 0.02 (wide.rs \
         pow_mag returns Unknown, functions.rs pow_ratio makes it a plain NaN)",
    ),
    (
        "panel/evaluator-disagrees",
        nan_of_a_long_ratio,
        "x^(p/q) for a long p in a narrow band of x is NaN, untagged, where the power is \
         below every double (it should be 0): x^(9007199254740993/6) at 0.02 (wide.rs \
         pow_mag returns Unknown, functions.rs pow_ratio makes it a plain NaN)",
    ),
    // functions.rs root: a degree no whole number goes through pow(x, 1/n).
    (
        "pt/scalar-vs-reference",
        root_far_in,
        "root(x, n) of a degree no whole number is pow(x, 1/n) with 1/n rounded: tens of \
         ulps where |ln x|/n is large (functions.rs root); the reference is the closer one",
    ),
    // docs/ti-conventions.md: circular functions are computed in floating
    // point, from the double nearest a literal. Exact literal arithmetic
    // and integer steps do not change that policy for trigonometry.
    (
        "mp/scalar-off-value",
        trig_of_a_long_number,
        "a circular function of a typed whole number no double holds is computed at its \
         double (documented: docs/ti-conventions.md), a different angle",
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
            // And the replays given up on, which a later run may not.
            for d in &self.done {
                if let Some(s) = d.outcome.note_samples.get("replay/gave-up-waiting") {
                    text.push_str(&format!(
                        "{}\t{}\tnote: replay/gave-up-waiting\t{}\t{}\t\t\n",
                        d.case.index,
                        d.case.family,
                        describe(&d.case),
                        s.replace(['\t', '\n'], " "),
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

/// The process a certificate's replay runs in: the tester starts this
/// binary for it, and stops it when the replay runs over
/// (`CROSSFUZZ_REPLAY_SECONDS`). Does nothing when run by hand.
#[cfg(feature = "mpfr-oracle")]
#[test]
#[ignore = "the replay's own process: the tester starts it"]
fn replay_child() {
    views::replay_child();
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
    for f in [
        "literal",
        "definedness",
        "counting",
        "display",
        "pair",
        "steps",
        "kinks",
        "edges",
    ] {
        assert!(
            families.get(f).copied().unwrap_or(0) >= 20,
            "{f}: {families:?}"
        );
    }
    // The new families' shapes turn up: each steps case has a step of an
    // exact literal or of x, mostly off and under a limit too; each kinks
    // case has x less a centre no double holds, and the narrow valley is
    // among them; edges come in degrees and in grads.
    let of = |f: &str| -> Vec<Case> {
        (0..600)
            .map(|i| generate::case(7, i))
            .filter(|c| c.family == f)
            .collect()
    };
    let steps = of("steps");
    assert!(steps.iter().all(|c| {
        let t = c.expr.text();
        ["floor(", "ceil(", "round(", "sign(", "mod("]
            .iter()
            .any(|s| t.contains(s))
    }));
    let off = steps.iter().filter(|c| c.digits.is_none()).count();
    assert!(
        off * 2 > steps.len() && off < steps.len(),
        "{off} of {} steps cases off",
        steps.len()
    );
    let kinks = of("kinks");
    assert!(kinks.iter().all(|c| c.expr.text().contains("x-(")));
    assert!(kinks.iter().any(|c| c.expr.text().contains("^(2/1000003)")));
    let edges = of("edges");
    for unit in [TrigUnit::Degrees, TrigUnit::Grads] {
        assert!(edges.iter().any(|c| c.unit == unit), "no edge in {unit:?}");
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

/// The oracle's checks sample f in doubles. What they conclude from the
/// samples is a finding of this tester only where a view that proves
/// things bears it out (`views::Proofs::stands`): f's enclosure over the
/// stretch in question. The false answers the oracle's own self-test
/// plants, planted here: still findings where an enclosure proves them
/// false, and counted where none can (a pole, a shorter period, two turns
/// for one, a bound reached, a line not approached).
#[test]
fn planted_false_answers_are_findings_where_proven() {
    use graphing::analysis::{AsymptoteSide, Family, KeyGraphFeatures};
    type Plant = Box<dyn Fn(&mut KeyGraphFeatures)>;
    let set = Setup {
        digits: None,
        sliders: Vec::new(),
        unit: TrigUnit::Radians,
    };
    let one = Family::single;
    // What is planted in whose panel, and the class it is found as
    // (`true`) or counted as.
    let cases: Vec<(&str, &str, bool, Plant)> = vec![
        // f″ = 6x keeps its sign about 1.
        (
            "x^3",
            "panel/inflection-not-inflection",
            true,
            Box::new(move |k| k.data.inflection_points.push((one(1.0), 1.0))),
        ),
        (
            "x^3",
            "panel/inflection-value-wrong",
            true,
            Box::new(|k| {
                for p in &mut k.data.inflection_points {
                    p.1 += 0.5;
                }
            }),
        ),
        // Continuous between two samples, and lower at 1 than at both.
        (
            "(x-1)^2",
            "panel/extremum-missing",
            true,
            Box::new(|k| k.data.minima.clear()),
        ),
        // f's enclosure from the sample out to ∞ leaves the line out.
        (
            "atan(x)",
            "panel/asymptote-not-approached",
            true,
            Box::new(|k| {
                for a in &mut k.data.horizontal_asymptotes {
                    a.0 *= 0.99;
                }
            }),
        ),
        (
            "exp(-x^2)",
            "panel/asymptote-not-approached",
            true,
            Box::new(|k| {
                k.data.horizontal_asymptotes = vec![(0.5, AsymptoteSide::AnyInfinity)];
            }),
        ),
        // Defined on every real about 2.
        (
            "x^2+1",
            "panel/excluded-point-defined",
            true,
            Box::new(move |k| k.data.excluded.push(one(2.0))),
        ),
        // 0 exactly at 0; continuous and of opposite signs at 1 and 2.
        (
            "x^2",
            "panel/zero-missing",
            true,
            Box::new(|k| {
                k.data.zeros.clear();
                k.x_intercept.clear();
            }),
        ),
        (
            "x^2-2",
            "panel/zero-missing",
            true,
            Box::new(|k| k.data.zeros.retain(|z| z.x < 0.0)),
        ),
        // f is 1 and more about 0.
        (
            "x^2+1",
            "panel/zero-not-zero",
            true,
            Box::new(move |k| {
                k.data.zeros = vec![one(0.0)];
                k.x_intercept = "0".into();
            }),
        ),
        // And here f is 1 about 0 to MPFR, which reads the decimals as
        // typed; f's enclosure in doubles there is [0, 2.3], which leaves
        // the zero standing.
        (
            "10^16*(1.0000000000000001-1)+x",
            if cfg!(feature = "mpfr-oracle") {
                "panel/zero-not-zero"
            } else {
                "panel/zero-not-zero-in-doubles"
            },
            cfg!(feature = "mpfr-oracle"),
            Box::new(move |k| {
                k.data.zeros = vec![one(0.0)];
                k.x_intercept = "0".into();
            }),
        ),
        // A value at a point is a witness.
        (
            "10^16*(1.0000000000000001-1)+x",
            "panel/y-intercept-wrong",
            true,
            Box::new(|k| {
                k.data.y_intercept = Some(0.0);
                k.y_intercept = "0".into();
            }),
        ),
        // That f is unbounded beside 0 no enclosure proves, and no
        // values; the replay's own rule does (a divisor with a zero
        // there, the numerator away from 0), where it is built in.
        (
            "1/x",
            if cfg!(feature = "mpfr-oracle") {
                "panel/vertical-asymptote-missing"
            } else {
                "panel/vertical-asymptote-missing-not-proven"
            },
            cfg!(feature = "mpfr-oracle"),
            Box::new(|k| {
                k.data.vertical_asymptotes.clear();
                k.data.excluded.clear();
            }),
        ),
        // f undefined between two ends that read alike.
        (
            "ln((x-sin(1))*(x-sin(1)-0.0000001))",
            "panel/row-ends-alike",
            true,
            Box::new(|k| k.domain = "x ∈ (−∞, ≈0.841471) ∪ (≈0.841471, ∞)".into()),
        ),
        // f(x + P/2) = f(x) at every sample proves no period.
        (
            "sin(x)",
            "panel/period-not-fundamental-not-proven",
            false,
            Box::new(|k| k.data.period = k.data.period.map(|p| 2.0 * p)),
        ),
        // 1 at the double beside π/2 is sin's value there rounded: the
        // sine is below 1 at every double.
        (
            "sin(x)",
            "panel/range-open-bound-reached-not-proven",
            false,
            Box::new(|k| {
                for i in &mut k.data.range {
                    i.hi.closed = false;
                }
            }),
        ),
        (
            "(x-1)^2*(x-1.000001)^2",
            "panel/extrema-merged-not-proven",
            false,
            Box::new(move |k| k.data.minima = vec![(one(1.0), 0.0)]),
        ),
        // f less a line of a double's slope, at three decades.
        (
            "x+1/x",
            "panel/asymptote-not-approached-not-proven",
            false,
            Box::new(|k| {
                for a in &mut k.data.oblique_asymptotes {
                    a.1 += 1.0;
                }
            }),
        ),
    ];
    for (text, class, finding, plant) in &cases {
        let out = views::panel_planted(text, &set, plant.as_ref());
        let found = out.has(class);
        let counted = out.notes.contains_key(*class);
        assert!(
            if *finding { found } else { counted && !found },
            "y={text}: expected {class} {}; findings {:?}, counted {:?}",
            if *finding { "found" } else { "counted" },
            out.findings,
            out.notes
        );
        // Counted means not a finding under the oracle's own name either.
        if let Some(of) = class.strip_suffix("-not-proven") {
            assert!(!out.has(of), "y={text}: {:?}", out.findings);
        }
    }
    // And a true panel has neither.
    for text in ["x^3", "atan(x)", "1/x", "sin(x)", "x^2-2", "x+1/x"] {
        let out = views::panel_planted(text, &set, &|_| {});
        assert!(out.findings.is_empty(), "y={text}: {:?}", out.findings);
    }
}

/// What the replay says of a row is a finding where it is against the row
/// from what the claims prove, and known-open where it leaves the row
/// open; and a certificate with claims the replay doesn't read is
/// replayed without them.
#[test]
#[cfg(feature = "mpfr-oracle")]
fn row_problems_are_found_by_what_they_say() {
    for (text, class) in [
        // The right limit of 3 + (x − (1 + 2·10⁻¹⁵))·e⁻ˣ + (x − x) at 14
        // digits, as main certified it.
        (
            "horizontal: the limit at +∞: [2.9999999999999996e0, 2.9999999999999996e0] is \
             narrower than the claims show",
            "replay/row-narrower",
        ),
        (
            "monotonicity: f′ is negative on [1e0, 2e0], inside a piece listed increasing",
            "replay/row-against",
        ),
        (
            "extrema: maximum at [1e0, 1e0] proven by the claims but not listed",
            "replay/row-against",
        ),
        (
            "vertical: f is unbounded at 0e0, but no asymptote is listed there",
            "replay/row-against",
        ),
        (
            "extrema: maximum at [8.4e-1, 8.5e-1] listed but not proven by the claims",
            "replay/row-problem",
        ),
        (
            "monotonicity: its claims leave [8.4e-1, 8.5e-1] of the region Kink undecided",
            "replay/row-problem",
        ),
        (
            "domain: f is not shown defined on its piece: not shown valid on [0e0, 1e0]",
            "replay/row-problem",
        ),
        (
            "period: the window [0e0, 3e0] is not as wide as the period, in [6.2e0, 6.3e0]",
            "replay/row-problem",
        ),
        // (Mirrors compared as listed: an excluded 0 enclosed off-centre.)
        (
            "parity: Even, but the domain is not symmetric about 0",
            "replay/row-problem",
        ),
        // A text the replay has learnt since: read, and added to a list.
        (
            "range: something new about the row",
            "replay/row-problem-unread",
        ),
    ] {
        assert_eq!(views::row_problem(text), class, "{text}");
    }
    let cert: serde_json::Value = serde_json::from_str(
        r#"{"extrema": {"Certified": {"value": [], "cert": {"claims": [
            {"KinkSides": {"order": 1}}, {"Kink": {"x": [0, 1]}}, {"KinkSides": {"order": 2}}
        ]}}}, "range": {"Certified": {"cert": {"claims": [{"Defined": [0, 1]}]}}}}"#,
    )
    .expect("JSON");
    let (rest, n) = views::without_unread(&cert);
    assert_eq!(n, 2);
    assert_eq!(
        rest["extrema"]["Certified"]["cert"]["claims"],
        serde_json::json!([{"Kink": {"x": [0, 1]}}])
    );
    assert_eq!(rest["range"], cert["range"]);
}

/// A positive radicand gives a genuine open neighborhood at x=2, however
/// small: its domain end is 2-10^-100. A fixed MPFR probe farther away
/// must not turn the correct derivative into a tester failure. The
/// neighbourhood is there and is proven, by f's enclosure over a box about
/// 2 inside it: the derivative at 2 is looked at (the point 2⁻²⁹⁹ below
/// it, where f is undefined, is what the count below is of), and nothing
/// is left open.
#[test]
#[cfg(feature = "mpfr-oracle")]
fn a_domain_edge_near_a_derivative_point_is_not_at_that_point() {
    use views::Beside;
    let set = Setup {
        digits: None,
        sliders: Vec::new(),
        unit: TrigUnit::Radians,
    };
    let out = views::run("sqrt(x-2+10^(-100))", None, &set, group::POINTS);
    assert!(out.findings.is_empty(), "{:?}", out.findings);
    assert!(out.tally["mpfr derivative points"] > 0);
    assert_eq!(
        out.tally["mpfr derivative points with a neighbourhood proven"],
        out.tally["mpfr derivative points"]
    );
    assert_eq!(
        out.tally["mpfr derivative neighbourhoods proven only nearer than 2⁻³⁰⁰ of the point"],
        1
    );
    assert!(out.notes.is_empty(), "{:?}", out.notes);

    // What is and isn't proven of the reals beside a point.
    let beside = |text: &str, x: f64| views::beside(text, &set, x).expect("a certificate");
    assert_eq!(beside("x^2", 0.0), Beside::Defined { bits: 300 });
    assert_eq!(beside("x^2", 1e300), Beside::Defined { bits: 300 });
    assert_eq!(
        beside("sqrt(x-2+10^(-100))", 2.0),
        Beside::Defined { bits: 1150 }
    );
    assert_eq!(
        beside("sqrt(x-2+10^(-1000))", 2.0),
        Beside::Defined { bits: 4900 }
    );
    // A domain that ends nearer than any distance tried: a neighbourhood
    // all the same, and nothing here proves it. Open, which is counted,
    // never a finding.
    assert_eq!(
        beside("sqrt(x-2+10^(-2000))", 2.0),
        Beside::Open { probe: Some(true) }
    );
    // The domain ends at the point: no neighbourhood. Proven at 0, where
    // the reals on one side can be enclosed without 0 itself; elsewhere a
    // box holds the point, and it is left open.
    assert_eq!(beside("x*sqrt(x)", 0.0), Beside::Undefined { below: true });
    assert_eq!(
        beside("x*sqrt(-x)", 0.0),
        Beside::Undefined { below: false }
    );
    assert_eq!(
        beside("(x-2)*sqrt(x-2)", 2.0),
        Beside::Open { probe: Some(true) }
    );
    // Defined at 0 alone (review 15): undefined on each side, the first
    // tried is told.
    assert_eq!(beside("sqrt(-x^2)", 0.0), Beside::Undefined { below: true });
}
