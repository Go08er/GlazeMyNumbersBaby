//! Every simplifier rule against MPFR (`--features mpfr-oracle`).
//!
//! For each rule `lhs → rhs` (`graphing::simplify::rules::RULES`), each
//! angle unit it can depend on, and about a thousand values of its pattern
//! variables chosen inside its stated condition (extremes ±10³⁰⁰,
//! subnormals, ±1 and its neighbours, near poles, and random magnitudes),
//! both sides are evaluated with MPFR under the graphing evaluator's
//! meanings (the three powers, exact quarter turns in degrees and grads,
//! acot's (0, π) branch): **wherever the left side is defined, the right
//! side must be defined and equal**. A rule may enlarge the domain, never
//! shrink it.
//!
//! Precision adapts: each side is evaluated at 1100, 2200, … bits until two
//! successive precisions agree to 2⁻⁶⁰, so cancellation in the sample
//! itself (1 − cos 10⁻³⁰⁰) isn't mistaken for a wrong rule; samples that
//! never settle are counted, not judged.
//!
//! `cargo test -p graphing --features mpfr-oracle --test simplify_rules -- --nocapture`
#![cfg(feature = "mpfr-oracle")]

use std::collections::HashMap;

use egg::{ENodeOrVar, Pattern, Var};
use graphing::functions::TrigUnit;
use graphing::simplify::Q;
use graphing::simplify::lang::Math;
use graphing::simplify::rules::{RULES, RuleSpec, VarCond};
use rug::Float;
use rug::float::Constant as MpConst;
use rug::ops::Pow;

fn init() {
    use gmp_mpfr_sys::mpfr;
    unsafe {
        mpfr::set_emin(mpfr::get_emin_min());
        mpfr::set_emax(mpfr::get_emax_max());
    }
}

/// A value: the real, and its exact rational when known (exponents of
/// `^` need it).
#[derive(Clone, Debug)]
struct Val {
    f: Float,
    q: Option<Q>,
}

fn fq(prec: u32, q: Q) -> Float {
    Float::with_val(prec, q.numer()) / Float::with_val(prec, q.denom())
}

fn val_q(prec: u32, q: Q) -> Val {
    Val {
        f: fq(prec, q),
        q: Some(q),
    }
}

/// A sampled variable value: a double (or a multiple of π), with its exact
/// rational if it is one we want treated as such.
#[derive(Clone, Copy, Debug)]
enum Sample {
    Double(f64),
    Exact(Q),
    /// k·π (radian turns).
    PiTimes(Q),
}

fn sample_val(prec: u32, s: Sample) -> Val {
    match s {
        Sample::Double(v) => Val {
            f: Float::with_val(prec, v),
            q: Q::from_f64(v).filter(|q| q.denom() <= 1 << 20),
        },
        Sample::Exact(q) => val_q(prec, q),
        Sample::PiTimes(k) => Val {
            f: fq(prec, k) * Float::with_val(prec, MpConst::Pi),
            q: None,
        },
    }
}

/// The unit's full turn in its own measure (radians: None, use π).
fn full_turn(unit: TrigUnit) -> Option<u32> {
    match unit {
        TrigUnit::Radians => None,
        TrigUnit::Degrees => Some(360),
        TrigUnit::Grads => Some(400),
    }
}

fn from_rad(prec: u32, r: Float, unit: TrigUnit) -> Float {
    match full_turn(unit) {
        None => r,
        Some(t) => {
            let two_pi = Float::with_val(prec, MpConst::Pi) * 2u32;
            r * Float::with_val(prec, t) / two_pi
        }
    }
}

fn sin_u(x: &Float, unit: TrigUnit) -> Float {
    match full_turn(unit) {
        None => x.clone().sin(),
        Some(t) => x.clone().sin_u(t),
    }
}

fn cos_u(x: &Float, unit: TrigUnit) -> Float {
    match full_turn(unit) {
        None => x.clone().cos(),
        Some(t) => x.clone().cos_u(t),
    }
}

fn is_int(f: &Float) -> bool {
    f.is_finite() && f.is_integer()
}

/// One node under the evaluator's meanings; None where undefined.
fn node(prec: u32, n: &Math, kids: &[Val], unit: TrigUnit) -> Option<Val> {
    let k = |i: usize| &kids[i];
    let f1 = |v: Float| Some(Val { f: v, q: None });
    let exact2 =
        |a: &Val, b: &Val, op: &dyn Fn(Q, Q) -> Option<Q>| a.q.zip(b.q).and_then(|(x, y)| op(x, y));
    match n {
        Math::Add(_) => Some(Val {
            f: Float::with_val(prec, &k(0).f + &k(1).f),
            q: exact2(k(0), k(1), &|a, b| a.add(b)),
        }),
        Math::Sub(_) => Some(Val {
            f: Float::with_val(prec, &k(0).f - &k(1).f),
            q: exact2(k(0), k(1), &|a, b| a.sub(b)),
        }),
        Math::Mul(_) => Some(Val {
            f: Float::with_val(prec, &k(0).f * &k(1).f),
            q: exact2(k(0), k(1), &|a, b| a.mul(b)),
        }),
        Math::Div(_) => {
            if k(1).f.is_zero() {
                return None;
            }
            Some(Val {
                f: Float::with_val(prec, &k(0).f / &k(1).f),
                q: exact2(k(0), k(1), &|a, b| a.div(b)),
            })
        }
        Math::Neg(_) => Some(Val {
            f: Float::with_val(prec, -&k(0).f),
            q: k(0).q.and_then(|q| q.neg()),
        }),
        Math::PowQ(_) => {
            // A syntactic rational exponent: integer powers, real roots.
            let (b, e) = (&k(0).f, k(1));
            let r = e.q?;
            if b.is_zero() {
                return if r.signum() > 0 {
                    f1(Float::with_val(prec, 0))
                } else {
                    None
                };
            }
            if r.is_int() {
                let n = i32::try_from(r.numer()).ok()?;
                return f1(Float::with_val(prec, b.clone().pow(n)));
            }
            let (p, d) = (
                i32::try_from(r.numer()).ok()?,
                u32::try_from(r.denom()).ok()?,
            );
            let neg = b.is_sign_negative();
            if neg && d % 2 == 0 {
                return None;
            }
            let root = Float::with_val(prec, b.clone().abs().root(d));
            let mut v = Float::with_val(prec, root.pow(p));
            if neg && p % 2 != 0 {
                v = -v;
            }
            f1(v)
        }
        Math::PowC(_) => {
            // functions::pow: a negative base only to an integer value.
            let (b, e) = (&k(0).f, &k(1).f);
            if b.is_zero() {
                return if *e > 0 {
                    f1(Float::with_val(prec, 0))
                } else {
                    None
                };
            }
            if b.is_sign_negative() && !is_int(e) {
                return None;
            }
            f1(Float::with_val(prec, b.clone().pow(e)))
        }
        Math::PowV(_) => {
            // The TI rule: base > 0, or 0 to a positive power.
            let (b, e) = (&k(0).f, &k(1).f);
            if b.is_zero() {
                return if *e > 0 {
                    f1(Float::with_val(prec, 0))
                } else {
                    None
                };
            }
            if b.is_sign_negative() {
                return None;
            }
            f1(Float::with_val(prec, b.clone().pow(e)))
        }
        Math::Sin(_) => f1(sin_u(&k(0).f, unit)),
        Math::Cos(_) => f1(cos_u(&k(0).f, unit)),
        Math::Tan(_) | Math::Sec(_) | Math::Csc(_) | Math::Cot(_) => {
            let x = &k(0).f;
            let (s, c) = (sin_u(x, unit), cos_u(x, unit));
            let one = Float::with_val(prec, 1);
            let (num, den) = match n {
                Math::Tan(_) => (s, c),
                Math::Sec(_) => (one, c),
                Math::Csc(_) => (one, s),
                _ => (c, s),
            };
            if den.is_zero() {
                return None;
            }
            f1(num / den)
        }
        Math::Asin(_) | Math::Acos(_) => {
            let x = &k(0).f;
            if x.clone().abs() > 1 {
                return None;
            }
            let r = if matches!(n, Math::Asin(_)) {
                x.clone().asin()
            } else {
                x.clone().acos()
            };
            f1(from_rad(prec, r, unit))
        }
        Math::Atan(_) => f1(from_rad(prec, k(0).f.clone().atan(), unit)),
        Math::Asec(_) | Math::Acsc(_) => {
            let x = &k(0).f;
            if x.clone().abs() < 1 {
                return None;
            }
            let r = x.clone().recip();
            let r = if matches!(n, Math::Asec(_)) {
                r.acos()
            } else {
                r.asin()
            };
            f1(from_rad(prec, r, unit))
        }
        Math::Acot(_) => {
            let x = &k(0).f;
            let pi = Float::with_val(prec, MpConst::Pi);
            let r = if x.is_zero() {
                pi / 2u32
            } else {
                let a = x.clone().recip().atan();
                if x.is_sign_positive() { a } else { pi + a }
            };
            f1(from_rad(prec, r, unit))
        }
        Math::Sinh(_) => f1(k(0).f.clone().sinh()),
        Math::Cosh(_) => f1(k(0).f.clone().cosh()),
        Math::Tanh(_) => f1(k(0).f.clone().tanh()),
        Math::Asinh(_) => f1(k(0).f.clone().asinh()),
        Math::Atanh(_) => {
            if k(0).f.clone().abs() >= 1 {
                return None;
            }
            f1(k(0).f.clone().atanh())
        }
        Math::Sqrt(_) => {
            if k(0).f.is_sign_negative() && !k(0).f.is_zero() {
                return None;
            }
            f1(k(0).f.clone().sqrt())
        }
        Math::Cbrt(_) => f1(k(0).f.clone().cbrt()),
        Math::Exp(_) => f1(k(0).f.clone().exp()),
        Math::Ln(_) => {
            if k(0).f <= 0 {
                return None;
            }
            f1(k(0).f.clone().ln())
        }
        Math::Abs(_) => Some(Val {
            f: k(0).f.clone().abs(),
            q: k(0).q.and_then(|q| q.abs()),
        }),
        Math::Num(q) => Some(val_q(prec, *q)),
        Math::Symbol(s) => match s.as_str() {
            "pi" => f1(Float::with_val(prec, MpConst::Pi)),
            "e" => f1(Float::with_val(prec, 1).exp()),
            other => panic!("symbol {other} in a rule"),
        },
        other => panic!("{other:?} in a rule: add it to the MPFR evaluator"),
    }
}

/// Evaluates a pattern at `prec` bits with the variables bound. The flag
/// says a sum or difference of two *different* subterms cancelled nearly
/// all the bits it had (cosh² a − sinh² a at a = 10¹⁵): the value is noise
/// at this precision. (The same subterm twice, as in a − a, is a true 0.)
fn eval(
    pat: &Pattern<Math>,
    binds: &HashMap<Var, Sample>,
    prec: u32,
    unit: TrigUnit,
) -> (Option<Val>, bool) {
    let nodes = pat.ast.as_ref();
    let mut vals: Vec<Option<Val>> = Vec::with_capacity(nodes.len());
    let mut keys: Vec<String> = Vec::with_capacity(nodes.len());
    let mut lost = false;
    for n in nodes {
        let (v, key) = match n {
            ENodeOrVar::Var(v) => (Some(sample_val(prec, binds[v])), v.to_string()),
            ENodeOrVar::ENode(e) => {
                let kids_ids = egg::Language::children(e);
                let key = format!(
                    "({} {})",
                    e,
                    kids_ids
                        .iter()
                        .map(|c| keys[usize::from(*c)].as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                let kids: Option<Vec<Val>> = kids_ids
                    .iter()
                    .map(|c| vals[usize::from(*c)].clone())
                    .collect();
                let v = match &kids {
                    Some(kids) => node(prec, e, kids, unit),
                    None => None,
                };
                if let (Math::Add([a, b]) | Math::Sub([a, b]), Some(kids), Some(r)) = (e, &kids, &v)
                    && keys[usize::from(*a)] != keys[usize::from(*b)]
                    && (kids[0].q.is_none() || kids[1].q.is_none())
                {
                    let m = Float::with_val(
                        prec,
                        kids[0].f.clone().abs().max(&kids[1].f.clone().abs()),
                    );
                    let floor =
                        Float::with_val(prec, &m * Float::with_val(64, 64 - prec as i32).exp2());
                    if !m.is_zero() && r.f.clone().abs() <= floor {
                        lost = true;
                    }
                }
                (v, key)
            }
        };
        // NaN is an undefined operation's result.
        vals.push(v.filter(|x| !x.f.is_nan()));
        keys.push(key);
    }
    (vals.last().cloned().flatten(), lost)
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Undefined,
    Value(Float),
    /// Precision never settled, or the value overflowed.
    Unresolved,
}

fn settled(pat: &Pattern<Math>, binds: &HashMap<Var, Sample>, unit: TrigUnit) -> Outcome {
    let tol = Float::with_val(64, -60).exp2();
    let mut last: Option<Option<Float>> = None;
    for prec in [1100, 2200, 4400, 8800] {
        // An intermediate beyond even MPFR's exponent range (e^(10²²⁶))
        // makes the sample undecidable, whatever the final value.
        unsafe { gmp_mpfr_sys::mpfr::clear_flags() };
        let (v, lost) = eval(pat, binds, prec, unit);
        let v = v.map(|v| v.f);
        let out_of_range = unsafe {
            gmp_mpfr_sys::mpfr::overflow_p() != 0 || gmp_mpfr_sys::mpfr::underflow_p() != 0
        };
        if out_of_range || v.as_ref().is_some_and(|f| f.is_infinite()) {
            return Outcome::Unresolved;
        }
        if lost {
            // Try more bits; nothing to compare at this precision.
            last = None;
            continue;
        }
        if let Some(prev) = &last {
            match (prev, &v) {
                (None, None) => return Outcome::Undefined,
                (Some(a), Some(b)) => {
                    let d = Float::with_val(prec, a - b).abs();
                    let m = Float::with_val(prec, a.clone().abs().max(&b.clone().abs()));
                    if d <= Float::with_val(prec, &m * &tol) {
                        return Outcome::Value(b.clone());
                    }
                }
                _ => {}
            }
        }
        last = Some(v);
    }
    Outcome::Unresolved
}

/// A small deterministic generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

const SPECIAL: &[f64] = &[
    0.0,
    1.0,
    -1.0,
    2.0,
    -2.0,
    0.5,
    -0.5,
    3.0,
    -7.0,
    1e-300,
    -1e-300,
    5e-324,
    -5e-324,
    2.2250738585072014e-308,
    1e300,
    -1e300,
    1e15,
    -1e15,
    1e-15,
    1.0000000000000002,
    0.9999999999999999,
    -1.0000000000000002,
    -0.9999999999999999,
    std::f64::consts::FRAC_PI_2,
    std::f64::consts::PI,
    -std::f64::consts::FRAC_PI_2,
    90.0,
    180.0,
    270.0,
    100.0,
    200.0,
    45.0,
];

fn free(rng: &mut Rng, i: usize) -> Sample {
    if i < SPECIAL.len() {
        return Sample::Double(SPECIAL[i]);
    }
    match rng.int(0, 4) {
        0 => Sample::Double((rng.unit() - 0.5) * 20.0),
        1 => {
            let mag = 10f64.powf((rng.unit() - 0.5) * 600.0);
            Sample::Double(if rng.int(0, 1) == 0 { mag } else { -mag })
        }
        2 => Sample::Exact(Q::new(rng.int(-12, 12) as i128, rng.int(1, 6) as i128).unwrap()),
        3 => Sample::Double(1.0 + (rng.unit() - 0.5) * 1e-6),
        _ => Sample::Double((rng.unit() - 0.5) * 2.0),
    }
}

fn sample(c: Option<VarCond>, rng: &mut Rng, i: usize, unit: TrigUnit) -> Sample {
    let halves = |k: i64| -> Sample {
        let k = Q::int(k as i128);
        match unit {
            TrigUnit::Radians => Sample::PiTimes(k),
            TrigUnit::Degrees => Sample::Exact(k.mul(Q::int(180)).unwrap()),
            TrigUnit::Grads => Sample::Exact(k.mul(Q::int(200)).unwrap()),
        }
    };
    match c {
        None => free(rng, i),
        Some(VarCond::Pos) => match free(rng, i) {
            Sample::Double(v) if v > 0.0 => Sample::Double(v),
            Sample::Double(v) if v < 0.0 => Sample::Double(-v),
            Sample::Exact(q) if q.signum() > 0 => Sample::Exact(q),
            Sample::Exact(q) if q.signum() < 0 => Sample::Exact(q.neg().unwrap()),
            _ => Sample::Double(1.5),
        },
        Some(VarCond::Neg) => match sample(Some(VarCond::Pos), rng, i, unit) {
            Sample::Double(v) => Sample::Double(-v),
            Sample::Exact(q) => Sample::Exact(q.neg().unwrap()),
            s => s,
        },
        Some(VarCond::Int) => Sample::Exact(Q::int(rng.int(-8, 8) as i128)),
        Some(VarCond::EvenInt) => Sample::Exact(Q::int(2 * rng.int(-4, 4) as i128)),
        Some(VarCond::OddInt) => Sample::Exact(Q::int(2 * rng.int(-4, 3) as i128 + 1)),
        Some(VarCond::OddDenom) => Sample::Exact(
            Q::new(rng.int(-6, 6) as i128, [1, 3, 5][rng.int(0, 2) as usize]).unwrap(),
        ),
        Some(VarCond::Turn) => halves(2 * rng.int(-3, 3)),
        Some(VarCond::OddHalfTurn) => halves(2 * rng.int(-3, 2) + 1),
        Some(VarCond::HalfTurn) => halves(rng.int(-5, 5)),
    }
}

fn depends_on_unit(r: &RuleSpec) -> bool {
    let trig = [
        "sin", "cos", "tan", "sec", "csc", "cot", "asin", "acos", "atan", "asec", "acsc", "acot",
        "QUARTER", "HALF", "TURN",
    ];
    let words = |s: &str| {
        s.replace(['(', ')'], " ")
            .split_whitespace()
            .any(|w| trig.contains(&w))
    };
    words(r.lhs) || words(r.rhs)
}

#[derive(Default, Debug)]
struct Tally {
    samples: usize,
    lhs_defined: usize,
    unresolved: usize,
}

fn check_rule(r: &RuleSpec, unit: TrigUnit, n: usize, tally: &mut Tally) -> Vec<String> {
    let lhs: Pattern<Math> = r.lhs_for(unit).parse().unwrap();
    let rhs: Pattern<Math> = r.rhs_for(unit).parse().unwrap();
    let vars = lhs.vars();
    let conds: HashMap<Var, VarCond> = r
        .conds
        .iter()
        .map(|(v, c)| (v.parse().unwrap(), *c))
        .collect();
    let mut rng = Rng(0x9e3779b97f4a7c15 ^ (r.name.len() as u64 * 0x100000001b3));
    let mut failures = Vec::new();
    for i in 0..n {
        let binds: HashMap<Var, Sample> = vars
            .iter()
            .enumerate()
            .map(|(j, v)| (*v, sample(conds.get(v).copied(), &mut rng, i + 7 * j, unit)))
            .collect();
        tally.samples += 1;
        let l = settled(&lhs, &binds, unit);
        let Outcome::Value(lv) = l else {
            if l == Outcome::Unresolved {
                tally.unresolved += 1;
            }
            continue;
        };
        tally.lhs_defined += 1;
        match settled(&rhs, &binds, unit) {
            Outcome::Value(rv) => {
                let tol = Float::with_val(64, -50).exp2();
                let d = Float::with_val(1100, &lv - &rv).abs();
                let m = Float::with_val(1100, lv.clone().abs().max(&rv.clone().abs()));
                if d > Float::with_val(1100, &m * &tol) {
                    failures.push(format!(
                        "{} [{unit:?}] {binds:?}: lhs {} rhs {}",
                        r.name,
                        lv.to_f64(),
                        rv.to_f64()
                    ));
                }
            }
            Outcome::Undefined => failures.push(format!(
                "{} [{unit:?}] {binds:?}: lhs defined ({}), rhs undefined",
                r.name,
                lv.to_f64()
            )),
            Outcome::Unresolved => tally.unresolved += 1,
        }
    }
    failures
}

#[test]
fn every_rule_holds_wherever_its_left_side_is_defined() {
    init();
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    let mut per_rule = Vec::new();
    for r in RULES {
        let units: &[TrigUnit] = if depends_on_unit(r) {
            &[TrigUnit::Radians, TrigUnit::Degrees, TrigUnit::Grads]
        } else {
            &[TrigUnit::Radians]
        };
        for unit in units {
            let n = if *unit == TrigUnit::Radians {
                1000
            } else {
                300
            };
            let before = tally.lhs_defined;
            failures.extend(check_rule(r, *unit, n, &mut tally));
            per_rule.push((r.name, *unit, tally.lhs_defined - before));
        }
    }
    eprintln!(
        "{} rules, {} samples, {} with the left side defined, {} unresolved",
        RULES.len(),
        tally.samples,
        tally.lhs_defined,
        tally.unresolved
    );
    // A rule never exercised where its left side is defined proves nothing.
    let starved: Vec<_> = per_rule.iter().filter(|(_, _, k)| *k < 20).collect();
    assert!(starved.is_empty(), "rules barely exercised: {starved:?}");
    assert!(
        failures.is_empty(),
        "{} failures, first: {:#?}",
        failures.len(),
        &failures[..failures.len().min(10)]
    );
}

/// The check is not vacuous: wrong rules of each kind are caught.
#[test]
fn wrong_rules_are_caught() {
    init();
    let bogus = [
        // A wrong value: cos is even, not odd.
        RuleSpec {
            name: "bogus-cos-odd",
            lhs: "(cos (neg ?a))",
            rhs: "(neg (cos ?a))",
            conds: &[],
            why: "",
        },
        // A shrunk domain: a is defined where ln a isn't.
        RuleSpec {
            name: "bogus-exp-ln-back",
            lhs: "(* ?a 1)",
            rhs: "(exp (ln ?a))",
            conds: &[],
            why: "",
        },
        // A missing branch condition: acot a = atan(1/a) only for a > 0.
        RuleSpec {
            name: "bogus-acot",
            lhs: "(acot ?a)",
            rhs: "(atan (/ 1 ?a))",
            conds: &[],
            why: "",
        },
        // A wrong period: odd half turns flip sin's sign.
        RuleSpec {
            name: "bogus-sin-half",
            lhs: "(sin (+ ?a ?w))",
            rhs: "(sin ?a)",
            conds: &[("?w", VarCond::HalfTurn)],
            why: "",
        },
        // A real-root exponent read as a general power (negative bases).
        RuleSpec {
            name: "bogus-cbrt",
            lhs: "(cbrt ?a)",
            rhs: "(^c ?a (/ 1 3))",
            conds: &[],
            why: "",
        },
    ];
    for r in &bogus {
        for unit in [TrigUnit::Radians, TrigUnit::Degrees] {
            let mut t = Tally::default();
            let f = check_rule(r, unit, 300, &mut t);
            assert!(!f.is_empty(), "{} [{unit:?}] wasn't caught", r.name);
        }
    }
}
