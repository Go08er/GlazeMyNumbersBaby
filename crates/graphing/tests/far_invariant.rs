//! `Wide::far`'s invariant (`src/wide.rs`) as a property, tested against
//! MPFR (`--features mpfr-oracle`).
//!
//! A value from 2⁵³ binades out keeps its sign, its side of the doubles
//! and, while its exponent is finite, that exponent within 2⁻⁴⁰ of the
//! true one, relatively; one whose exponent is infinite is known only to
//! be past 2⁴⁷ binades, less 2³⁰ for each product or quotient that may
//! have pulled it toward 1. Every order, sign and value the evaluator
//! decides from such values rests on that. Review 21's audit read each
//! producer and found the powers sound; review 22 found one whose exponent
//! was 0.09% off ((1 − 2⁻⁴⁴)^(10³⁰⁰): t·e + t·log₂|m| cancelling). So
//! this checks the rule rather than reading it.
//!
//! Each case is a short program in that regime: powers of bases a hair
//! either side of 1 (exactly 1 ± 2⁻ᵏ, 1 ± j·2⁻⁵³ and m·2ᵉ, results of
//! operations on them, and x at run time) to huge exponents written every
//! way the compiler tells apart (powers of 10 and 2, decimals, odd whole
//! numbers past 2⁵³, ratios past 10⁶); e^c for c far out and about the
//! 2⁵³-binade edge; n! and nCr past the doubles; and chains of these
//! through small powers aimed at that edge, roots, products, quotients,
//! sums of one sign, differences, min, max, |·| and sign, and comparisons
//! with a value near their own. Every subtree is compiled and evaluated on
//! its own through the compiled evaluator's extended range
//! (`Program::eval_extended`), and checked against MPFR:
//!
//! * a far value: its sign, its side, and its exponent within the bound,
//!   2⁻⁴⁰ and the d·3·2⁻⁵³/(1 − 2⁻⁷)^d that `Wide::far` derives it from
//!   for d operations (finite), or past 2⁴⁷ binades less 2³⁰ a product
//!   or quotient (infinite);
//! * any other value: within 2⁻⁴⁰ of the truth, relatively (a sum a far
//!   value leaves alone, a min, a sign, a power's mantissa from its
//!   logarithm), and an exact 0 only for 0;
//! * a value at all only where the truth is defined.
//!
//! The truth of a node is the exact operation on what its operands stand
//! for: one within reach exactly itself (the evaluator rounds step by
//! step, as doubles do, and a literal is the double it reads, its
//! convention), a far one its own truth, so an error is charged to the
//! node that makes it. MPFR holds each as its sign and log₂|v| at 1024
//! bits, so e^(10³⁰⁰) is an ordinary number to it.
//!
//! ```text
//! cargo test -p graphing --features mpfr-oracle --test far_invariant
//! cargo test --release -p graphing --features mpfr-oracle --test far_invariant -- \
//!     --ignored long_run --nocapture
//! ```
//!
//! The long run takes `FAR_SEED`, `FAR_CASES` and `FAR_THREADS`, and
//! prints the largest relative exponent error seen on each path, and the
//! most one operation added.
#![cfg(feature = "mpfr-oracle")]

use graphing::TrigUnit;
use graphing::compile::compile_str;
use graphing::error::{EquationError, ErrorCode, EvaluationErrorCode};
use rug::Float;
use rug::float::Constant;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// MPFR's precision for log₂|v|: 2⁻¹⁰²⁴ of itself, 2⁻⁹⁰⁰ of a binade
/// to 2¹²⁴ binades out.
const P: u32 = 1024;
/// From here on a value is far (see `wide::TWO53`).
const TWO53: f64 = 9_007_199_254_740_992.0;
/// `Wide::far`'s bound on a finite far exponent, relatively; and the most
/// any other value may be off, relatively.
const BOUND: f64 = 1.0 / 1_099_511_627_776.0;
/// An infinite exponent's value is past this many binades...
const INFINITE_PAST: f64 = 140_737_488_355_328.0;
/// ...less this for each product or quotient in it.
const STEP: f64 = 1_073_741_824.0;

/// What one operation may add to a far exponent's error, relatively (two
/// roundings and a binade, 3·2⁻⁵³), and how much a pull toward 1 may
/// multiply it by: d operations deep, it is within d·3·2⁻⁵³/(1 − 2⁻⁷)^d
/// (`Wide::far`).
const STEP_ERROR: f64 = 3.0 / TWO53;
const PULL: f64 = 1.0 / (1.0 - 1.0 / 128.0);

/// The short run: seconds, in CI's MPFR step.
const SHORT_SEED: u64 = 22;
const SHORT_CASES: u64 = 3000;
/// The long run's default.
const LONG_CASES: u64 = 100_000;

/// MPFR's widest exponent range (per thread): 2^(log₂|v|) for a value
/// within reach is then a number, not 0 or ∞.
fn init() {
    use gmp_mpfr_sys::mpfr;
    unsafe {
        mpfr::set_emin(mpfr::get_emin_min());
        mpfr::set_emax(mpfr::get_emax_max());
    }
}

fn fl(v: f64) -> Float {
    Float::with_val(P, v)
}

fn ln2() -> Float {
    Float::with_val(P, Constant::Log2)
}

/// 2^k for k from −1074 to 1023, exactly.
fn pow2(k: i64) -> f64 {
    assert!((-1074..=1023).contains(&k), "2^{k}");
    if k >= -1022 {
        f64::from_bits(((k + 1023) as u64) << 52)
    } else {
        f64::from_bits(1 << (k + 1074))
    }
}

/// A real number, as MPFR holds it here.
#[derive(Clone, Debug, PartialEq)]
enum Real {
    Zero,
    /// Its sign, and log₂ of its size (±∞ past even MPFR's range).
    Val {
        neg: bool,
        l: Float,
    },
}

impl Real {
    /// m·2^e, exactly (but for log₂'s rounding at 2⁻¹⁰²⁴).
    fn of(m: f64, e: f64) -> Real {
        if m == 0.0 {
            Real::Zero
        } else {
            Real::Val {
                neg: m < 0.0,
                l: fl(e) + fl(m.abs()).log2(),
            }
        }
    }

    fn double(v: f64) -> Real {
        Real::of(v, 0.0)
    }

    fn neg(&self) -> Real {
        match self {
            Real::Zero => Real::Zero,
            Real::Val { neg, l } => Real::Val {
                neg: !neg,
                l: l.clone(),
            },
        }
    }

    fn abs(&self) -> Real {
        match self {
            Real::Zero => Real::Zero,
            Real::Val { l, .. } => Real::Val {
                neg: false,
                l: l.clone(),
            },
        }
    }

    /// Its value (0 or ±∞ only past MPFR's range).
    fn value(&self) -> Float {
        match self {
            Real::Zero => fl(0.0),
            Real::Val { neg, l } => {
                let v = l.clone().exp2();
                if *neg { -v } else { v }
            }
        }
    }

    fn log2(&self) -> Option<&Float> {
        match self {
            Real::Zero => None,
            Real::Val { l, .. } => Some(l),
        }
    }
}

/// What a node is.
#[derive(Clone, Debug)]
enum Truth {
    Is(Real),
    Undefined,
    /// Not decided here (past MPFR's range where that matters, or an
    /// operand that isn't a value).
    Unknown,
}

fn add(a: &Real, b: &Real) -> Truth {
    let (Real::Val { neg: na, l: la }, Real::Val { neg: nb, l: lb }) = (a, b) else {
        return Truth::Is(if *a == Real::Zero {
            b.clone()
        } else {
            a.clone()
        });
    };
    if la.is_infinite() && la == lb {
        return if na == nb {
            Truth::Is(a.clone())
        } else {
            Truth::Unknown
        };
    }
    let (big, small, neg) = if la >= lb {
        (la, lb, *na)
    } else {
        (lb, la, *nb)
    };
    let d = Float::with_val(P, small - big).exp2();
    let fix = if na == nb {
        d.ln_1p()
    } else if d == 1 {
        return Truth::Is(Real::Zero);
    } else {
        (-d).ln_1p()
    };
    let fix = fix / ln2();
    Truth::Is(Real::Val {
        neg,
        l: Float::with_val(P, big + &fix),
    })
}

fn mul(a: &Real, b: &Real) -> Truth {
    match (a, b) {
        (Real::Val { neg: na, l: la }, Real::Val { neg: nb, l: lb }) => {
            let l = Float::with_val(P, la + lb);
            if l.is_nan() {
                Truth::Unknown
            } else {
                Truth::Is(Real::Val { neg: na != nb, l })
            }
        }
        _ => Truth::Is(Real::Zero),
    }
}

fn div(a: &Real, b: &Real) -> Truth {
    match (a, b) {
        (_, Real::Zero) => Truth::Undefined,
        (Real::Zero, _) => Truth::Is(Real::Zero),
        (Real::Val { neg: na, l: la }, Real::Val { neg: nb, l: lb }) => {
            let l = Float::with_val(P, la - lb);
            if l.is_nan() {
                Truth::Unknown
            } else {
                Truth::Is(Real::Val { neg: na != nb, l })
            }
        }
    }
}

/// |a|^t.
fn pow_mag(a: &Real, t: &Float) -> Truth {
    match a {
        Real::Zero if *t > 0 => Truth::Is(Real::Zero),
        Real::Zero => Truth::Undefined,
        Real::Val { l, .. } if l.is_zero() => Truth::Is(Real::Val {
            neg: false,
            l: fl(0.0),
        }),
        Real::Val { l, .. } => {
            let l = Float::with_val(P, l * t);
            if l.is_nan() {
                Truth::Unknown
            } else {
                Truth::Is(Real::Val { neg: false, l })
            }
        }
    }
}

fn exp(a: &Real) -> Truth {
    Truth::Is(Real::Val {
        neg: false,
        l: a.value() / ln2(),
    })
}

/// a against b; `None` for two past MPFR's range alike.
fn order(a: &Real, b: &Real) -> Option<Ordering> {
    let rank = |r: &Real| match r {
        Real::Zero => 0,
        Real::Val { neg: true, .. } => -1,
        Real::Val { .. } => 1,
    };
    match rank(a).cmp(&rank(b)) {
        Ordering::Equal => match (a, b) {
            (Real::Val { neg, l: la }, Real::Val { l: lb, .. }) => {
                if la.is_infinite() && la == lb {
                    return None;
                }
                let o = la.partial_cmp(lb)?;
                Some(if *neg { o.reverse() } else { o })
            }
            _ => Some(Ordering::Equal),
        },
        o => Some(o),
    }
}

/// ln Γ(x) for x > 0, in binades.
fn log2_gamma(x: &Float) -> Float {
    let (l, _) = x.clone().ln_abs_gamma();
    l / ln2()
}

/// n! for n ≥ 0 (Γ(n + 1)).
fn factorial(n: &Real) -> Truth {
    let v = n.value();
    if v < 0 || v.is_infinite() {
        return Truth::Unknown;
    }
    Truth::Is(Real::Val {
        neg: false,
        l: log2_gamma(&Float::with_val(P, &v + 1)),
    })
}

/// nCr or nPr for n ≥ r ≥ 0.
fn count(n: &Real, r: &Real, perm: bool) -> Truth {
    let (n, r) = (n.value(), r.value());
    if r < 0 || n < r || n.is_infinite() {
        return Truth::Unknown;
    }
    let one = |v: &Float| Float::with_val(P, v + 1);
    let mut l = log2_gamma(&one(&n)) - log2_gamma(&one(&Float::with_val(P, &n - &r)));
    if !perm {
        l -= log2_gamma(&one(&r));
    }
    Truth::Is(Real::Val { neg: false, l })
}

/// root(a, n): a^(1/n), keeping a's sign for an odd whole n. 1/n is
/// exact for a whole n, and its double for any other (the typed lowering
/// rounds it once).
fn root(a: &Real, n: f64) -> Truth {
    if n == 0.0 {
        return Truth::Undefined;
    }
    let whole = n.fract() == 0.0;
    let odd = whole && n % 2.0 != 0.0;
    let inv = if whole {
        Float::with_val(P, 1) / fl(n)
    } else {
        fl(1.0 / n)
    };
    match a {
        Real::Zero if n > 0.0 => Truth::Is(Real::Zero),
        Real::Zero => Truth::Undefined,
        Real::Val { neg: true, .. } if !odd => Truth::Undefined,
        Real::Val { neg, .. } => match pow_mag(a, &inv) {
            Truth::Is(r) if *neg => Truth::Is(r.neg()),
            t => t,
        },
    }
}

/// The largest exponent error seen on one path.
#[derive(Clone, Copy, Default, Debug)]
struct Stat {
    /// Far values with a finite exponent checked.
    far: u64,
    /// The largest |e − T|/|T|.
    worst: f64,
    /// The most of that one operation added to its operands' (far,
    /// finite) worst.
    added: f64,
    /// Far values with an infinite exponent, and other values, checked.
    infinite: u64,
    other: u64,
    /// The largest |Δ log₂| of another value (about its relative error).
    other_worst: f64,
}

/// A node: its text, what the evaluator holds and what it truly is.
struct Built {
    text: String,
    held: Option<(f64, f64)>,
    truth: Truth,
    /// Products and quotients in it.
    pulls: u32,
    /// The most operations on a path to it.
    ops: u32,
    /// Its exponent's relative error, where it is far and finite.
    err: f64,
}

impl Built {
    fn far(&self) -> bool {
        matches!(self.held, Some((m, e)) if m != 0.0 && (e.is_infinite() || e.abs() >= TWO53))
    }

    /// What it stands for as an operand: itself within reach, its truth
    /// when far.
    fn input(&self) -> Option<Real> {
        let (m, e) = self.held?;
        if m == 0.0 || (e.is_finite() && e.abs() < TWO53) {
            Some(Real::of(m, e))
        } else if let Truth::Is(r) = &self.truth {
            Some(r.clone())
        } else {
            None
        }
    }

    fn positive(&self) -> bool {
        matches!(self.held, Some((m, _)) if m > 0.0)
    }
}

/// An exponent as the compiler reads it.
enum Expo {
    /// A subtree: the exponent is what it stands for.
    Node(Built),
    /// p/q in lowest terms, q > 1, written as two integers: exactly p/q.
    Ratio(i128, i128),
    /// A whole number below 10⁶ in size, written as one.
    Int(i64),
}

impl Expo {
    fn text(&self) -> String {
        match self {
            Expo::Node(b) => b.text.clone(),
            Expo::Ratio(p, q) if *p < 0 => format!("(-{}/{q})", -p),
            Expo::Ratio(p, q) => format!("({p}/{q})"),
            Expo::Int(n) if *n < 0 => format!("(-{})", -n),
            Expo::Int(n) => format!("{n}"),
        }
    }

    fn value(&self) -> Option<Float> {
        match self {
            Expo::Node(b) => Some(b.input()?.value()),
            Expo::Ratio(p, q) => Some(Float::with_val(P, *p) / Float::with_val(P, *q)),
            Expo::Int(n) => Some(Float::with_val(P, *n)),
        }
    }

    /// What it makes of a negative base: `Some(Some(odd))`, a power of
    /// its size with the sign of an odd or even one; `Some(None)`,
    /// undefined (a ratio of even q); `None`, not modelled here.
    fn negative_base(&self) -> Option<Option<bool>> {
        match self {
            Expo::Int(n) => Some(Some(n % 2 != 0)),
            Expo::Ratio(_, q) if q % 2 == 0 => Some(None),
            Expo::Ratio(p, _) => Some(Some(p % 2 != 0)),
            Expo::Node(_) => None,
        }
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // SplitMix64.
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / TWO53
    }

    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    fn pick<'a, T>(&mut self, of: &'a [T]) -> &'a T {
        &of[self.below(of.len() as u64) as usize]
    }
}

/// A double as typed text the parser reads as exactly that double
/// (the shortest decimal that rounds back to it).
fn decimal(v: f64) -> String {
    let s = format!("{v:e}");
    let (m, e) = s.split_once('e').expect("an exponent");
    if e == "0" {
        format!("({m})")
    } else {
        format!("({m}*10^({e}))")
    }
}

/// One case: a program built node by node, each checked as it is made.
struct Case {
    rng: Rng,
    /// Whether a base is to be x, and the value it then stands for.
    want_x: bool,
    x: Option<f64>,
    failure: Option<String>,
    stats: BTreeMap<&'static str, Stat>,
    nodes: u64,
}

impl Case {
    fn new(seed: u64, index: u64) -> Case {
        let mut rng = Rng(seed ^ index.wrapping_mul(0xa076_1d64_78bd_642f));
        let want_x = rng.chance(0.35);
        Case {
            rng,
            want_x,
            x: None,
            failure: None,
            stats: BTreeMap::new(),
            nodes: 0,
        }
    }

    /// Compiles `text` as typed and evaluates it in extended range.
    fn held(&self, text: &str) -> Option<(f64, f64)> {
        match compile_str(text, TrigUnit::Radians) {
            Ok(p) => p.eval_extended(self.x.unwrap_or(0.0), 0.0),
            // A constant exactly 0 to a negative power: undefined, said
            // when typed.
            Err(EquationError {
                code: ErrorCode::Evaluation(EvaluationErrorCode::DivideByZero),
                ..
            }) => None,
            Err(e) => panic!("{text} doesn't compile: {e:?}"),
        }
    }

    /// Evaluates a node and checks it against its truth.
    fn finish(
        &mut self,
        path: &'static str,
        text: String,
        truth: Truth,
        pulls: u32,
        inputs: &[&Built],
    ) -> Built {
        self.nodes += 1;
        let held = self.held(&text);
        let ops = inputs.iter().map(|a| a.ops + 1).max().unwrap_or(0);
        let mut b = Built {
            text,
            held,
            truth,
            pulls,
            ops,
            err: 0.0,
        };
        if self.failure.is_none()
            && let Err(why) = self.check(path, &mut b, inputs)
        {
            let mut msg = format!(
                "{why}\n  node ({path}): {}\n  x = {:?}, held {:?}, truth {}",
                b.text,
                self.x,
                b.held,
                show(&b.truth)
            );
            for (i, a) in inputs.iter().enumerate() {
                msg += &format!(
                    "\n  operand {i}: {}\n    held {:?}, truth {}",
                    a.text,
                    a.held,
                    show(&a.truth)
                );
            }
            self.failure = Some(msg);
        }
        b
    }

    fn check(
        &mut self,
        path: &'static str,
        b: &mut Built,
        inputs: &[&Built],
    ) -> Result<(), String> {
        let Some((m, e)) = b.held else {
            // Undefined or can't tell: nothing claimed.
            return Ok(());
        };
        let t = match &b.truth {
            Truth::Unknown => return Ok(()),
            Truth::Undefined => return Err("a value where the truth is undefined".into()),
            Truth::Is(t) => t,
        };
        let stat = self.stats.entry(path).or_default();
        let Real::Val { neg, l } = t else {
            return if m == 0.0 {
                Ok(())
            } else {
                Err("a nonzero value for an exact 0".into())
            };
        };
        if m == 0.0 {
            return Err("an exact 0 for a nonzero value".into());
        }
        if *neg != (m < 0.0) {
            return Err("the wrong sign".into());
        }
        let far = e.is_infinite() || e.abs() >= TWO53;
        if !far {
            stat.other += 1;
            let mine = fl(e) + fl(m.abs()).log2();
            let off = Float::with_val(P, &mine - l).abs();
            if l.is_finite() {
                stat.other_worst = stat.other_worst.max(off.to_f64());
            }
            // (|Δ log₂| ≤ 2⁻⁴⁰ is a relative error below 2⁻⁴⁰·ln 2.)
            return if l.is_finite() && off <= BOUND {
                Ok(())
            } else {
                Err(format!(
                    "a value within reach off by 2^{:.1} in log2 (truth log2 {})",
                    off.to_f64().log2(),
                    l.to_f64()
                ))
            };
        }
        if (e > 0.0) != (*l > 0) {
            return Err("the wrong side of the doubles".into());
        }
        if e.is_infinite() {
            stat.infinite += 1;
            let past = INFINITE_PAST - f64::from(b.pulls) * STEP;
            return if l.clone().abs() > past {
                Ok(())
            } else {
                Err(format!(
                    "an infinite exponent for a value {} binades out",
                    l.to_f64()
                ))
            };
        }
        if l.is_infinite() {
            return Err("a finite far exponent for a value past even MPFR's range".into());
        }
        let off = Float::with_val(P, l.clone().floor() - e).abs();
        let rel = (off / l.clone().abs()).to_f64();
        let before = inputs
            .iter()
            .filter(|a| a.far())
            .map(|a| a.err)
            .fold(0.0, f64::max);
        b.err = rel;
        stat.far += 1;
        stat.worst = stat.worst.max(rel);
        stat.added = stat.added.max(rel - before);
        let d = f64::from(b.ops);
        let bound = BOUND.min(d * STEP_ERROR * PULL.powf(d));
        if rel <= bound {
            Ok(())
        } else {
            Err(format!(
                "a far exponent off by 2^{:.1} of itself, {} operations deep (bound 2^{:.1})",
                rel.log2(),
                b.ops,
                bound.log2()
            ))
        }
    }

    // ----- Leaves and operations.

    /// The double `v`, written as `text` (as cos(0)·…, a base escapes the
    /// exact folding of literals: cos(0) is 1 to the bit, but no
    /// rational), or as x where it `may_be_x` and the case puts a base at
    /// run time.
    fn leaf(&mut self, text: String, v: f64, may_be_x: bool) -> Built {
        let text = if may_be_x && self.want_x && self.x.is_none() {
            self.x = Some(v);
            "x".to_string()
        } else {
            text
        };
        self.finish("leaf", text, Truth::Is(Real::double(v)), 0, &[])
    }

    /// m·2ᵉ written so, exactly (beyond the doubles too).
    fn dyadic(&mut self, m: i64, e: i64, may_be_x: bool) -> Built {
        let text = format!("(cos(0)*{m}*2^({e}))");
        if (-1074..=1023).contains(&e) {
            let v = m as f64 * pow2(e);
            if v.is_normal() {
                return self.leaf(text, v, may_be_x);
            }
        }
        let truth = Truth::Is(Real::of(m as f64, e as f64));
        self.finish("leaf", text, truth, 0, &[])
    }

    fn op1(
        &mut self,
        path: &'static str,
        text: String,
        a: &Built,
        f: impl FnOnce(&Real) -> Truth,
    ) -> Built {
        let truth = a.input().map_or(Truth::Unknown, |r| f(&r));
        let pulls = a.pulls;
        self.finish(path, text, truth, pulls, &[a])
    }

    fn op2(
        &mut self,
        path: &'static str,
        text: String,
        a: &Built,
        b: &Built,
        pull: bool,
        f: impl FnOnce(&Real, &Real) -> Truth,
    ) -> Built {
        let truth = match (a.input(), b.input()) {
            (Some(x), Some(y)) => f(&x, &y),
            _ => Truth::Unknown,
        };
        let pulls = a.pulls + b.pulls + u32::from(pull);
        self.finish(path, text, truth, pulls, &[a, b])
    }

    fn add(&mut self, a: &Built, b: &Built) -> Built {
        let text = format!("({})+({})", a.text, b.text);
        self.op2("add", text, a, b, false, add)
    }

    fn sub(&mut self, a: &Built, b: &Built) -> Built {
        let text = format!("({})-({})", a.text, b.text);
        self.op2("add", text, a, b, false, |x, y| add(x, &y.neg()))
    }

    fn mul(&mut self, a: &Built, b: &Built) -> Built {
        let text = format!("({})*({})", a.text, b.text);
        self.op2("mul/div", text, a, b, true, mul)
    }

    fn div(&mut self, a: &Built, b: &Built) -> Built {
        let text = format!("({})/({})", a.text, b.text);
        self.op2("mul/div", text, a, b, true, div)
    }

    fn minmax(&mut self, max: bool, a: &Built, b: &Built) -> Built {
        let name = if max { "max" } else { "min" };
        let text = format!("{name}({},{})", a.text, b.text);
        self.op2("min/max", text, a, b, false, |x, y| match order(x, y) {
            Some(o) => Truth::Is(if (o == Ordering::Greater) == max {
                x.clone()
            } else {
                y.clone()
            }),
            None => Truth::Unknown,
        })
    }

    fn pow(&mut self, a: &Built, t: Expo) -> Built {
        let text = format!("({})^({})", a.text, t.text());
        let path = if a.far() {
            "pow, far base"
        } else if matches!(a.input(), Some(Real::Val { ref l, .. }) if l.clone().abs() < 1.0 / 1024.0)
        {
            "pow, base near 1"
        } else {
            "pow"
        };
        let truth = match (a.input(), t.value()) {
            (Some(r), Some(tv)) => {
                let mag = pow_mag(&r.abs(), &tv);
                match (&r, t.negative_base()) {
                    (Real::Val { neg: true, .. }, Some(Some(true))) => match mag {
                        Truth::Is(v) => Truth::Is(v.neg()),
                        m => m,
                    },
                    (Real::Val { neg: true, .. }, Some(Some(false))) => mag,
                    (Real::Val { neg: true, .. }, Some(None)) => Truth::Undefined,
                    (Real::Val { neg: true, .. }, None) => Truth::Unknown,
                    _ => mag,
                }
            }
            _ => Truth::Unknown,
        };
        let inputs: Vec<&Built> = match &t {
            Expo::Node(b) => vec![a, b],
            _ => vec![a],
        };
        let pulls = inputs.iter().map(|b| b.pulls).sum();
        self.finish(path, text, truth, pulls, &inputs)
    }

    fn root(&mut self, a: &Built, n: f64) -> Built {
        let text = format!("root({},{})", a.text, decimal(n));
        self.op1("root", text, a, |r| root(r, n))
    }

    // ----- What programs are made of.

    /// k for 2⁻ᵏ: from 1 to 53, often 30 or more.
    fn k(&mut self) -> i64 {
        if self.rng.chance(0.5) {
            self.rng.range(1, 53)
        } else {
            self.rng.range(30, 53)
        }
    }

    /// A base a hair from 1.
    fn near_one(&mut self) -> Built {
        let k = self.k();
        let below = self.rng.chance(0.6);
        let sign = if below { '-' } else { '+' };
        let two_k = pow2(-k);
        match self.rng.below(10) {
            0..=3 => {
                // Exactly 1 ± 2⁻ᵏ (1 + 2⁻⁵³ is no double).
                let k = if below { k } else { k.min(52) };
                let v = if below {
                    1.0 - pow2(-k)
                } else {
                    1.0 + pow2(-k)
                };
                self.leaf(format!("(cos(0){sign}2^(-{k}))"), v, true)
            }
            4 => {
                // 1 − j·2⁻⁵³ or 1 + j·2⁻⁵², exactly.
                let j = self.rng.range(1, 4096);
                if below {
                    let v = 1.0 - j as f64 * pow2(-53);
                    self.leaf(format!("(cos(0)-{j}*2^(-53))"), v, true)
                } else {
                    let v = 1.0 + j as f64 * pow2(-52);
                    self.leaf(format!("(cos(0)+{j}*2^(-52))"), v, true)
                }
            }
            5 => {
                // A double in [½, 2), m·2ᵉ.
                let m = self.rng.range(1 << 52, (1 << 53) - 1);
                let e = if below { -53 } else { -52 };
                self.dyadic(m, e, true)
            }
            6 => {
                // e^(±2⁻ᵏ), rounded.
                let c = if below {
                    self.leaf(format!("(-2^(-{k}))"), -two_k, false)
                } else {
                    self.leaf(format!("(2^(-{k}))"), two_k, false)
                };
                let text = format!("exp({})", c.text);
                self.op1("exp", text, &c, exp)
            }
            7 => {
                // 1/(1 ± 2⁻ᵏ), rounded.
                let one = self.leaf("cos(0)".into(), 1.0, false);
                let k = k.min(52);
                let v = if below {
                    1.0 + pow2(-k)
                } else {
                    1.0 - pow2(-k)
                };
                let s = if below { '+' } else { '-' };
                let d = self.leaf(format!("(cos(0){s}2^(-{k}))"), v, true);
                self.div(&one, &d)
            }
            8 => {
                // √(1 ± 2⁻ᵏ) and ∛ of it, rounded.
                let k = if below { k } else { k.min(52) };
                let v = if below {
                    1.0 - pow2(-k)
                } else {
                    1.0 + pow2(-k)
                };
                let a = self.leaf(format!("(cos(0){sign}2^(-{k}))"), v, true);
                if self.rng.chance(0.5) {
                    let text = format!("sqrt({})", a.text);
                    self.op1("root", text, &a, |r| pow_mag(r, &fl(0.5)))
                } else {
                    let text = format!("cbrt({})", a.text);
                    self.op1("root", text, &a, |r| root(r, 3.0))
                }
            }
            _ => {
                // (1 − 2⁻ᵏ)(1 + 2⁻ʲ), rounded.
                let j = self.k().min(52);
                let a = self.leaf(format!("(cos(0)-2^(-{k}))"), 1.0 - pow2(-k), true);
                let b = self.leaf(format!("(cos(0)+2^(-{j}))"), 1.0 + pow2(-j), false);
                self.mul(&a, &b)
            }
        }
    }

    /// An exponent from 10¹⁰ to 10³⁰⁸ in size, written as the compiler
    /// tells apart.
    fn big_exponent(&mut self) -> Expo {
        let neg = self.rng.chance(0.35);
        let s = if neg { "-" } else { "" };
        match self.rng.below(7) {
            0 | 1 => {
                let k = self.rng.range(10, 308);
                let v: f64 = format!("{s}1e{k}").parse().unwrap();
                Expo::Node(self.leaf(format!("({s}10^({k}))"), v, false))
            }
            2 => {
                let k = self.rng.range(34, 1023);
                let v = if neg { -1.0 } else { 1.0 } * pow2(k);
                Expo::Node(self.leaf(format!("({s}2^({k}))"), v, false))
            }
            3 => {
                // A decimal of 14 digits, its double.
                let d = self.rng.range(10_000_000_000_000, 99_999_999_999_999);
                let k = self.rng.range(-3, 290);
                let v: f64 = format!("{s}{d}e{k}").parse().unwrap();
                Expo::Node(self.leaf(format!("({s}{d}*10^({k}))"), v, false))
            }
            4 => {
                // A ratio past 10⁶: p/q exactly (`wide::pow_ratio`).
                let q = *self.rng.pick(&[3i128, 7, 9, 11, 13, 1001, 999_999_937]);
                let mut p = (self.rng.next() as i128) << self.rng.range(0, 60);
                p = p.max(1_000_000);
                if p % q == 0 {
                    p += 1;
                }
                let g = gcd(p, q);
                let (p, q) = (p / g, q / g);
                Expo::Ratio(if neg { -p } else { p }, q)
            }
            5 => {
                // An odd whole number its double isn't (`wide::pow_odd`).
                let n = (1u64 << 53) + 1 + 2 * self.rng.below(1 << 40);
                let v = if neg { -(n as f64) } else { n as f64 };
                Expo::Node(self.leaf(format!("({s}{n})"), v, false))
            }
            _ => {
                // Through the general path.
                let k = self.rng.range(10, 308);
                let v: f64 = format!("{s}1e{k}").parse().unwrap();
                Expo::Node(self.leaf(format!("(cos(0)*{s}10^({k}))"), v, false))
            }
        }
    }

    /// e^c, c far out or about the 2⁵³-binade edge.
    fn exp_far(&mut self) -> Built {
        let c = match self.rng.below(3) {
            0 => {
                let k = self.rng.range(9, 1022);
                (1.0 + self.rng.unit()) * pow2(k)
            }
            1 => self.edge() * std::f64::consts::LN_2,
            _ => 10f64.powi(self.rng.range(3, 307) as i32) * (1.0 + 9.0 * self.rng.unit()),
        };
        let c = if self.rng.chance(0.4) { -c } else { c };
        let a = self.leaf(decimal(c), c, false);
        let text = format!("exp({})", a.text);
        self.op1("exp", text, &a, exp)
    }

    /// A relative step: 0, or ±2⁻ʲ.
    fn near_edge(&mut self) -> f64 {
        if self.rng.chance(0.1) {
            0.0
        } else {
            let d = pow2(-self.rng.range(1, 53));
            if self.rng.chance(0.5) { d } else { -d }
        }
    }

    /// A size in binades about the 2⁵³ edge: 2⁵³·(1 ± 2⁻ʲ), or within a
    /// few halves of 2⁵³.
    fn edge(&mut self) -> f64 {
        if self.rng.chance(0.3) {
            TWO53 + 0.5 * self.rng.range(-6, 6) as f64
        } else {
            TWO53 * (1.0 + self.near_edge())
        }
    }

    /// n! or nCr, nPr past the doubles.
    fn counts(&mut self) -> Built {
        let n = (10f64.powi(self.rng.range(3, 16) as i32) * (1.0 + 9.0 * self.rng.unit())).round();
        let a = self.leaf(format!("(cos(0)*{})", decimal(n)), n, false);
        match self.rng.below(3) {
            0 => {
                let text = format!("({})!", a.text);
                self.op1("count", text, &a, factorial)
            }
            k => {
                let r = (n * self.rng.unit()).round().max(1.0);
                let b = self.leaf(format!("(cos(0)*{})", decimal(r)), r, false);
                let (name, perm) = if k == 1 {
                    ("nCr", false)
                } else {
                    ("nPr", true)
                };
                let text = format!("{name}({},{})", a.text, b.text);
                self.op2("count", text, &a, &b, false, |x, y| count(x, y, perm))
            }
        }
    }

    /// A value within reach, often past the doubles: 2^(±M), e^(±c), m·2ᵉ.
    fn within(&mut self) -> Built {
        match self.rng.below(3) {
            0 => {
                let m = self.rng.range(1000, (1 << 53) - 1);
                let s = if self.rng.chance(0.5) { "-" } else { "" };
                let two = self.leaf("2".into(), 2.0, false);
                let t = self.leaf(
                    format!("({s}{m})"),
                    if s.is_empty() { m } else { -m } as f64,
                    false,
                );
                self.pow(&two, Expo::Node(t))
            }
            1 => {
                let c = (10f64.powi(self.rng.range(0, 15) as i32) * (1.0 + 5.0 * self.rng.unit()))
                    .min(6.2e15);
                let c = if self.rng.chance(0.5) { -c } else { c };
                let a = self.leaf(decimal(c), c, false);
                let text = format!("exp({})", a.text);
                self.op1("exp", text, &a, exp)
            }
            _ => {
                // m·2ᵉ, past the doubles at times.
                let m = self.rng.range(1, (1 << 53) - 1);
                let e = self.rng.range(-1200, 1100);
                self.dyadic(m, e, true)
            }
        }
    }

    /// A value far out, mostly.
    fn core(&mut self, depth: u32) -> Built {
        match self.rng.below(10) {
            0..=3 => {
                let b = self.near_one();
                if self.rng.chance(0.3) {
                    // Aimed within reach or at the edge: the mantissa
                    // comes from the logarithm.
                    self.aimed(&b)
                } else {
                    let t = self.big_exponent();
                    self.pow(&b, t)
                }
            }
            4 => {
                // A base nowhere near 1 to a large power.
                let m = self.rng.range(1, 1 << 20);
                let e = self.rng.range(-40, 40);
                let b = self.dyadic(m, e, true);
                let k = self.rng.range(14, 300);
                let s = if self.rng.chance(0.4) { "-" } else { "" };
                let tv: f64 = format!("{s}1e{k}").parse().unwrap();
                let t = self.leaf(format!("({s}10^({k}))"), tv, false);
                self.pow(&b, Expo::Node(t))
            }
            5 | 6 => self.exp_far(),
            7 => self.counts(),
            8 => self.within(),
            _ if depth < 2 => {
                let f = self.core(depth + 1);
                self.transform(f, depth + 1)
            }
            _ => self.exp_far(),
        }
    }

    /// A small power of a far value aimed at the 2⁵³-binade edge, or
    /// at some size within reach: t = target/log₂|f|, written as its
    /// shortest decimal (a non-integer exponent, `wide::pow_var`).
    fn aimed(&mut self, f: &Built) -> Built {
        let a = if f.positive() {
            None
        } else {
            Some(self.abs(f))
        };
        let base = a.as_ref().unwrap_or(f);
        let target = match self.rng.below(3) {
            0 | 1 => self.edge(),
            _ => pow2(self.rng.range(0, 60)) * (1.0 + self.rng.unit()),
        };
        let target = if self.rng.chance(0.5) {
            -target
        } else {
            target
        };
        let l = base
            .input()
            .and_then(|r| r.log2().map(Float::to_f64))
            .filter(|l| l.is_finite() && *l != 0.0);
        let t = match l {
            Some(l) => target / l,
            None => 10f64.powi(-(self.rng.range(1, 300) as i32)),
        };
        let t = if t.is_finite() && t != 0.0 { t } else { 1e-300 };
        let tl = self.leaf(decimal(t), t, false);
        self.pow(base, Expo::Node(tl))
    }

    fn abs(&mut self, f: &Built) -> Built {
        let text = format!("abs({})", f.text);
        self.op1("neg/abs", text, f, |r| Truth::Is(r.abs()))
    }

    /// Another operand: far, or within reach.
    fn other(&mut self, depth: u32) -> Built {
        if self.rng.chance(0.5) {
            self.core(depth + 1)
        } else {
            self.within()
        }
    }

    /// One more operation on f.
    fn transform(&mut self, f: Built, depth: u32) -> Built {
        match self.rng.below(17) {
            0..=2 => self.aimed(&f),
            3 => {
                let a = if f.positive() { f } else { self.abs(&f) };
                let k = self.rng.range(1, 300);
                let s = if self.rng.chance(0.5) { "-" } else { "" };
                let tv: f64 = format!("{s}1e-{k}").parse().unwrap();
                let t = self.leaf(format!("({s}10^(-{k}))"), tv, false);
                self.pow(&a, Expo::Node(t))
            }
            4 => {
                let n = *self.rng.pick(&[-3i64, -2, -1, 2, 3, 5, 10, 999, 1000]);
                self.pow(&f, Expo::Int(n))
            }
            5 => {
                let (p, q) = *self.rng.pick(&[
                    (1i128, 3i128),
                    (2, 3),
                    (-1, 3),
                    (5, 7),
                    (1, 2),
                    (3, 2),
                    (-7, 4),
                ]);
                let a = if q % 2 == 0 && !f.positive() {
                    self.abs(&f)
                } else {
                    f
                };
                self.pow(&a, Expo::Ratio(p, q))
            }
            6 => {
                let a = if f.positive() { f } else { self.abs(&f) };
                let q = *self.rng.pick(&[3i128, 7, 1_000_003]);
                let p = self.rng.range(1_000_001, 1_000_000_000_000) as i128;
                let g = gcd(p, q);
                let (p, q) = (p / g, q / g);
                if q == 1 {
                    return self.pow(&a, Expo::Int(2));
                }
                let p = if self.rng.chance(0.3) { -p } else { p };
                self.pow(&a, Expo::Ratio(p, q))
            }
            7 => {
                let n: f64 = *self
                    .rng
                    .pick(&[2.0, 3.0, 4.0, 5.0, 7.0, 2.5, -3.0, 1000.0, 1e15, 0.25]);
                let odd = n.fract() == 0.0 && n % 2.0 != 0.0;
                let a = if odd || f.positive() { f } else { self.abs(&f) };
                self.root(&a, n)
            }
            8 => {
                if self.rng.chance(0.5) {
                    let a = if f.positive() { f } else { self.abs(&f) };
                    let text = format!("sqrt({})", a.text);
                    self.op1("root", text, &a, |r| pow_mag(r, &fl(0.5)))
                } else {
                    let text = format!("cbrt({})", f.text);
                    self.op1("root", text, &f, |r| root(r, 3.0))
                }
            }
            9 => {
                let g = self.other(depth);
                if self.rng.chance(0.5) {
                    self.mul(&f, &g)
                } else {
                    self.mul(&g, &f)
                }
            }
            10 => {
                let g = self.other(depth);
                match self.rng.below(3) {
                    0 => self.div(&f, &g),
                    1 => self.div(&g, &f),
                    _ => {
                        let one = self.leaf("1".into(), 1.0, false);
                        self.div(&one, &f)
                    }
                }
            }
            11 => {
                // A sum of one sign.
                let a = if f.positive() { f } else { self.abs(&f) };
                let g = self.other(depth);
                let b = if g.positive() { g } else { self.abs(&g) };
                if self.rng.chance(0.5) {
                    self.add(&a, &b)
                } else {
                    self.add(&b, &a)
                }
            }
            12 => {
                let g = self.other(depth);
                if self.rng.chance(0.5) {
                    self.sub(&f, &g)
                } else {
                    self.sub(&g, &f)
                }
            }
            13 => {
                let g = self.other(depth);
                let max = self.rng.chance(0.5);
                if self.rng.chance(0.5) {
                    self.minmax(max, &f, &g)
                } else {
                    self.minmax(max, &g, &f)
                }
            }
            14 => {
                if self.rng.chance(0.5) {
                    self.abs(&f)
                } else {
                    let text = format!("-({})", f.text);
                    self.op1("neg/abs", text, &f, |r| Truth::Is(r.neg()))
                }
            }
            15 => {
                let text = format!("exp({})", f.text);
                self.op1("exp", text, &f, exp)
            }
            _ => self.near(f),
        }
    }

    /// f against a value near its own: e^c with c·log₂e a hair from
    /// log₂ f, by difference, sign, min or max.
    fn near(&mut self, f: Built) -> Built {
        let a = if f.positive() { f } else { self.abs(&f) };
        let l = a
            .input()
            .and_then(|r| r.log2().map(Float::to_f64))
            .filter(|l| l.is_finite() && *l != 0.0);
        let Some(l) = l else {
            return a;
        };
        let c = l * std::f64::consts::LN_2 * (1.0 + self.near_edge());
        let cl = self.leaf(decimal(c), c, false);
        let text = format!("exp({})", cl.text);
        let g = self.op1("exp", text, &cl, exp);
        match self.rng.below(3) {
            0 => {
                let d = self.sub(&a, &g);
                let text = format!("sign({})", d.text);
                self.op1("sign", text, &d, |r| {
                    Truth::Is(match r {
                        Real::Zero => Real::Zero,
                        Real::Val { neg, .. } => Real::Val {
                            neg: *neg,
                            l: fl(0.0),
                        },
                    })
                })
            }
            1 => self.sub(&g, &a),
            _ => {
                let max = self.rng.chance(0.5);
                self.minmax(max, &a, &g)
            }
        }
    }
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

fn show(t: &Truth) -> String {
    match t {
        Truth::Is(Real::Zero) => "0".into(),
        Truth::Is(Real::Val { neg, l }) => format!(
            "{}2^{}",
            if *neg { "-" } else { "" },
            l.to_string_radix(10, Some(20))
        ),
        Truth::Undefined => "undefined".into(),
        Truth::Unknown => "not decided".into(),
    }
}

/// One case's program, checked node by node.
fn run_case(seed: u64, index: u64) -> Case {
    let mut c = Case::new(seed, index);
    let mut f = c.core(0);
    for _ in 0..c.rng.range(0, 4) {
        if c.failure.is_some() {
            break;
        }
        f = c.transform(f, 0);
    }
    if c.failure.is_none() && c.rng.chance(0.3) {
        c.near(f);
    }
    c
}

#[derive(Default)]
struct Report {
    cases: u64,
    nodes: u64,
    failures: Vec<(u64, String)>,
    stats: BTreeMap<&'static str, Stat>,
}

impl Report {
    fn absorb(&mut self, index: u64, c: Case) {
        self.cases += 1;
        self.nodes += c.nodes;
        if let Some(f) = c.failure {
            self.failures.push((index, f));
        }
        for (k, s) in c.stats {
            let t = self.stats.entry(k).or_default();
            t.far += s.far;
            t.infinite += s.infinite;
            t.other += s.other;
            t.worst = t.worst.max(s.worst);
            t.added = t.added.max(s.added);
            t.other_worst = t.other_worst.max(s.other_worst);
        }
    }

    fn print(&self, seed: u64) {
        println!(
            "far invariant: seed {seed}, {} cases, {} nodes, {} failing",
            self.cases,
            self.nodes,
            self.failures.len()
        );
        let lg = |v: f64| {
            if v > 0.0 {
                format!("2^{:.1}", v.log2())
            } else {
                "0".into()
            }
        };
        for (k, s) in &self.stats {
            println!(
                "  {k:<17} far {:>7} (worst {:>9}, one step {:>9}), infinite {:>6}, \
                 other {:>7} (worst {:>9})",
                s.far,
                lg(s.worst),
                lg(s.added),
                s.infinite,
                s.other,
                lg(s.other_worst)
            );
        }
        for (i, f) in self.failures.iter().take(8) {
            println!("case {i}: {f}\n");
        }
    }

    fn assert_clean(&self, seed: u64) {
        assert!(
            self.failures.is_empty(),
            "{} of {} cases break `Wide::far`'s invariant (seed {seed}); the first: case {}: {}",
            self.failures.len(),
            self.cases,
            self.failures[0].0,
            self.failures[0].1
        );
    }
}

fn run(seed: u64, cases: u64, threads: u64) -> Report {
    let report = Mutex::new(Report::default());
    std::thread::scope(|s| {
        for t in 0..threads.max(1) {
            let report = &report;
            s.spawn(move || {
                init();
                let mut i = t;
                while i < cases {
                    let c = run_case(seed, i);
                    report.lock().unwrap().absorb(i, c);
                    i += threads.max(1);
                }
            });
        }
    });
    let mut r = report.into_inner().unwrap();
    r.failures.sort_by_key(|f| f.0);
    r
}

fn env_u64(name: &str) -> Option<u64> {
    let v = std::env::var(name).ok()?;
    Some(
        v.trim()
            .parse()
            .unwrap_or_else(|_| panic!("{name}={v} is not a number")),
    )
}

/// The short run: a fixed seed, seconds.
#[test]
fn short_run() {
    let r = run(SHORT_SEED, SHORT_CASES, 1);
    r.print(SHORT_SEED);
    r.assert_clean(SHORT_SEED);
}

/// The long run: `FAR_SEED`, `FAR_CASES`, `FAR_THREADS` (release).
#[test]
#[ignore = "the long run: minutes"]
fn long_run() {
    let seed = env_u64("FAR_SEED").unwrap_or(SHORT_SEED);
    let cases = env_u64("FAR_CASES").unwrap_or(LONG_CASES);
    let threads = env_u64("FAR_THREADS").unwrap_or(1);
    let started = std::time::Instant::now();
    let r = run(seed, cases, threads);
    r.print(seed);
    println!("{:.1} s", started.elapsed().as_secs_f64());
    r.assert_clean(seed);
}

/// Review 22's example, node by node: (1 − 2⁻⁴⁴)^(10³⁰⁰) is far below the
/// doubles, its exponent within the bound; its 1.098…·10⁻²⁷¹th power is
/// about e^(−6.243·10¹⁵), above e^(−6.2432·10¹⁵).
#[test]
fn review_22s_power() {
    init();
    let mut c = Case::new(0, 0);
    c.want_x = false;
    let b = c.leaf("(cos(0)-2^(-44))".into(), 1.0 - pow2(-44), false);
    let t = c.leaf("(10^(300))".into(), 1e300, false);
    let f = c.pow(&b, Expo::Node(t));
    let s = c.leaf(decimal(1.0982801747529e-271), 1.0982801747529e-271, false);
    let g = c.pow(&f, Expo::Node(s));
    let cl = c.leaf(decimal(-6.2432e15), -6.2432e15, false);
    let text = format!("exp({})", cl.text);
    let e = c.op1("exp", text, &cl, exp);
    c.sub(&g, &e);
    assert!(c.failure.is_none(), "{}", c.failure.unwrap());
    assert!(f.far(), "{:?}", f.held);
}
