//! One function under one setting, every pair of the engine's views of it
//! that should agree, checked against each other:
//!
//! * (a) the compiled point evaluator, scalar and batched
//!   (`Program::compile_typed`);
//! * (b) the typed reference evaluator (`analysis::truth::reval_typed`);
//! * (c) the interval form with its decorations (`interval::enclose`,
//!   `interval::taylor`), on the tree as typed;
//! * (d) the certifier and the panel written from it (`analysis::analyze`,
//!   `certify::certify_equation`);
//! * (e) the independent MPFR replay of the certificate (`tests/replay`,
//!   `--features mpfr-oracle`), strictly, and that checker's own
//!   evaluation of f at points, which shares no arithmetic and no reading
//!   of the literals with (a), (b) or (c);
//! * (f) the plotter's drawn points (`plot::plot`).
//!
//! A finding's class starts with its group: `pt/` (a, b and c at points),
//! `mp/` (those against MPFR), `panel/` (d against a and b sampled: the
//! oracle's checks, `examples/oracle/checks.rs`, and d against c),
//! `replay/` (e), `digits/` (one function typed under two settings),
//! `pair/` (two spellings of one function), `plot/` (f against a, b and c).

use std::collections::BTreeMap;

use graphing::analysis::truth::{R, reval_typed};
use graphing::analysis::{AnalysisError, KeyGraphFeatures, analyze, analyze_certified};
use graphing::ast::{BinOp, Expr};
use graphing::certify::{Analysis, Row};
use graphing::compile::{CompileOptions, Input, Sliders};
use graphing::interval::{Ctx, Dec, DecInterval, Interval, enclose};
use graphing::lexer::ParseOptions;
use graphing::plot::{PlotOptions, plot};
use graphing::{Equation, TrigUnit, Viewport};

use crate::checks::{self, Density, F, K, NoFunction, Setting, Val};
use crate::generate::{rounded_text, slider_unrounded, unrounded};

/// What a function is read and evaluated under.
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    /// The digit limit on numbers typed (`None`: off).
    pub digits: Option<u8>,
    pub sliders: Vec<(String, f64)>,
    pub unit: TrigUnit,
}

impl Setup {
    fn parse(&self) -> ParseOptions {
        ParseOptions {
            decimal_comma: false,
            literal_digits: self.digits,
        }
    }

    fn vars(&self) -> BTreeMap<String, f64> {
        self.sliders.iter().cloned().collect()
    }
}

/// The groups of checks ([`run`] takes any of them).
pub mod group {
    pub const POINTS: u32 = 1;
    pub const PANEL: u32 = 2;
    pub const REPLAY: u32 = 4;
    pub const DIGITS: u32 = 8;
    pub const PAIR: u32 = 16;
    pub const PLOT: u32 = 32;
    pub const ALL: u32 = 63;

    /// The group a finding's class belongs to.
    pub fn of(class: &str) -> u32 {
        match class.split('/').next().unwrap_or("") {
            "pt" | "mp" => POINTS,
            "panel" => PANEL,
            "replay" => REPLAY,
            "digits" => DIGITS,
            "pair" => PAIR,
            "plot" => PLOT,
            _ => ALL,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub class: String,
    pub detail: String,
}

/// What the checks of one function found and counted.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// Disagreements: failures.
    pub findings: Vec<Finding>,
    /// Counted, not failures: differences the engine documents (a scalar
    /// outside a tight enclosure by its own rounding), claims about the
    /// counting functions the replay doesn't model.
    pub notes: BTreeMap<String, usize>,
    /// How much was checked.
    pub tally: BTreeMap<String, usize>,
    /// The first of each note, for looking into.
    pub note_samples: BTreeMap<String, String>,
}

impl Outcome {
    /// At most three findings a class: a function wrong at one point is
    /// wrong at many.
    pub fn fail(&mut self, class: &str, detail: String) {
        if self.findings.iter().filter(|f| f.class == class).count() < 3 {
            self.findings.push(Finding {
                class: class.to_string(),
                detail,
            });
        }
    }

    pub fn note(&mut self, class: &str, detail: impl FnOnce() -> String) {
        *self.notes.entry(class.to_string()).or_insert(0) += 1;
        if !self.note_samples.contains_key(class) {
            self.note_samples.insert(class.to_string(), detail());
        }
    }

    pub fn count(&mut self, what: &str, n: usize) {
        *self.tally.entry(what.to_string()).or_insert(0) += n;
    }

    pub fn has(&self, class: &str) -> bool {
        self.findings.iter().any(|f| f.class == class)
    }
}

fn panic_text(e: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = e.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else {
        "a panic".into()
    }
}

/// Runs `f`, timed; a panic inside is a finding `panic/<name>`.
fn guarded(out: &mut Outcome, name: &str, f: impl FnOnce(&mut Outcome)) {
    let t0 = std::time::Instant::now();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut *out)));
    if let Err(e) = r {
        out.fail(&format!("panic/{name}"), panic_text(e));
    }
    out.count(&format!("ms in {name}"), t0.elapsed().as_millis() as usize);
}

/// `y = text` under `set`, as the oracle's checks load it.
struct Loaded {
    eq: Equation,
    f: F,
    sliders: Sliders,
    unit: TrigUnit,
}

impl Loaded {
    fn opts(&self) -> CompileOptions<'_> {
        CompileOptions {
            trig_unit: self.unit,
            variables: &self.sliders,
        }
    }

    /// f's tree as typed (its sliders by name).
    fn ast(&self) -> &Expr {
        self.eq.explicit().expect("an explicit function").1
    }
}

/// The replay's own process ([`mp::replayed`] starts it).
#[cfg(feature = "mpfr-oracle")]
pub fn replay_child() {
    mp::replay_child();
}

fn load(text: &str, set: &Setup) -> Result<Loaded, NoFunction> {
    let vars = set.vars();
    let setting = Setting {
        unit: set.unit,
        parse: set.parse(),
        vars: &vars,
        density: Density::Quarter,
    };
    let (eq, f, sliders) = checks::load(text, &setting)?;
    Ok(Loaded {
        eq,
        f,
        sliders,
        unit: set.unit,
    })
}

// ------------------------------------------------------------------ points

fn ulp(x: f64) -> f64 {
    checks::ulp(x)
}

/// Where f is looked at: a grid about 0 (the integers and halves the
/// counting functions and steps turn on), the integers further out,
/// magnitudes from 10⁻⁸ to 10¹⁵ and a few beyond, and each x-free
/// subtree's value with its neighbours.
fn sample_xs(f: &F) -> Vec<f64> {
    let mut xs: Vec<f64> = Vec::new();
    for i in -64..=64 {
        xs.push(f64::from(i) / 8.0);
    }
    for i in -40..=40 {
        xs.push(f64::from(i));
    }
    for k in -32..=60 {
        let v = 10f64.powf(f64::from(k) / 4.0);
        xs.push(v);
        xs.push(-v);
    }
    for v in [
        61.0,
        62.0,
        100.0,
        170.0,
        171.0,
        172.0,
        1000.0,
        1_000_001.0,
        9_007_199_254_740_992.0,
        9_007_199_254_740_994.0,
        1e16,
        1e17,
        1e100,
        1e300,
        5e-324,
        f64::MIN_POSITIVE,
        0.1,
        0.3,
        1.000_000_000_000_000_2,
        0.999_999_999_999_999_9,
        std::f64::consts::PI,
        std::f64::consts::FRAC_PI_2,
    ] {
        xs.push(v);
        xs.push(-v);
    }
    let mut cs = Vec::new();
    checks::constants(&f.ast, f.unit, &mut cs);
    cs.sort_by(f64::total_cmp);
    cs.dedup();
    for &c in cs.iter().take(12) {
        for s in [c, -c] {
            xs.extend([s, checks::step(s, 1), checks::step(s, -1), s + 0.5, s - 0.5]);
        }
    }
    xs.retain(|x| x.is_finite());
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    xs
}

/// Whether `e` is arithmetic on numbers alone (sums, products,
/// quotients, whole powers), which is folded exactly as typed and
/// rounded once.
fn literal_arithmetic(e: &Expr) -> bool {
    match e {
        Expr::Num(..) => true,
        Expr::Neg(a) => literal_arithmetic(a),
        Expr::Bin(BinOp::Pow, a, b) => {
            literal_arithmetic(a) && matches!(**b, Expr::Num(n, _) if n == n.trunc())
        }
        Expr::Bin(_, a, b) => literal_arithmetic(a) && literal_arithmetic(b),
        _ => false,
    }
}

/// `e` with each number that is not exactly its double, and each piece of
/// arithmetic on numbers alone, written `v·1`: to the oracle's first-order
/// rounding bound (`F::error`, which takes numbers for exact) the
/// product's rounding stands for the one rounding of the value, half an
/// ulp of it. The numbers stay as typed, so the bound is of f itself:
/// with each at its double, 1.0000000000000001·10¹⁶ − 10¹⁶ was 0 for the
/// 1 it is, 0.25/(that − tanh x) was −0.25 for 3·10⁹ at 12, and the
/// bound, 6.8, a thousandth of what 1 − tanh 12 costs in doubles.
fn noisy(e: &Expr) -> Expr {
    if literal_arithmetic(e) {
        return if matches!(e, Expr::Num(_, lit) if lit.is_exact()) {
            e.clone()
        } else {
            Expr::bin(BinOp::Mul, e.clone(), Expr::exact(1.0))
        };
    }
    match e {
        Expr::Neg(a) => Expr::Neg(Box::new(noisy(a))),
        Expr::Degrees(a) => Expr::Degrees(Box::new(noisy(a))),
        Expr::Bin(op, a, b) => Expr::bin(*op, noisy(a), noisy(b)),
        Expr::Call(f, args) => Expr::Call(*f, args.iter().map(noisy).collect()),
        other => other.clone(),
    }
}

/// How far the compiled value may sit from f's true value at x by its own
/// rounding: four ulp and twice the first-order bound on the rounding of
/// each operation and each inexact literal.
fn rounding(l: &Loaded, noisy_ast: &Expr, x: f64, c: f64) -> f64 {
    let (_, err) = l.f.error(noisy_ast, x);
    4.0 * ulp(c) + 2.0 * err
}

/// Whether the compiled value `c` and the reference's `p`, two numbers,
/// are apart by no more than the rounding of both ([`rounding`]: its
/// first-order bound carries each operation's error through the slopes
/// of what follows, so an ill-conditioned f is allowed what its
/// conditioning costs, and no more).
fn within_both_roundings(l: &Loaded, noisy_ast: &Expr, x: f64, c: f64, p: f64) -> bool {
    c.is_finite() && p.is_finite() && (c - p).abs() <= 2.0 * rounding(l, noisy_ast, x, c)
}

/// Whether the compiled value `c` is f's, which MPFR encloses in
/// `[lo, hi]`, to a few ulps: what the oracle's checks, which judge f by
/// its doubles, take for granted. (Not within its own rounding bound,
/// which is the size of what cancelled: off, 2⁵³ + 1 times cosh x less 2⁵³
/// times it is cosh x, and 0 in doubles with a bound of 8.)
fn faithful(c: f64, lo: f64, hi: f64) -> bool {
    if c.is_nan() {
        return false;
    }
    if lo <= c && c <= hi {
        return true;
    }
    // (The nearer end: the other may be beyond the doubles.)
    let end = if c < lo { lo } else { hi };
    c.is_finite() && end.is_finite() && (c - end).abs() <= 8.0 * ulp(c.abs().max(end.abs()))
}

fn distance(c: f64, lo: f64, hi: f64) -> f64 {
    if c < lo {
        lo - c
    } else if c > hi {
        c - hi
    } else {
        0.0
    }
}

fn show_enc(e: &DecInterval) -> String {
    if e.is_empty() {
        format!("∅ {:?}", e.dec)
    } else {
        format!("[{:e}, {:e}] {:?}", e.lo(), e.hi(), e.dec)
    }
}

struct Sampled {
    xs: Vec<f64>,
    vals: Vec<Val>,
    /// f's point enclosure at each (for MPFR's word against it).
    #[cfg_attr(not(feature = "mpfr-oracle"), allow(dead_code))]
    encs: Vec<DecInterval>,
}

/// (a) against (b) and (c) at points and over boxes.
fn points(l: &Loaded, out: &mut Outcome) -> Sampled {
    let opts = l.opts();
    let ctx = Ctx::new(opts);
    let ast = l.ast();
    let f = &l.f;
    let xs = sample_xs(f);
    let noisy_ast = noisy(&f.ast);
    out.count("points", xs.len());

    // The batched path gives the scalar path's bits.
    let mut batch = vec![0.0; xs.len()];
    f.prog
        .eval_batch(Input::Slice(&xs), Input::Scalar(0.0), &mut batch);

    let mut vals = Vec::with_capacity(xs.len());
    let mut encs = Vec::with_capacity(xs.len());
    for (i, &x) in xs.iter().enumerate() {
        let mut v = f.at(x);
        let c = v.c;
        if batch[i].to_bits() != c.to_bits() && !(batch[i].is_nan() && c.is_nan()) {
            out.fail(
                "pt/scalar-vs-batch",
                format!("x={x:e}: scalar {c:e}, batched {:e}", batch[i]),
            );
        }
        if v.k == K::Disagree && c.is_nan() && f.prog.undecided(x, 0.0) {
            // The scalar's NaN says "can't tell" (an intermediate beyond
            // the doubles): no claim against the reference's value.
            out.note("pt/scalar-cannot-tell", || {
                format!(
                    "x={x:e}: compiled NaN (can't tell), reference {:?}",
                    reval_typed(&f.ast, x, l.unit)
                )
            });
            v.k = K::Unknown;
        }
        // Two double evaluations of f, each within its own rounding of
        // f(x), are apart by no more than both, which is no disagreement
        // (the oracle's 8 ulp are for a well-conditioned f): (3 − 7x)·(1 −
        // cos(log x)) at 5·10⁻³²⁴, where an ulp of the logarithm, −323.3,
        // is 45 of the result's, had them 53 ulps apart. (A compiled value
        // outside f's own enclosure is found below, whatever the
        // reference's.)
        if v.k == K::Disagree && within_both_roundings(l, &noisy_ast, x, c, v.v) {
            out.note("pt/scalar-vs-reference-within-their-rounding", || {
                format!(
                    "x={x:e}: compiled {c:e}, reference {:e}, each within {:e} of f",
                    v.v,
                    rounding(l, &noisy_ast, x, c)
                )
            });
            v.k = K::Def;
            v.v = c;
        }
        if v.k == K::Disagree {
            out.fail(
                "pt/scalar-vs-reference",
                format!(
                    "x={x:e}: compiled {c:e}, reference {:?}",
                    reval_typed(&f.ast, x, l.unit)
                ),
            );
        }
        let e = enclose(ast, Interval::point(x), &ctx);
        if e.dec == Dec::Ill {
            out.fail("pt/ill-enclosure", format!("x={x:e}"));
        } else if e.is_empty() {
            // Proven undefined: nothing may give a number.
            if !c.is_nan() {
                out.fail(
                    "pt/number-where-enclosure-empty",
                    format!("x={x:e}: compiled {c:e}, enclosure ∅"),
                );
            } else if let R::V(r) = reval_typed(&f.ast, x, l.unit) {
                out.fail(
                    "pt/reference-where-enclosure-empty",
                    format!("x={x:e}: reference {:e}, enclosure ∅", r.f()),
                );
            }
        } else {
            if e.dec >= Dec::Def && v.k == K::Undef {
                // (A compiled NaN that says "can't tell" leaves the
                // reference alone against the enclosure.)
                let class = if f.prog.undecided(x, 0.0) {
                    "pt/reference-undefined-where-enclosure-defined"
                } else {
                    "pt/undefined-where-enclosure-defined"
                };
                out.fail(
                    class,
                    format!(
                        "x={x:e}: compiled NaN, reference undefined, enclosure {}",
                        show_enc(&e)
                    ),
                );
            }
            if v.k == K::Def && c.is_finite() {
                let d = distance(c, e.lo(), e.hi());
                if d > 0.0 {
                    let allow = rounding(l, &noisy_ast, x, c);
                    if d > allow {
                        out.fail(
                            "pt/scalar-outside-enclosure",
                            format!(
                                "x={x:e}: compiled {c:e} is {d:e} outside {} (its rounding: {allow:e})",
                                show_enc(&e)
                            ),
                        );
                    } else {
                        out.note("pt/scalar-outside-enclosure-by-rounding", || {
                            format!("x={x:e}: compiled {c:e}, {d:e} outside {}", show_enc(&e))
                        });
                    }
                }
                if (e.pos && c < 0.0) || (e.neg && c > 0.0) {
                    out.fail(
                        "pt/sign-fact-against-scalar",
                        format!(
                            "x={x:e}: compiled {c:e}, enclosure {} pos={} neg={}",
                            show_enc(&e),
                            e.pos,
                            e.neg
                        ),
                    );
                }
            }
            // ±∞ stands for a value beyond the doubles: the enclosure
            // reaches there.
            if v.k == K::Def
                && c.is_infinite()
                && ((c > 0.0 && e.hi() < f64::MAX) || (c < 0.0 && e.lo() > -f64::MAX))
            {
                out.fail(
                    "pt/overflow-outside-enclosure",
                    format!("x={x:e}: compiled {c:e}, enclosure {}", show_enc(&e)),
                );
            }
        }
        vals.push(v);
        encs.push(e);
    }

    // Boxes between neighbouring samples: a value at an end lies in the
    // box's enclosure; an empty one has both ends undefined, a defined one
    // neither.
    let mut boxes = 0;
    for i in 0..xs.len().saturating_sub(1) {
        let (a, b) = (xs[i], xs[i + 1]);
        if b - a > 0.25 && !i.is_multiple_of(7) {
            continue;
        }
        boxes += 1;
        let e = enclose(ast, Interval::new(a, b), &ctx);
        for j in [i, i + 1] {
            let (x, v) = (xs[j], &vals[j]);
            if e.is_empty() {
                if v.k == K::Def {
                    out.fail(
                        "pt/box-empty-but-defined",
                        format!("[{a:e}, {b:e}] encloses as ∅, but f({x:e}) = {:e}", v.c),
                    );
                }
                continue;
            }
            if e.dec >= Dec::Def && v.k == K::Undef {
                out.fail(
                    "pt/box-defined-but-undefined",
                    format!(
                        "[{a:e}, {b:e}] encloses as {}, but f({x:e}) is undefined",
                        show_enc(&e)
                    ),
                );
            }
            if v.k == K::Def && v.c.is_finite() {
                let d = distance(v.c, e.lo(), e.hi());
                if d > 0.0 && d > rounding(l, &noisy_ast, x, v.c) {
                    out.fail(
                        "pt/box-excludes-value",
                        format!(
                            "[{a:e}, {b:e}] encloses as {}, but f({x:e}) = {:e}",
                            show_enc(&e),
                            v.c
                        ),
                    );
                }
            }
        }
    }
    out.count("boxes", boxes);
    Sampled { xs, vals, encs }
}

// -------------------------------------------------------------------- MPFR

#[cfg(feature = "mpfr-oracle")]
mod mp {
    use super::*;
    use crate::replay::{self, Fx, iv};
    use graphing::interval::{derivs_valid, taylor};
    use rug::Float;
    use rug::float::Round;

    /// How long a replay is waited for (`CROSSFUZZ_REPLAY_SECONDS`).
    fn replay_seconds() -> u64 {
        std::env::var("CROSSFUZZ_REPLAY_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30)
    }

    /// The precision f is evaluated with at points: enough to place
    /// sin²4 + cos²4 − 1 − 10⁻¹⁰⁰ below 0.
    const PREC: u32 = 1280;

    fn at_prec<T>(f: impl FnOnce() -> T) -> T {
        let before = iv::prec();
        iv::set_prec(PREC);
        let r = f();
        iv::set_prec(before);
        r
    }

    fn bounds(v: &iv::Iv) -> (f64, f64) {
        (v.lo.to_f64_round(Round::Down), v.hi.to_f64_round(Round::Up))
    }

    fn show(v: &iv::Iv) -> String {
        if v.empty {
            "∅".into()
        } else {
            let (lo, hi) = bounds(v);
            format!(
                "[{lo:e}, {hi:e}]{}",
                if v.def {
                    " defined"
                } else {
                    " maybe undefined"
                }
            )
        }
    }

    /// What three values say of a slow infinite limit: within 10⁻⁶ of
    /// each other next to the point they are "f settles" to the replay,
    /// which leaves the limit open as spoken against
    /// (tests/replay/claims.rs), and a·ln x for a small a does that at
    /// every double there is. Where the
    /// replay's own (x − a)·f′(x) at those three points (x·f′(x) for a
    /// limit far out) keeps one sign, the one the limit needs, and
    /// doesn't shrink toward the point, f is still moving as a logarithm
    /// does, and this says so: the values aren't against the limit. (For
    /// a function that does settle it tends to 0, and nothing is said.)
    fn still_moving(fx: &Fx, claim: &replay::Claim, note: &str) -> Option<String> {
        let replay::Claim::Simplifier(fact) = claim else {
            return None;
        };
        let (to, at) = fact.strip_prefix("f → ")?.split_once(" as x → ")?;
        let up = match to {
            "+∞" => true,
            "−∞" => false,
            _ => return None,
        };
        if !note.contains("f settles (") {
            return None;
        }
        let at = at.split(':').next()?.trim();
        // The point approached: finite (from one side), or far out.
        let a = match at {
            "+∞" | "−∞" => None,
            _ => Some(
                at.trim_end_matches(['⁺', '⁻'])
                    .replace('−', "-")
                    .parse::<f64>()
                    .ok()?,
            ),
        };
        let xs: Vec<f64> = note
            .split(" at ")
            .skip(1)
            .filter_map(|t| {
                t.split([',', ')'])
                    .next()?
                    .trim()
                    .replace('−', "-")
                    .parse()
                    .ok()
            })
            .collect();
        if xs.len() != 3 {
            return None;
        }
        // (x − a)·f′(x) > 0 beside a is f falling toward it; x·f′(x) > 0
        // far out is f rising outward.
        let want_positive = if a.is_some() { !up } else { up };
        let mut g = Vec::new();
        for &x in &xs {
            let d = at_prec(|| fx.series(&fx.f, x, x, 1)).swap_remove(1);
            if d.empty || !d.def {
                return None;
            }
            let (lo, hi) = bounds(&d);
            let w = x - a.unwrap_or(0.0);
            let (glo, ghi) = if w >= 0.0 {
                (lo * w, hi * w)
            } else {
                (hi * w, lo * w)
            };
            if !(glo.is_finite() && ghi.is_finite())
                || (want_positive && glo <= 0.0)
                || (!want_positive && ghi >= 0.0)
            {
                return None;
            }
            g.push(if want_positive { glo } else { -ghi });
        }
        (g[2] >= g[0] / 2.0).then(|| {
            format!(
                "the replay's own (x − a)·f′(x) at those points is {:e}, {:e}, {:e} in size: f is still moving as a logarithm does",
                g[0], g[1], g[2]
            )
        })
    }

    /// The replay's reading of the function, from the certificate: its
    /// own scan of the source's numbers, checked against the parser's.
    pub fn function(json: &serde_json::Value, out: &mut Outcome) -> Option<Fx> {
        iv::init();
        match replay::function(json) {
            Ok(fx) => Some(fx),
            Err(e) => {
                out.fail("replay/rejected", e);
                None
            }
        }
    }

    /// Whether the replay's evaluator models f (not the counting
    /// functions, nor a degree mark outside degrees mode).
    pub fn models(fx: &Fx) -> bool {
        !replay::claims::unmodelled(&fx.f) && fx.refused.is_none()
    }

    /// Whether a value `c` computed at the double x says anything of
    /// f(x): not where f over the doubles next to x (MPFR's enclosure of
    /// it over [x − ulp, x + ulp]) is undefined somewhere, unbounded or
    /// holds `c`. An intermediate rounded by half an ulp is x moved by
    /// about as much: where that changes f past `c`, the doubles don't
    /// resolve f there (cos(x²) far out, mod(x, 1/x)).
    fn resolved(fx: &Fx, x: f64, c: f64) -> bool {
        let u = checks::ulp(x);
        let m = at_prec(|| fx.series(&fx.f, x - u, x + u, 0)).swap_remove(0);
        if m.empty || !m.def {
            return false;
        }
        let (lo, hi) = bounds(&m);
        !(lo <= c && c <= hi)
    }

    /// MPFR's word on f at x ([`MpAt`]).
    pub fn at(fx: &Fx, x: f64) -> MpAt {
        let m = at_prec(|| fx.series(&fx.f, x, x, 0)).swap_remove(0);
        MpAt {
            undefined: if m.empty {
                Some(true)
            } else if m.def {
                Some(false)
            } else {
                None
            },
            value: (!m.empty && m.def).then(|| bounds(&m)),
        }
    }

    /// (a), (b) and (c) against the replay's own evaluation of f at
    /// points, and the Taylor form's derivatives against its series.
    pub fn points(l: &Loaded, fx: &Fx, s: &Sampled, out: &mut Outcome) {
        if !models(fx) {
            out.count("mpfr: functions the replay doesn't model", 1);
            return;
        }
        let opts = l.opts();
        let ctx = Ctx::new(opts);
        let ast = l.ast();
        let noisy_ast = noisy(&l.f.ast);
        let mut checked = 0usize;
        let mut derivs = 0;
        // Points MPFR proves f defined at, and those of them where the
        // doubles have it undefined.
        let (mut defined, mut lost) = (0usize, 0usize);
        let mut lost_at = String::new();
        // Points MPFR proves f undefined at, and those of them where the
        // doubles have a value.
        let (mut undefined, mut phantom) = (0usize, 0usize);
        let mut phantom_at = String::new();
        for (i, &x) in s.xs.iter().enumerate() {
            // The grid about 0 by halves, the integers, and every fourth
            // of the rest.
            let near = x.abs() <= 12.0 && (2.0 * x).fract() == 0.0;
            if !near && !i.is_multiple_of(4) {
                continue;
            }
            checked += 1;
            let (v, e) = (&s.vals[i], &s.encs[i]);
            let c = v.c;
            let m = at_prec(|| fx.series(&fx.f, x, x, 0)).swap_remove(0);
            if m.empty {
                if !e.is_empty() && e.dec >= Dec::Def {
                    out.fail(
                        "mp/enclosure-defined-where-undefined",
                        format!("x={x:e}: enclosure {}, MPFR: f is undefined", show_enc(e)),
                    );
                }
                // A number there is the doubles' (√(g) where g rounds to
                // 0 from below): no claim about f unless f's enclosure
                // says so too (above). Counted.
                undefined += 1;
                let reference = || match reval_typed(&l.f.ast, x, l.unit) {
                    R::V(r) => Some(r.f()),
                    _ => None,
                };
                if !c.is_nan() || reference().is_some() {
                    phantom += 1;
                    if phantom_at.is_empty() {
                        phantom_at = format!(
                            "x={x:e}: compiled {c:e}, reference {:?}, MPFR: f is undefined",
                            reference()
                        );
                    }
                    out.note("mp/number-in-doubles-where-undefined", || {
                        phantom_at.clone()
                    });
                }
                continue;
            }
            if !m.def {
                continue;
            }
            let (lo, hi) = bounds(&m);
            if e.is_empty() {
                out.fail(
                    "mp/enclosure-empty-where-defined",
                    format!("x={x:e}: enclosure ∅, MPFR: f = {}", show(&m)),
                );
            } else if e.hi() < lo || e.lo() > hi {
                out.fail(
                    "mp/enclosure-misses-value",
                    format!("x={x:e}: enclosure {}, MPFR: f = {}", show_enc(e), show(&m)),
                );
            }
            defined += 1;
            if v.k == K::Undef {
                // In doubles an operand can round onto a pole or a domain
                // end (1 − cos x at a tiny x): undefined there is the
                // doubles', not a claim about f, unless f's enclosure
                // says so too (checked above). Counted.
                lost += 1;
                if lost_at.is_empty() {
                    lost_at = format!(
                        "x={x:e}: compiled NaN, reference undefined, MPFR: f = {}",
                        show(&m)
                    );
                }
                out.note("mp/undefined-in-doubles-where-defined", || lost_at.clone());
            }
            if v.k == K::Def && c.is_finite() && lo.is_finite() && hi.is_finite() {
                let d = distance(c, lo, hi);
                if d > 0.0 {
                    let allow = rounding(l, &noisy_ast, x, c);
                    let line = || {
                        format!(
                            "x={x:e}: compiled {c:e}, MPFR: f = {} ({d:e} apart; its rounding: {allow:e}; enclosure {})",
                            show(&m),
                            show_enc(e)
                        )
                    };
                    // Within f's own enclosure the scalar is "all that is
                    // known within a wide one" (`plot::drawn_value`): the
                    // doubles' arithmetic on x and on literals no double
                    // holds, which the interval form encloses. Outside it,
                    // or where the enclosure says nothing, only its own
                    // rounding is allowed, unless f isn't resolved by a
                    // double x at all ([`resolved`]).
                    let inside = !e.iv.is_entire() && e.lo() <= c && c <= e.hi();
                    if d <= allow {
                    } else if inside {
                        out.note("mp/scalar-off-value-within-enclosure", line);
                    } else if !resolved(fx, x, c) {
                        // (cos(x²) at 3·10¹⁴: x² is rounded by more than a
                        // period.)
                        out.note("mp/scalar-off-value-below-the-doubles-resolution", line);
                    } else {
                        out.fail("mp/scalar-off-value", line());
                    }
                }
            }
            if v.k == K::Def
                && c.is_infinite()
                && (c > 0.0) != (lo > 0.0)
                && lo.is_finite()
                && hi.is_finite()
                && lo.abs() < f64::MAX
                && hi.abs() < f64::MAX
            {
                out.fail(
                    "mp/scalar-off-value",
                    format!("x={x:e}: compiled {c:e}, MPFR: f = {}", show(&m)),
                );
            }

            // Derivatives, at a third of these points.
            if !checked.is_multiple_of(3) {
                continue;
            }
            let t = taylor(ast, Interval::point(x), 2, &ctx);
            if !derivs_valid(&t, 1) {
                continue;
            }
            derivs += 1;
            let ms = at_prec(|| fx.series(&fx.f, x, x, 2));
            for k in 1..=2 {
                if !derivs_valid(&t, k) || ms[k].empty || !ms[k].def {
                    continue;
                }
                let (lo, hi) = bounds(&ms[k]);
                if t[k].hi() < lo || t[k].lo() > hi {
                    out.fail(
                        "mp/derivative-misses-value",
                        format!(
                            "x={x:e}: Taylor coefficient {k} enclosed as {}, MPFR: {}",
                            show_enc(&t[k]),
                            show(&ms[k])
                        ),
                    );
                }
            }
            // A derivative at x needs f defined on both sides of x.
            for side in [-1.0, 1.0] {
                let beside = at_prec(|| {
                    let mut p = Float::with_val(PREC, x);
                    // 2⁻³⁰⁰ of x, and no more than 2⁻²⁵⁰ (far out, a
                    // fraction of x is many periods of a tangent).
                    let step = if x == 0.0 {
                        Float::with_val(PREC, f64::MIN_POSITIVE) >> 200u32
                    } else {
                        let rel = Float::with_val(PREC, x.abs()) >> 300u32;
                        let abs = Float::with_val(PREC, 1.0) >> 250u32;
                        if rel < abs { rel } else { abs }
                    };
                    p += step * side;
                    fx.series_iv(&fx.f, iv::Iv::point(p), 0).swap_remove(0)
                });
                if beside.empty {
                    out.fail(
                        "mp/derivative-without-neighbourhood",
                        format!(
                            "x={x:e}: f′ enclosed as {}, but MPFR: f is undefined just {} it",
                            show_enc(&t[1]),
                            if side < 0.0 { "below" } else { "above" }
                        ),
                    );
                }
            }
        }
        // The doubles losing f at most of the points looked at: still the
        // doubles' (7/(1 − cos(x/2³¹)) is undefined in them for |x| < 32,
        // where the cosine rounds to 1; off, √(0.99999999999999999·e⁷ − e⁷)
        // is 0 in them everywhere and defined nowhere), and f's enclosure
        // says "maybe" throughout. Counted apart, to be seen.
        if lost >= 8 && 2 * lost > defined {
            out.note(
                "mp/undefined-in-doubles-at-most-points-where-defined",
                || format!("{lost} of {defined} points MPFR proves f defined at, e.g. {lost_at}"),
            );
        }
        if phantom >= 8 && 2 * phantom > undefined {
            out.note("mp/number-in-doubles-at-most-points-where-undefined", || {
                format!(
                    "{phantom} of {undefined} points MPFR proves f undefined at, e.g. {phantom_at}"
                )
            });
        }
        out.count("mpfr points", checked);
        out.count("mpfr derivative points", derivs);
    }

    /// (e): the certificate replayed strictly. A claim refuted, a row that
    /// doesn't follow, a certifier's tree shown wrong, and a claim behind
    /// a Certified or Partial row left open all fail; a claim about the
    /// counting functions, which the replay doesn't model, is counted.
    ///
    /// The replay has no budget of its own and can't be cancelled (a
    /// claim it can't decide may be bisected for minutes): it runs in a
    /// process of its own (this test binary's `replay_child`, which
    /// [`replay_child`] is), stopped after [`replay_seconds`] and counted
    /// (`replay/gave-up-waiting`: a matter of time, so not a failure).
    /// (On a thread it ran on after it was given up on, and a run's slow
    /// replays piled up until the others had to be skipped.)
    ///
    /// A limit the replay leaves open with f's values against it fails
    /// the run (`replay/values-against`), unless f is still moving there
    /// as a logarithm does ([`still_moving`]).
    pub fn replayed(json: &serde_json::Value, fx: &Fx, out: &mut Outcome) {
        use std::io::{Read, Write};
        use std::process::{Command, Stdio};
        let spawned = std::env::current_exe().and_then(|exe| {
            Command::new(exe)
                .args(["replay_child", "--exact", "--ignored", "--nocapture"])
                .env(CHILD, "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
        });
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                out.fail("replay/no-process", e.to_string());
                return;
            }
        };
        // Fed and read on threads of their own: the wait is for the answer
        // alone.
        let (mut input, mut output) = (
            child.stdin.take().expect("piped"),
            child.stdout.take().expect("piped"),
        );
        let certificate = json.to_string();
        let feeder = std::thread::spawn(move || {
            let _ = input.write_all(certificate.as_bytes());
        });
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut said = String::new();
            let _ = output.read_to_string(&mut said);
            let _ = tx.send(said);
        });
        let said = rx.recv_timeout(std::time::Duration::from_secs(replay_seconds()));
        if said.is_err() {
            let _ = child.kill();
        }
        let status = child.wait();
        let _ = feeder.join();
        let Ok(said) = said else {
            out.note("replay/gave-up-waiting", || {
                format!("{} after {} s", fx.source, replay_seconds())
            });
            return;
        };
        let answer = said
            .lines()
            .find_map(|l| l.find(ANSWER).map(|i| &l[i + ANSWER.len()..]))
            .and_then(|a| serde_json::from_str::<serde_json::Value>(a).ok());
        let Some(answer) = answer else {
            out.fail(
                "panic/replay",
                format!("the replay's process ended with no answer ({status:?})"),
            );
            return;
        };
        for f in answer["findings"].as_array().into_iter().flatten() {
            if let (Some(class), Some(detail)) = (f[0].as_str(), f[1].as_str()) {
                out.fail(class, detail.to_string());
            }
        }
        for (class, n) in answer["notes"].as_object().into_iter().flatten() {
            *out.notes.entry(class.clone()).or_insert(0) += n.as_u64().unwrap_or(0) as usize;
            if let Some(sample) = answer["samples"][class.as_str()].as_str() {
                out.note_samples
                    .entry(class.clone())
                    .or_insert_with(|| sample.to_string());
            }
        }
        for (what, n) in answer["tally"].as_object().into_iter().flatten() {
            out.count(what, n.as_u64().unwrap_or(0) as usize);
        }
    }

    /// Set for the replay's process alone.
    const CHILD: &str = "CROSSFUZZ_REPLAY_CHILD";

    /// What the line the replay's process answers with starts with.
    const ANSWER: &str = "@@crossfuzz-replay@@";

    /// The replay's process: a certificate read from its input is
    /// replayed, and what that found and counted is its answer, one line.
    /// Nothing unless [`replayed`] started it. It ends by itself soon
    /// after the time it is given, so that none is left running when the
    /// tester that would have stopped it is stopped first.
    pub fn replay_child() {
        use std::io::Read;
        if std::env::var_os(CHILD).is_none() {
            return;
        }
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(replay_seconds() + 5));
            std::process::exit(3);
        });
        let mut certificate = String::new();
        std::io::stdin()
            .read_to_string(&mut certificate)
            .expect("a certificate on the input");
        let json: serde_json::Value = serde_json::from_str(&certificate).expect("JSON");
        let out = std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || {
                let mut out = Outcome::default();
                // (What reading the function finds, the tester has.)
                if let Some(fx) = function(&json, &mut Outcome::default()) {
                    judge(&json, &fx, &mut out);
                }
                out
            })
            .expect("a replay thread")
            .join()
            .unwrap_or_else(|e| {
                let mut out = Outcome::default();
                out.fail("panic/replay", panic_text(e));
                out
            });
        let findings: Vec<(&str, &str)> = out
            .findings
            .iter()
            .map(|f| (f.class.as_str(), f.detail.as_str()))
            .collect();
        println!(
            "\n{ANSWER}{}",
            serde_json::json!({
                "findings": findings,
                "notes": out.notes,
                "samples": out.note_samples,
                "tally": out.tally,
            })
        );
    }

    /// The certificate through the replay, and what it says onto `out`.
    fn judge(json: &serde_json::Value, fx: &Fx, out: &mut Outcome) {
        use replay::Class;
        let counting = replay::claims::unmodelled(&fx.f);
        let r =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| replay::replay(json))) {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    out.fail("replay/rejected", e);
                    return;
                }
                Err(e) => {
                    out.fail("panic/replay", panic_text(e));
                    return;
                }
            };
        for b in &r.binding {
            if b.starts_with("missing") {
                out.fail("replay/binding", b.clone());
            }
        }
        let shown = |name: &String| {
            r.rows.iter().any(|row| {
                row.name == *name && matches!(row.status.as_str(), "Certified" | "Partial")
            })
        };
        for c in &r.claims {
            out.count(&format!("claims {}", c.outcome.class.name()), 1);
            let line = || format!("{:?} ({}): {}", c.claim, c.rows.join(","), c.outcome.note);
            match c.outcome.class {
                Class::Strong | Class::Weak => {}
                Class::Refuted => out.fail("replay/refuted", line()),
                // A limit left open with f's values at the points tried
                // against it: no proof (so not refuted), but to look at
                // like one, not one of the replay's known limits. Unless
                // the values aren't against it: 10⁻¹⁶·ln x "settles" at
                // every double beside 0, and goes to −∞.
                Class::Unconfirmed if c.outcome.note.starts_with(replay::claims::AGAINST) => {
                    match still_moving(fx, &c.claim, &c.outcome.note) {
                        Some(more) => out.note("replay/values-of-a-slow-limit", || {
                            format!("{}; {more}", line())
                        }),
                        None => out.fail("replay/values-against", line()),
                    }
                }
                Class::Unsupported if counting => {
                    out.note("replay/unsupported-counting", line);
                }
                Class::Unsupported if c.rows.iter().any(shown) => {
                    out.fail("replay/unsupported", line());
                }
                Class::Unconfirmed if c.rows.iter().any(shown) => {
                    if counting {
                        out.note("replay/unconfirmed-counting", line);
                    } else {
                        out.fail("replay/unconfirmed", line());
                    }
                }
                Class::Unsupported | Class::Unconfirmed => {
                    out.note("replay/open-behind-unknown-row", line);
                }
            }
        }
        for p in r.row_problems() {
            if counting {
                out.note("replay/row-problem-counting", || p.clone());
            } else if p.contains("it rests on") && p.contains("refuted claims") {
                // (The refuted claims are findings already.)
                out.note("replay/row-rests-on-refuted-claims", || p.clone());
            } else {
                out.fail("replay/row-problem", p);
            }
        }
        out.count(
            "rows replayed",
            r.rows.iter().filter(|x| x.status != "Unknown").count(),
        );
    }
}

// ------------------------------------------------------------------- panel

/// MPFR's word on f at a point, where the replay's evaluator has one.
#[derive(Clone, Copy, Debug, Default)]
pub struct MpAt {
    /// Whether f is undefined there (`None`: it can't tell).
    pub undefined: Option<bool>,
    /// f's value there, enclosed in doubles, where it proves f defined.
    pub value: Option<(f64, f64)>,
}

type Mp<'a> = Option<&'a dyn Fn(f64) -> MpAt>;

/// Whether f is undefined at x, as far as the views that prove things
/// say: `Some(true)` where f's enclosure there is empty (or MPFR's is),
/// `Some(false)` where one proves f defined, `None` where neither tells.
/// The compiled program's NaN with the reference's "undefined" doesn't:
/// an operand can round onto a pole (ln(1 − x²)/ln(1 − x²) at 10⁻¹², where
/// 1 − x² is 1 in doubles).
fn proven_undefined(l: &Loaded, mp: Mp<'_>, x: f64) -> Option<bool> {
    let e = enclose(l.ast(), Interval::point(x), &Ctx::new(l.opts()));
    if e.is_empty() {
        return Some(true);
    }
    if e.dec >= Dec::Def {
        return Some(false);
    }
    mp.and_then(|m| m(x).undefined)
}

/// How densely the oracle's checks sample a function typed as `text`: a
/// quarter of their points, and a sixteenth where a ratio of long
/// integers is written (`x^(1/1000001)`: each value costs the reference
/// evaluator a hundred microseconds, and the checks take hundreds of
/// thousands).
fn density(text: &str) -> Density {
    let cs = text.as_bytes();
    let long = |digits: usize| digits >= 7;
    let ratio = (0..cs.len()).any(|i| {
        cs[i] == b'/' && {
            let before = cs[..i]
                .iter()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .count();
            let after = cs[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .count();
            long(before) || long(after)
        }
    });
    if ratio {
        Density::Sparse
    } else {
        Density::Quarter
    }
}

/// The items of a partial list ("these are some of them"): each is
/// proven, so each listed zero is a zero of f and each listed turn or
/// inflection has f's value. (The oracle's checks take complete answers
/// only.) Judged as the oracle judges a complete list's items, where the
/// doubles have f's value ([`Mp`], when there is one), and against (c):
/// an item fails only if f's enclosure over the doubles about it doesn't
/// hold what the item says either. (A zero of −|cos x|^(−p/7) + x³ beside
/// π, p past 2⁵³, is right to nine digits while f in doubles is −4.6
/// there: the cosine's last bit moves f by a seventh.)
fn partial_items(l: &Loaded, k: &KeyGraphFeatures, mp: Mp<'_>, out: &mut Outcome) {
    use graphing::analysis::flags;
    if k.analysis_error != AnalysisError::NoError || k.partial_features == 0 {
        return;
    }
    let d = &k.data;
    let f = &l.f;
    // Whether the doubles' f at x is f's (as far as MPFR tells).
    let sound = |x: f64| match mp.and_then(|m| m(x).value) {
        Some((lo, hi)) => faithful(f.prog.eval(x, 0.0), lo, hi),
        None => true,
    };
    let poles: Vec<graphing::analysis::Family> = d
        .vertical_asymptotes
        .iter()
        .chain(&d.excluded)
        .copied()
        .collect();
    let ctx = Ctx::new(l.opts());
    // Whether f's enclosure over the doubles about x holds y.
    let enclosed = |x: f64, y: f64| {
        let around = Interval::new(checks::step(x, -16), checks::step(x, 16));
        let e = enclose(l.ast(), around, &ctx);
        !e.is_empty() && e.lo() <= y && y <= e.hi()
    };
    if k.partial_features & flags::ZEROS != 0 {
        for z in &d.zeros {
            for x in checks::members(z) {
                out.count("items of partial lists", 1);
                if sound(x) && f.zero_holds(x, &poles) == Some(false) && !enclosed(x, 0.0) {
                    out.fail(
                        "panel/partial-zero-not-zero",
                        format!("{}: f({x:e}) = {:e}", k.x_intercept, f.at(x).v),
                    );
                }
            }
        }
    }
    for (flag, list, name) in [
        (flags::MINIMA, &d.minima, "minimum"),
        (flags::MAXIMA, &d.maxima, "maximum"),
        (flags::INFLECTION_POINTS, &d.inflection_points, "inflection"),
    ] {
        if k.partial_features & flag == 0 {
            continue;
        }
        for (fam, y) in list {
            for x in checks::members(fam) {
                out.count("items of partial lists", 1);
                let v = f.at(x);
                if !sound(x) {
                    continue;
                }
                if v.k == K::Undef && proven_undefined(l, mp, x) == Some(true) {
                    out.fail(
                        "panel/partial-item-undefined",
                        format!("{name} ({x:e}, {y}), where f is undefined"),
                    );
                } else if v.k == K::Def && v.v.is_finite() && !v.sat {
                    let wrong = if *y == 0.0 {
                        f.zero_holds(x, &poles) == Some(false)
                    } else {
                        (y - v.v).abs() > f.tol(x) && !checks::alike(*y, v.v)
                    };
                    if wrong && !enclosed(x, *y) {
                        out.fail(
                            "panel/partial-item-value-wrong",
                            format!("{name} ({x:e}, {y:e}), but f there = {:e}", v.v),
                        );
                    }
                }
            }
        }
    }
}

/// Whether the y-intercept, which the oracle's check found off f(0) (it
/// reads the panel's number, to six digits), is right as shown. A value
/// the certificate knows to an enclosure is shown to as many significant
/// digits (three to six) as every value in the enclosure rounds to alike
/// (`analysis::certified::approx`), marked "≈", and the panel's number is
/// the enclosure's middle: 10¹⁷·(b − 0.3) + ∛2⁵³ with a slider b at 0.3,
/// whose enclosure at 0 is five wide, is "≈208060" with the number
/// 208061.06, and f(0) is 208062.72. Right when the text is the
/// enclosure's ends to the digits they share, and f(0) (MPFR's, or the
/// doubles' without it) rounds to it too.
fn shown_as_enclosed(
    l: &Loaded,
    k: &KeyGraphFeatures,
    cert: Option<&Analysis>,
    mp: Mp<'_>,
) -> bool {
    let Some(graphing::certify::Row::Certified { value: Some(e), .. }) =
        cert.map(|a| &a.y_intercept)
    else {
        return false;
    };
    let (lo, hi) = (e.lo.0, e.hi.0);
    if lo.is_nan()
        || hi.is_nan()
        || lo >= hi
        || (lo <= 0.0 && 0.0 <= hi)
        || !k.y_intercept.starts_with('≈')
    {
        return false;
    }
    let to = |v: f64, digits: usize| format!("{:.*e}", digits - 1, v);
    let Some(digits) = (3..=6).rev().find(|&d| to(lo, d) == to(hi, d)) else {
        return false;
    };
    let (flo, fhi) = match mp.and_then(|m| m(0.0).value) {
        Some(v) => v,
        None => {
            let v = l.f.at(0.0);
            if v.k != K::Def {
                return false;
            }
            (v.v, v.v)
        }
    };
    let shown = to(lo, digits);
    checks::panel_number(&k.y_intercept)
        .is_some_and(|(p, _)| to(p, 6) == to(shown.parse().unwrap_or(f64::NAN), 6))
        && to(flo, digits) == shown
        && to(fhi, digits) == shown
}

/// (d) against (a) and (b) sampled, by the oracle's checks; and against
/// (c) at 0.
fn panel(
    text: &str,
    l: &Loaded,
    k: &KeyGraphFeatures,
    cert: Option<&Analysis>,
    mp: Mp<'_>,
    out: &mut Outcome,
) {
    for f in checks::check_analysed(text, &l.f, k, &[], false, density(text)) {
        let class = format!("panel/{}", f.class);
        if f.note {
            out.note(&class, || f.detail.clone());
            continue;
        }
        // The oracle judges by f in doubles. Where MPFR has f's value at
        // the finding's point and the compiled one is off it by more than
        // its rounding (off, 2⁵³ + 1 times a thing less 2⁵³ times it is
        // that thing, and 0 in doubles), the finding is about the
        // doubles, not the panel: counted.
        let at = if f.class.starts_with("y-intercept") {
            0.0
        } else {
            f.at
        };
        if at.is_finite()
            && let Some((lo, hi)) = mp.and_then(|m| m(at).value)
        {
            let c = l.f.prog.eval(at, 0.0);
            if !faithful(c, lo, hi) {
                out.note(&format!("{class}-where-the-doubles-miss-f"), || {
                    format!(
                        "{} (f({at:e}) = [{lo:e}, {hi:e}], compiled {c:e})",
                        f.detail
                    )
                });
                continue;
            }
        }
        // f's enclosure over the doubles about x, to hold y: a zero so
        // sharp that f is far from 0 at the doubles beside it
        // (tan(x)^(2/1000001) at the double nearest π is 0.99993) is a
        // zero of the enclosure there all the same.
        let ctx = Ctx::new(l.opts());
        let enclosed = |x: f64, y: f64| {
            let around = Interval::new(checks::step(x, -16), checks::step(x, 16));
            let e = enclose(l.ast(), around, &ctx);
            !e.is_empty() && e.lo() <= y && y <= e.hi()
        };
        match f.class {
            "zero-not-zero" if f.at.is_finite() && enclosed(f.at, 0.0) => {
                out.note("panel/zero-not-zero-in-doubles", || f.detail.clone());
            }
            // f(x + P) against f(x), where x + P lands beside such a point.
            "period-witness"
                if f.at.is_finite()
                    && k.data.period.is_some_and(|p| {
                        let v = l.f.prog.eval(f.at, 0.0);
                        !v.is_nan() && enclosed(f.at + p, v)
                    }) =>
            {
                out.note("panel/period-witness-in-doubles", || f.detail.clone());
            }
            // The compiled NaN there may say "can't tell".
            "evaluator-disagrees"
                if l.f.prog.eval(f.at, 0.0).is_nan() && l.f.prog.undecided(f.at, 0.0) =>
            {
                out.note("pt/scalar-cannot-tell", || f.detail.clone());
            }
            // Or the two are apart by their own rounding, as at a point.
            "evaluator-disagrees"
                if matches!(
                    reval_typed(&l.f.ast, f.at, l.unit),
                    R::V(p) if within_both_roundings(
                        l,
                        &noisy(&l.f.ast),
                        f.at,
                        l.f.prog.eval(f.at, 0.0),
                        p.f(),
                    )
                ) =>
            {
                out.note("pt/scalar-vs-reference-within-their-rounding", || {
                    f.detail.clone()
                });
            }
            // "y = c is not approached": f is far from c at the farthest
            // samples. That refutes the limit only if f stays away, which
            // samples can't say and f's enclosure over the rest of the
            // tail can: min(acosh x, 2⁶³ − 1) is acosh x, 56, at 10²⁴, and
            // 2⁶³ − 1 from e^(2⁶³) on, past every double; its limit is
            // 2⁶³ − 1. Where every such line of that side lies in f's
            // enclosure from the sample outward, nothing is refuted.
            "asymptote-not-approached"
                if f.detail.starts_with("horizontal") && f.at.is_finite() && f.at != 0.0 && {
                    let tail = if f.at > 0.0 {
                        Interval::new(f.at, f64::INFINITY)
                    } else {
                        Interval::new(f64::NEG_INFINITY, f.at)
                    };
                    let e = enclose(l.ast(), tail, &ctx);
                    let way = if f.at > 0.0 {
                        graphing::analysis::AsymptoteSide::PositiveInfinity
                    } else {
                        graphing::analysis::AsymptoteSide::NegativeInfinity
                    };
                    let mut lines = k.data.horizontal_asymptotes.iter().filter(|h| {
                        h.1 == way || h.1 == graphing::analysis::AsymptoteSide::AnyInfinity
                    });
                    !e.is_empty() && lines.all(|h| e.lo() <= h.0 && h.0 <= e.hi())
                } =>
            {
                out.note("panel/asymptote-not-reached-within-the-doubles", || {
                    f.detail.clone()
                });
            }
            // "0 shown for a value below every double": the value is the
            // reference's, which keeps 53 bits. f(0) is not 0 only where
            // f's enclosure there says so (strictly signed, or 0 outside
            // it): e^(−10⁶) is; sin(π)^(2⁵³/6) is 0, and (1.2·10⁻¹⁶) to
            // that power to the reference, which takes the sine at π's
            // double.
            "y-intercept-underflow-shown-as-0"
                if {
                    let e = enclose(l.ast(), Interval::point(0.0), &ctx);
                    !(e.is_empty() || e.gt0() || e.lt0() || e.lo() > 0.0 || e.hi() < 0.0)
                } =>
            {
                out.note("panel/y-intercept-0-and-f-not-proven-nonzero", || {
                    f.detail.clone()
                });
            }
            // The panel's number for a value known to an enclosure is the
            // enclosure's middle, and its text as many digits as the
            // enclosure fixes: the text is the claim.
            "y-intercept-wrong" if shown_as_enclosed(l, k, cert, mp) => {
                out.note("panel/y-intercept-shown-to-the-digits-known", || {
                    format!("{}, shown as {}", f.detail, k.y_intercept)
                });
            }
            // "Undefined" at a sample, as the doubles have it: a
            // contradiction only where f is undefined in fact, or (with
            // MPFR to say) not proven defined.
            "domain-includes-undefined"
            | "y-intercept-undefined"
            | "extremum-undefined"
            | "inflection-undefined" => {
                let undefined = proven_undefined(l, mp, f.at);
                if undefined == Some(true) || (undefined.is_none() && mp.is_some()) {
                    out.fail(&class, f.detail);
                } else {
                    out.note(&format!("{class}-in-doubles"), || f.detail.clone());
                }
            }
            _ => out.fail(&class, f.detail),
        }
    }
    partial_items(l, k, mp, out);
    let Some(a) = cert else { return };
    let rows = [
        a.domain.value().is_some(),
        a.x_intercepts.value().is_some(),
        a.y_intercept.value().is_some(),
        a.parity.value().is_some(),
        a.period.value().is_some(),
        a.extrema.value().is_some(),
        a.inflections.value().is_some(),
        a.monotonicity.value().is_some(),
        a.range.value().is_some(),
        a.vertical.value().is_some(),
        a.horizontal.value().is_some(),
        a.oblique.value().is_some(),
    ];
    out.count(
        "rows certified or partial",
        rows.iter().filter(|r| **r).count(),
    );
    out.count("rows", rows.len());
    // The y-intercept against f's point enclosure at 0.
    let ctx = Ctx::new(l.opts());
    let e = enclose(l.ast(), Interval::point(0.0), &ctx);
    if let Row::Certified { value, .. } | Row::Partial { value, .. } = &a.y_intercept {
        match value {
            Some(y) if e.is_empty() => out.fail(
                "panel/y-intercept-where-enclosure-empty",
                format!(
                    "certified [{:e}, {:e}], f's enclosure at 0 is ∅",
                    y.lo.0, y.hi.0
                ),
            ),
            Some(y) if e.hi() < y.lo.0 || e.lo() > y.hi.0 => out.fail(
                "panel/y-intercept-outside-enclosure",
                format!(
                    "certified [{:e}, {:e}], f's enclosure at 0 is {}",
                    y.lo.0,
                    y.hi.0,
                    show_enc(&e)
                ),
            ),
            None if !e.is_empty() && e.dec >= Dec::Def => out.fail(
                "panel/no-y-intercept-where-enclosure-defined",
                format!("certified none, f's enclosure at 0 is {}", show_enc(&e)),
            ),
            _ => {}
        }
    }
}

// ------------------------------------------------------------------ digits

/// What an analysis says, without what names the text or the setting.
fn stripped(a: &Analysis) -> serde_json::Value {
    let mut v = serde_json::to_value(a).expect("a certificate serialises");
    if let Some(m) = v.as_object_mut() {
        m.remove("source");
        m.remove("binding");
        m.remove("evals");
    }
    v
}

/// The first place two JSON values differ.
fn first_difference(a: &serde_json::Value, b: &serde_json::Value, at: &str) -> Option<String> {
    use serde_json::Value::{Array, Object};
    match (a, b) {
        (Object(x), Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => {
                        if let Some(d) = first_difference(v, w, &format!("{at}.{k}")) {
                            return Some(d);
                        }
                    }
                    None => return Some(format!("{at}.{k}: only in the first")),
                }
            }
            y.keys()
                .find(|k| !x.contains_key(*k))
                .map(|k| format!("{at}.{k}: only in the second"))
        }
        (Array(x), Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{at}: {} items against {}", x.len(), y.len()));
            }
            x.iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (v, w))| first_difference(v, w, &format!("{at}[{i}]")))
        }
        _ if a == b => None,
        _ => {
            let short = |v: &serde_json::Value| {
                let s = v.to_string();
                if s.chars().count() > 160 {
                    format!("{}…", s.chars().take(160).collect::<String>())
                } else {
                    s
                }
            };
            Some(format!("{at}: {} against {}", short(a), short(b)))
        }
    }
}

/// One function typed under two settings is one function: under a digit
/// limit N, `text` is `text` with its numbers rounded to N digits typed
/// with the limit off; and with nothing to round, any limit is off. The
/// scalar's bits, the panel and the certificate are then the same.
fn digits(
    text: &str,
    set: &Setup,
    l: &Loaded,
    s: &Sampled,
    ka: &KeyGraphFeatures,
    cert: Option<&Analysis>,
    out: &mut Outcome,
) {
    // (A slider is rounded too, and off it is its double: only sliders
    // that read the same either way.)
    if !set.sliders.iter().all(|(_, v)| {
        slider_unrounded(*v, Some(5))
            || !l
                .eq
                .variables()
                .iter()
                .any(|n| set.sliders.iter().any(|(m, _)| m == n))
    }) {
        return;
    }
    let (other_text, other) = match set.digits {
        Some(d) => (
            rounded_text(text, d),
            Setup {
                digits: None,
                ..set.clone()
            },
        ),
        None => {
            let Some(d) = [20u8, 16, 15, 14, 5]
                .into_iter()
                .find(|d| unrounded(text, Some(*d)))
            else {
                return;
            };
            (
                text.to_string(),
                Setup {
                    digits: Some(d),
                    ..set.clone()
                },
            )
        }
    };
    let label = format!(
        "y={other_text} at {}",
        other
            .digits
            .map_or("off".to_string(), |d| format!("{d} digits"))
    );
    let lo = match load(&other_text, &other) {
        Ok(lo) => lo,
        Err(_) => {
            out.fail(
                "digits/one-compiles",
                format!("{label} doesn't parse or compile"),
            );
            return;
        }
    };
    out.count("settings compared", 1);
    for (&x, v) in s.xs.iter().zip(&s.vals) {
        let c = lo.f.prog.eval(x, 0.0);
        if c.to_bits() != v.c.to_bits() && !(c.is_nan() && v.c.is_nan()) {
            out.fail(
                "digits/scalar-differs",
                format!("x={x:e}: {:e}, but {label} gives {c:e}", v.c),
            );
            break;
        }
    }
    let (kb, cb) = analyze_certified(&lo.eq, &lo.opts(), None).expect("not cancelled");
    if *ka != kb {
        let (ta, tb) = (format!("{ka:?}"), format!("{kb:?}"));
        let at = ta
            .char_indices()
            .zip(tb.char_indices())
            .find(|(p, q)| p.1 != q.1)
            .map_or(0, |(p, _)| p.0);
        let from = ta[..at].char_indices().rev().nth(60).map_or(0, |(i, _)| i);
        let cut = |t: &str| t[from..].chars().take(200).collect::<String>();
        out.fail(
            "digits/panel-differs",
            format!("…{}… but {label} gives …{}…", cut(&ta), cut(&tb)),
        );
    }
    match (cert, &cb) {
        (Some(a), Some(b)) => {
            if let Some(d) = first_difference(&stripped(a), &stripped(b), "") {
                out.fail("digits/certificate-differs", format!("{d} ({label})"));
            }
        }
        (None, None) => {}
        _ => out.fail("digits/one-certifies", label),
    }
}

// -------------------------------------------------------------------- plot

/// (f): every drawn point inside the viewport lies in f's point enclosure
/// (the plotter's own form folds literal arithmetic and can be tighter by
/// rounding: a quarter pixel is allowed, the plotter's tolerance), and no
/// point is drawn where f is undefined; no hole is marked where f has a
/// value. Not where the plot says its budget ran out.
fn plotted(l: &Loaded, out: &mut Outcome) {
    let opts = l.opts();
    let Ok(ceq) = l.eq.compile(&opts) else {
        return;
    };
    let ctx = Ctx::new(opts);
    let ast = l.ast();
    let views = [
        Viewport::new(-10.0, 10.0, -10.0, 10.0, 800.0, 800.0),
        Viewport::new(-3.0, 5.0, -2.5, 2.5, 640.0, 400.0),
    ];
    let which = l.eq.text().bytes().map(usize::from).sum::<usize>() % views.len();
    for vp in [views[which]] {
        let p = plot(&ceq, &vp, &PlotOptions::default());
        if p.has_missing_data {
            out.count("plots past their budget", 1);
            continue;
        }
        out.count("plots", 1);
        let total: usize = p.curves.iter().map(Vec::len).sum();
        let every = (total / 240).max(1);
        let mut n = 0usize;
        for line in &p.curves {
            for pt in line {
                n += 1;
                if !n.is_multiple_of(every) || !(pt.y > vp.y_min && pt.y < vp.y_max) {
                    continue;
                }
                out.count("drawn points", 1);
                let e = enclose(ast, Interval::point(pt.x), &ctx);
                let v = l.f.at(pt.x);
                if e.is_empty() {
                    out.fail(
                        "plot/point-where-enclosure-empty",
                        format!("({:e}, {:e}) drawn, f's enclosure there is ∅", pt.x, pt.y),
                    );
                } else if distance(pt.y, e.lo(), e.hi()) > 0.25 * vp.y_per_px() {
                    out.fail(
                        "plot/point-outside-enclosure",
                        format!(
                            "({:e}, {:e}) drawn, f's enclosure there is {}",
                            pt.x,
                            pt.y,
                            show_enc(&e)
                        ),
                    );
                }
                if v.k == K::Undef {
                    out.fail(
                        "plot/point-where-undefined",
                        format!(
                            "({:e}, {:e}) drawn, f is undefined there (enclosure {})",
                            pt.x,
                            pt.y,
                            show_enc(&e)
                        ),
                    );
                }
            }
        }
        for h in &p.holes {
            let v = l.f.at(h.x);
            if v.k == K::Def {
                out.fail(
                    "plot/hole-where-defined",
                    format!("a hole at ({:e}, {:e}), but f there = {:e}", h.x, h.y, v.c),
                );
            }
        }
    }
}

/// Whether f is 0 at every sample it has a value at (and so even and odd
/// at once, as far as sampling tells).
fn nowhere_but_zero(f: &F) -> bool {
    sample_xs(f).into_iter().all(|x| {
        let v = f.at(x);
        v.k != K::Def || v.xf.is_zero()
    })
}

// --------------------------------------------------------------------- run

/// Every check of `groups` on `y = text` under `set` (and, for
/// [`group::PAIR`], against `pair`, a spelling equal to it everywhere).
pub fn run(text: &str, pair: Option<&str>, set: &Setup, groups: u32) -> Outcome {
    let mut out = Outcome::default();
    let l = match load(text, set) {
        Ok(l) => l,
        Err(NoFunction::Parse) => {
            out.fail("gen/does-not-parse", String::new());
            return out;
        }
        Err(NoFunction::NotCompiled) => {
            // What doesn't compile (a constant division by zero) has no
            // analysis either.
            out.count("functions that don't compile", 1);
            guarded(&mut out, "panel", |out| {
                let eq = Equation::parse_with(&format!("y={text}"), set.parse()).expect("parsed");
                let vars = set.vars();
                let k = analyze(
                    &eq,
                    &CompileOptions {
                        trig_unit: set.unit,
                        variables: &vars,
                    },
                );
                if k.analysis_error == AnalysisError::NoError {
                    out.fail(
                        "panel/analyses-what-does-not-compile",
                        format!("domain {}", k.domain),
                    );
                }
            });
            return out;
        }
    };
    out.count("functions", 1);

    let mut sampled = None;
    if groups & (group::POINTS | group::DIGITS) != 0 {
        guarded(&mut out, "points", |out| {
            let mut quiet = Outcome::default();
            let s = points(
                &l,
                if groups & group::POINTS != 0 {
                    out
                } else {
                    &mut quiet
                },
            );
            sampled = Some(s);
        });
    }

    // The panel and the certificate it was written from, made once.
    let mut analysed = None;
    let mut cert = None;
    let wants = groups & (group::PANEL | group::REPLAY | group::DIGITS) != 0
        || (cfg!(feature = "mpfr-oracle") && groups & group::POINTS != 0);
    if wants {
        guarded(&mut out, "analyze", |out| {
            let (k, a) = analyze_certified(&l.eq, &l.opts(), None).expect("not cancelled");
            analysed = Some(k);
            if a.is_none() {
                out.note("certify/no-certificate", || text.to_string());
            }
            cert = a;
        });
    }

    // MPFR's word on f at a point, where it has one.
    type Told<'a> = Option<Box<dyn Fn(f64) -> MpAt + 'a>>;
    #[cfg(not(feature = "mpfr-oracle"))]
    let mp_defined: Told<'_> = None;
    #[cfg(feature = "mpfr-oracle")]
    let mut fx = None;
    #[cfg(feature = "mpfr-oracle")]
    if let Some(a) = &cert {
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(a).expect("a certificate serialises"))
                .expect("a certificate reads back");
        guarded(&mut out, "replay", |out| fx = mp::function(&json, out));
        if let Some(fx) = &fx {
            if groups & group::POINTS != 0
                && let Some(s) = &sampled
            {
                guarded(&mut out, "mpfr-points", |out| mp::points(&l, fx, s, out));
            }
            if groups & group::REPLAY != 0 {
                guarded(&mut out, "replay", |out| mp::replayed(&json, fx, out));
            }
        }
    }
    #[cfg(feature = "mpfr-oracle")]
    let mp_defined: Told<'_> = fx
        .as_ref()
        .filter(|fx| mp::models(fx))
        .map(|fx| Box::new(move |x| mp::at(fx, x)) as Box<dyn Fn(f64) -> MpAt>);

    if let Some(k) = &analysed {
        if groups & group::PANEL != 0 {
            guarded(&mut out, "panel", |out| {
                panel(text, &l, k, cert.as_ref(), mp_defined.as_deref(), out)
            });
        }
        if groups & group::DIGITS != 0
            && let Some(s) = &sampled
        {
            guarded(&mut out, "digits", |out| {
                digits(text, set, &l, s, k, cert.as_ref(), out)
            });
        }
    }
    if groups & group::PAIR != 0
        && let Some(other) = pair
    {
        guarded(&mut out, "pair", |out| {
            let vars = set.vars();
            let setting = Setting {
                unit: set.unit,
                parse: set.parse(),
                vars: &vars,
                density: density(&format!("{text} {other}")),
            };
            out.count("pairs", 1);
            for f in checks::check_pair_in(text, other, false, &setting) {
                // "Even" (or "odd") of the zero function is true, though
                // not the whole answer ("both"): one spelling proving
                // more than the other is no contradiction.
                // (And "even" against "odd" both hold of a function that
                // is 0 wherever it is defined: x·√(x − x).)
                let parity =
                    f.class == "equivalent-answers-differ" && f.detail.starts_with("parity:");
                let says = |p: &str| f.detail.contains(&format!("says {p}"));
                let weaker = parity
                    && ((says("Both") && (says("Even") || says("Odd")))
                        || (says("Even") && says("Odd") && nowhere_but_zero(&l.f)));
                let cannot_tell = f.class == "spelling-value-differs"
                    && f.at.is_finite()
                    && l.f.prog.eval(f.at, 0.0).is_nan()
                    && l.f.prog.undecided(f.at, 0.0);
                if f.note {
                    out.note(&format!("pair/{}", f.class), || f.detail.clone());
                } else if cannot_tell {
                    out.note("pt/scalar-cannot-tell", || f.detail.clone());
                } else if weaker {
                    out.note("pair/parity-both-against-one", || f.detail.clone());
                } else {
                    out.fail(&format!("pair/{}", f.class), f.detail);
                }
            }
        });
    }
    if groups & group::PLOT != 0 {
        guarded(&mut out, "plot", |out| plotted(&l, out));
    }
    out
}
