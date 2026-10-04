//! The interval core against MPFR at 256 bits (`--features mpfr-oracle`).
//!
//! For every operation and every input box from an adversarial set
//! (subnormals, ±1 neighbourhoods, poles, 10¹⁵, ±10³⁰⁰, infinite and empty
//! boxes) the exact image, sampled at the box's ends, inside it, and at its
//! critical points, must lie in the enclosure; where MPFR finds the
//! function undefined the decoration must not claim it defined; on point
//! inputs the enclosure must be at most a few ulps wide. The Taylor
//! evaluator's coefficients are checked the same way against MPFR values
//! and high-precision finite differences.
//!
//! `cargo test -p graphing --features mpfr-oracle --test interval_oracle`
//! (add `-- --ignored` for the long random fuzz).
#![cfg(feature = "mpfr-oracle")]

use graphing::Equation;
use graphing::ast::{BinOp, Constant, Expr, Func};
use graphing::compile::CompileOptions;
use graphing::functions::TrigUnit;
use graphing::interval::{Ctx, Dec, DecInterval, Interval, Literals, derivs_valid, elem, taylor};
use graphing::lexer::ParseOptions;
use rug::Float;
use rug::float::Constant as MpConst;
use rug::ops::Pow;
use std::collections::HashMap;

const P: u32 = 256;

thread_local! {
    /// The MPFR precision of the truths (raised for finite differences).
    static PREC: std::cell::Cell<u32> = const { std::cell::Cell::new(P) };
}

fn p() -> u32 {
    PREC.get()
}

/// MPFR's widest exponent range, so e^−10¹⁵ is a tiny positive number and
/// not an underflowed 0 (per thread).
fn init() {
    use gmp_mpfr_sys::mpfr;
    unsafe {
        mpfr::set_emin(mpfr::get_emin_min());
        mpfr::set_emax(mpfr::get_emax_max());
    }
}
const INF: f64 = f64::INFINITY;

fn mp(x: f64) -> Float {
    Float::with_val(p(), x)
}

fn pi_mp() -> Float {
    Float::with_val(P, MpConst::Pi)
}

fn pi_mp_p() -> Float {
    Float::with_val(p(), MpConst::Pi)
}

/// `v ∈ iv`, allowing MPFR's own 2⁻²⁰⁰ relative error where the exact
/// value is an endpoint.
fn inside(v: &Float, iv: Interval) -> bool {
    if iv.is_empty() {
        return false;
    }
    if v.is_infinite() {
        return if v.is_sign_positive() {
            iv.hi() == INF
        } else {
            iv.lo() == -INF
        };
    }
    let slack = Float::with_val(P, v.clone().abs() * Float::with_val(P, 2f64.powi(-200)))
        + Float::with_val(P, f64::from_bits(1));
    let lo_ok = iv.lo() == -INF || Float::with_val(P, v + &slack) >= iv.lo();
    let hi_ok = iv.hi() == INF || Float::with_val(P, v - &slack) <= iv.hi();
    lo_ok && hi_ok
}

/// Doubles between two doubles (an ulp distance).
fn ulps(lo: f64, hi: f64) -> u64 {
    let ord = |x: f64| -> i64 {
        let b = x.to_bits() as i64;
        if b < 0 { i64::MIN - b } else { b }
    };
    (ord(hi) - ord(lo)).unsigned_abs()
}

// ---------------------------------------------------------------- inputs

/// The adversarial points: `examples/identities.rs`'s set and more.
fn points() -> Vec<f64> {
    let mut m = vec![
        5e-324,
        1e-320,
        1e-310,
        f64::MIN_POSITIVE,
        1e-300,
        1e-200,
        1e-100,
        1e-20,
        1e-8,
        1e-3,
        0.1,
        0.25,
        0.5,
        0.7,
        0.75,
        0.9,
    ];
    m.extend([
        1.0 - 2f64.powi(-53),
        1.0 - 1e-10,
        1.0,
        1.0 + 2f64.powi(-52),
        1.0 + 1e-10,
        1.5,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::FRAC_PI_2.next_up(),
        2.0,
        std::f64::consts::E,
        3.0,
        std::f64::consts::PI,
        std::f64::consts::PI.next_up(),
        2.0 * std::f64::consts::PI,
        10.0,
        45.0,
        89.99999999999999,
        90.0,
        90.00000000000001,
        100.0,
        180.0,
        200.0,
        270.0,
        354.0,
        360.0,
        400.0,
        700.0,
        709.0,
        710.0,
        745.0,
        746.0,
        1e3,
        1e8,
        1e15,
        1e17,
        1e22,
        1e100,
        1e154,
        1e300,
        f64::MAX,
    ]);
    let mut v = vec![0.0];
    for a in m {
        v.push(a);
        v.push(-a);
    }
    v
}

/// A small deterministic generator (xorshift).
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

    /// Doubles spread over magnitudes and signs.
    fn double(&mut self) -> f64 {
        let r = self.next() % 10;
        let s = if self.next() % 2 == 0 { 1.0 } else { -1.0 };
        let v = match r {
            0 => self.unit() * 2.0,
            1 => self.unit() * 10.0,
            2 => 10f64.powf(self.unit() * 40.0 - 20.0),
            3 => 10f64.powf(self.unit() * 600.0 - 300.0),
            4 => f64::from_bits(self.next() % 0x7ff0_0000_0000_0000),
            5 => self.unit() * 720.0,
            6 => 1.0 + (self.unit() - 0.5) * 1e-6,
            7 => (self.next() % 20) as f64 * std::f64::consts::FRAC_PI_2,
            8 => (self.next() % 9) as f64 * 45.0,
            _ => self.unit() * 1e3,
        };
        s * v
    }
}

/// Boxes: points, neighbours, small and wide, unbounded, entire, empty.
fn boxes(rng: &mut Rng, n_random: usize) -> Vec<Interval> {
    let pts = points();
    let mut out = vec![Interval::EMPTY, Interval::ENTIRE];
    for &p in &pts {
        out.push(Interval::point(p));
        out.push(Interval::new(p, p.next_up()));
        out.push(Interval::new(p, INF));
        out.push(Interval::new(-INF, p));
    }
    for w in pts.windows(2) {
        out.push(Interval::new(w[0].min(w[1]), w[0].max(w[1])));
    }
    for _ in 0..n_random {
        let (a, b) = (rng.double(), rng.double());
        out.push(Interval::new(a.min(b), a.max(b)));
        let c = rng.double();
        let w = c.abs() * 10f64.powf(-rng.unit() * 15.0);
        out.push(Interval::new(c, c + w));
    }
    out.retain(|i| i.is_empty() || !(i.lo().is_nan() || i.hi().is_nan()));
    out
}

// ---------------------------------------------------------- MPFR truths

type Truth2 = Box<dyn Fn(&Float, &Float) -> Option<Float>>;

fn defined(v: Float) -> Option<Float> {
    if v.is_nan() { None } else { Some(v) }
}

fn unit_turn(unit: TrigUnit) -> u32 {
    match unit {
        TrigUnit::Radians => 0,
        TrigUnit::Degrees => 360,
        TrigUnit::Grads => 400,
    }
}

/// Radians → the unit.
fn mp_from_rad(r: Float, unit: TrigUnit) -> Float {
    match unit {
        TrigUnit::Radians => r,
        u => Float::with_val(
            p(),
            r * Float::with_val(p(), u.full_turn()) / Float::with_val(p(), 2 * pi_mp_p()),
        ),
    }
}

fn mp_sin(x: &Float, unit: TrigUnit) -> Float {
    match unit_turn(unit) {
        0 => x.clone().sin(),
        t => x.clone().sin_u(t),
    }
}

fn mp_cos(x: &Float, unit: TrigUnit) -> Float {
    match unit_turn(unit) {
        0 => x.clone().cos(),
        t => x.clone().cos_u(t),
    }
}

/// The app's real function `f` at an exact point, None where undefined.
fn mp_unary(f: Func, x: &Float, unit: TrigUnit) -> Option<Float> {
    use Func::*;
    let one = mp(1.0);
    let zero = x.is_zero();
    Some(match f {
        Sin => mp_sin(x, unit),
        Cos => mp_cos(x, unit),
        Tan | Sec | Csc | Cot => {
            let (s, c) = (mp_sin(x, unit), mp_cos(x, unit));
            let (num, den) = match f {
                Tan => (s, c),
                Sec => (one, c),
                Csc => (one, s),
                _ => (c, s),
            };
            if den.is_zero() {
                return None;
            }
            Float::with_val(p(), num / den)
        }
        Asin | Acos => {
            if x.clone().abs() > 1 {
                return None;
            }
            mp_from_rad(
                if f == Asin {
                    x.clone().asin()
                } else {
                    x.clone().acos()
                },
                unit,
            )
        }
        Atan => mp_from_rad(x.clone().atan(), unit),
        Asec | Acsc => {
            if x.clone().abs() < 1 {
                return None;
            }
            let r = Float::with_val(p(), x.clone().recip());
            mp_from_rad(if f == Asec { r.acos() } else { r.asin() }, unit)
        }
        Acot => {
            let r = if zero {
                Float::with_val(p(), pi_mp_p() / 2)
            } else {
                let a = Float::with_val(p(), x.clone().recip()).atan();
                if x.is_sign_positive() {
                    a
                } else {
                    Float::with_val(p(), pi_mp_p() + a)
                }
            };
            mp_from_rad(r, unit)
        }
        Sinh => x.clone().sinh(),
        Cosh => x.clone().cosh(),
        Tanh => x.clone().tanh(),
        Sech => x.clone().sech(),
        Csch => {
            if zero {
                return None;
            }
            x.clone().csch()
        }
        Coth => {
            if zero {
                return None;
            }
            x.clone().coth()
        }
        Asinh => x.clone().asinh(),
        Acosh => {
            if *x < 1 {
                return None;
            }
            x.clone().acosh()
        }
        Atanh => {
            if x.clone().abs() >= 1 {
                return None;
            }
            x.clone().atanh()
        }
        Asech => {
            if !(*x > 0 && *x <= 1) {
                return None;
            }
            Float::with_val(p(), x.clone().recip()).acosh()
        }
        Acsch => {
            if zero {
                return None;
            }
            Float::with_val(p(), x.clone().recip()).asinh()
        }
        Acoth => {
            if x.clone().abs() <= 1 {
                return None;
            }
            Float::with_val(p(), x.clone().recip()).atanh()
        }
        Sqrt => {
            if *x < 0 {
                return None;
            }
            x.clone().sqrt()
        }
        Cbrt => x.clone().cbrt(),
        Ln | Log => {
            if *x <= 0 {
                return None;
            }
            if f == Ln {
                x.clone().ln()
            } else {
                x.clone().log10()
            }
        }
        Exp => x.clone().exp(),
        Abs => x.clone().abs(),
        Floor => x.clone().floor(),
        Ceil => x.clone().ceil(),
        Round => x.clone().round(),
        Sign => {
            if zero {
                mp(0.0)
            } else {
                x.clone().signum()
            }
        }
        Factorial => {
            let a = Float::with_val(p(), x + 1u32);
            if a <= 0 && a.is_integer() {
                return None;
            }
            a.gamma()
        }
        _ => return None,
    })
    .and_then(defined)
}

/// Integer power with the app's 0⁰ = 1 and undefined 0 to a negative power.
fn mp_powi(x: &Float, n: i32) -> Option<Float> {
    if x.is_zero() && n < 0 {
        return None;
    }
    Some(Float::with_val(p(), x.clone().pow(n)))
}

/// x^(p/q), q > 1, real-root semantics.
fn mp_powrat(x: &Float, p: i32, q: i32) -> Option<Float> {
    if x.is_zero() {
        return if p > 0 { Some(mp(0.0)) } else { None };
    }
    let neg = x.is_sign_negative();
    if neg && q % 2 == 0 {
        return None;
    }
    let a = x.clone().abs();
    let pr = PREC.get();
    let e = Float::with_val(pr, Float::with_val(pr, p) / q);
    let m = Float::with_val(pr, a.pow(&e));
    Some(if neg && p % 2 != 0 { -m } else { m })
}

/// b^e for a general exponent: b > 0, or b = 0 with e > 0.
fn mp_pow(b: &Float, e: &Float) -> Option<Float> {
    if b.is_zero() {
        return if *e > 0 { Some(mp(0.0)) } else { None };
    }
    if *b < 0 {
        return None;
    }
    defined(Float::with_val(p(), b.clone().pow(e)))
}

fn mp_mod(a: &Float, b: &Float) -> Option<Float> {
    if b.is_zero() {
        return None;
    }
    // MPFR's fmod is exact at any quotient; then floored (b's sign).
    let r = Float::with_val(p(), a % b);
    if !r.is_zero() && r.is_sign_negative() != b.is_sign_negative() {
        Some(Float::with_val(p(), r + b))
    } else {
        Some(r)
    }
}

// --------------------------------------------------------- the checks

#[derive(Default)]
struct Tally {
    cases: usize,
    samples: usize,
    violations: Vec<String>,
    widest: HashMap<String, u64>,
}

impl Tally {
    fn fail(&mut self, s: String) {
        if self.violations.len() < 60 {
            self.violations.push(s);
        } else if self.violations.len() == 60 {
            self.violations.push("…".into());
        }
    }
}

/// Sample points of a box: its ends, inside, and `extra` (critical points).
fn samples(iv: Interval, rng: &mut Rng, extra: &[Float]) -> Vec<Float> {
    if iv.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for v in [iv.lo(), iv.hi(), iv.mid()] {
        if v.is_finite() {
            out.push(mp(v));
        }
    }
    if iv.contains(0.0) {
        out.push(mp(0.0));
    }
    for v in [1.0, -1.0] {
        if iv.contains(v) {
            out.push(mp(v));
        }
    }
    if iv.lo().is_finite() && iv.hi().is_finite() {
        for _ in 0..4 {
            let t = rng.unit();
            let v = iv.lo() + (iv.hi() - iv.lo()) * t;
            if iv.contains(v) {
                out.push(mp(v));
            }
        }
        // Interior points as exact high-precision values (between doubles).
        let w = Float::with_val(P, Float::with_val(P, iv.hi()) - iv.lo());
        out.push(Float::with_val(
            P,
            Float::with_val(P, w.clone() / 3u32) + iv.lo(),
        ));
    }
    for e in extra {
        let ok = (iv.lo() == -INF || *e >= iv.lo()) && (iv.hi() == INF || *e <= iv.hi());
        if ok {
            out.push(e.clone());
        }
    }
    out
}

/// Points of `k·step + offset` inside the box (at most a few), exactly.
fn lattice(iv: Interval, step: &Float, offset: &Float) -> Vec<Float> {
    let mut out = Vec::new();
    if !(iv.lo().is_finite() && iv.hi().is_finite()) || iv.lo().abs() > 1e40 || iv.hi().abs() > 1e40
    {
        return out;
    }
    let k0 = Float::with_val(P, (Float::with_val(P, iv.lo()) - offset) / step).ceil();
    let k1 = Float::with_val(P, (Float::with_val(P, iv.hi()) - offset) / step).floor();
    let mut k = k0;
    let mut n = 0;
    while k <= k1 && n < 6 {
        out.push(Float::with_val(P, Float::with_val(P, &k * step) + offset));
        k += 1u32;
        n += 1;
    }
    out
}

/// Critical points (extrema, jumps) of f inside a box, in the unit.
fn critical(f: Func, iv: Interval, unit: TrigUnit) -> Vec<Float> {
    use Func::*;
    let half = match unit {
        TrigUnit::Radians => pi_mp(),
        u => mp(u.full_turn() / 2.0),
    };
    let quarter = Float::with_val(P, &half / 2u32);
    match f {
        Sin | Csc => lattice(iv, &half, &quarter),
        Cos | Sec => lattice(iv, &half, &mp(0.0)),
        Floor | Ceil => lattice(iv, &mp(1.0), &mp(0.0)),
        Round => lattice(iv, &mp(1.0), &mp(0.5)),
        _ => Vec::new(),
    }
}

/// How many ulps a point input's enclosure may span.
fn tightness(f: Func, unit: TrigUnit) -> u64 {
    use Func::*;
    let degrees = unit != TrigUnit::Radians;
    match f {
        Exp | Ln | Log | Sqrt | Cbrt | Sinh | Cosh | Tanh | Asinh | Acosh | Atanh => 2,
        Sin | Cos | Tan | Asin | Acos | Atan if !degrees => 2,
        Sin | Cos => 4,
        Abs | Floor | Ceil | Round | Sign => 0,
        _ => 16,
    }
}

fn check_point_width(t: &mut Tally, name: &str, r: &DecInterval, truth: &Float, limit: u64) {
    if r.is_empty() || !r.iv.is_bounded() || truth.is_zero() {
        return;
    }
    let tv = truth.to_f64();
    // Near the subnormals a composite's roundings lose bits by design.
    if !tv.is_normal() || tv.abs() > 1e300 || tv.abs() < 1e-290 {
        return;
    }
    let u = ulps(r.lo(), r.hi());
    let w = t.widest.entry(name.to_string()).or_default();
    *w = (*w).max(u);
    if u > limit {
        t.fail(format!(
            "{name}: point enclosure [{:e}, {:e}] is {u} ulps (limit {limit})",
            r.lo(),
            r.hi()
        ));
    }
}

fn check_unary(
    t: &mut Tally,
    name: &str,
    op: &dyn Fn(&DecInterval) -> DecInterval,
    truth: &dyn Fn(&Float) -> Option<Float>,
    crit: &dyn Fn(Interval) -> Vec<Float>,
    limit: u64,
    boxes: &[Interval],
    rng: &mut Rng,
) {
    for &b in boxes {
        let x = DecInterval::new(b);
        let r = op(&x);
        t.cases += 1;
        let extra = crit(b);
        for s in samples(b, rng, &extra) {
            t.samples += 1;
            match truth(&s) {
                Some(v) => {
                    if !inside(&v, r.iv) {
                        t.fail(format!(
                            "{name}([{:e}, {:e}]) = [{:e}, {:e}] {:?} misses f({}) = {}",
                            b.lo(),
                            b.hi(),
                            r.lo(),
                            r.hi(),
                            r.dec,
                            s.to_string_radix(10, Some(20)),
                            v.to_string_radix(10, Some(20))
                        ));
                    } else if b.is_point() {
                        check_point_width(t, name, &r, &v, limit);
                    }
                }
                None => {
                    if r.dec >= Dec::Def {
                        t.fail(format!(
                            "{name}([{:e}, {:e}]) claims {:?} but is undefined at {}",
                            b.lo(),
                            b.hi(),
                            r.dec,
                            s.to_string_radix(10, Some(20))
                        ));
                    }
                }
            }
        }
        // Strict-sign facts must hold at every defined sample.
        if r.pos || r.neg {
            for s in samples(b, rng, &extra) {
                if let Some(v) = truth(&s) {
                    // Beyond MPFR's own exponent range a tiny value is 0.
                    let mpfr_underflow = v.is_zero() && s.clone().abs() > 1e15;
                    if !mpfr_underflow && ((r.pos && v <= 0) || (r.neg && v >= 0)) {
                        t.fail(format!(
                            "{name}: sign fact wrong at {}",
                            s.to_string_radix(10, Some(20))
                        ));
                    }
                }
            }
        }
    }
}

fn check_binary(
    t: &mut Tally,
    name: &str,
    op: &dyn Fn(&DecInterval, &DecInterval) -> DecInterval,
    truth: &dyn Fn(&Float, &Float) -> Option<Float>,
    pairs: &[(Interval, Interval)],
    limit: u64,
    rng: &mut Rng,
) {
    for &(a, b) in pairs {
        let r = op(&DecInterval::new(a), &DecInterval::new(b));
        t.cases += 1;
        let (sa, sb) = (samples(a, rng, &[]), samples(b, rng, &[]));
        for x in &sa {
            for y in &sb {
                t.samples += 1;
                match truth(x, y) {
                    Some(v) => {
                        if !inside(&v, r.iv) {
                            t.fail(format!(
                                "{name}([{:e},{:e}], [{:e},{:e}]) = [{:e}, {:e}] misses f({}, {}) = {}",
                                a.lo(),
                                a.hi(),
                                b.lo(),
                                b.hi(),
                                r.lo(),
                                r.hi(),
                                x.to_string_radix(10, Some(17)),
                                y.to_string_radix(10, Some(17)),
                                v.to_string_radix(10, Some(17))
                            ));
                        } else if a.is_point() && b.is_point() {
                            check_point_width(t, name, &r, &v, limit);
                        }
                    }
                    None => {
                        if r.dec >= Dec::Def {
                            t.fail(format!(
                                "{name}([{:e},{:e}], [{:e},{:e}]) claims {:?} but is undefined at ({}, {})",
                                a.lo(),
                                a.hi(),
                                b.lo(),
                                b.hi(),
                                r.dec,
                                x.to_string_radix(10, Some(17)),
                                y.to_string_radix(10, Some(17))
                            ));
                        }
                    }
                }
            }
        }
    }
}

fn unary_ops(unit: TrigUnit) -> Vec<(String, Func, Box<dyn Fn(&DecInterval) -> DecInterval>)> {
    use Func::*;
    let u = unit;
    let mut v: Vec<(String, Func, Box<dyn Fn(&DecInterval) -> DecInterval>)> = vec![
        ("sin".into(), Sin, Box::new(move |x| elem::sin(x, u))),
        ("cos".into(), Cos, Box::new(move |x| elem::cos(x, u))),
        ("tan".into(), Tan, Box::new(move |x| elem::tan(x, u))),
        ("sec".into(), Sec, Box::new(move |x| elem::sec(x, u))),
        ("csc".into(), Csc, Box::new(move |x| elem::csc(x, u))),
        ("cot".into(), Cot, Box::new(move |x| elem::cot(x, u))),
        ("asin".into(), Asin, Box::new(move |x| elem::asin(x, u))),
        ("acos".into(), Acos, Box::new(move |x| elem::acos(x, u))),
        ("atan".into(), Atan, Box::new(move |x| elem::atan(x, u))),
        ("asec".into(), Asec, Box::new(move |x| elem::asec(x, u))),
        ("acsc".into(), Acsc, Box::new(move |x| elem::acsc(x, u))),
        ("acot".into(), Acot, Box::new(move |x| elem::acot(x, u))),
    ];
    if unit == TrigUnit::Radians {
        let more: Vec<(&str, Func, Box<dyn Fn(&DecInterval) -> DecInterval>)> = vec![
            ("sinh", Sinh, Box::new(elem::sinh)),
            ("cosh", Cosh, Box::new(elem::cosh)),
            ("tanh", Tanh, Box::new(elem::tanh)),
            ("sech", Sech, Box::new(elem::sech)),
            ("csch", Csch, Box::new(elem::csch)),
            ("coth", Coth, Box::new(elem::coth)),
            ("asinh", Asinh, Box::new(elem::asinh)),
            ("acosh", Acosh, Box::new(elem::acosh)),
            ("atanh", Atanh, Box::new(elem::atanh)),
            ("asech", Asech, Box::new(elem::asech)),
            ("acsch", Acsch, Box::new(elem::acsch)),
            ("acoth", Acoth, Box::new(elem::acoth)),
            ("sqrt", Sqrt, Box::new(elem::sqrt)),
            ("cbrt", Cbrt, Box::new(elem::cbrt)),
            ("ln", Ln, Box::new(elem::ln)),
            ("log10", Log, Box::new(elem::log10)),
            ("exp", Exp, Box::new(elem::exp)),
            ("abs", Abs, Box::new(elem::abs)),
            ("floor", Floor, Box::new(elem::floor)),
            ("ceil", Ceil, Box::new(elem::ceil)),
            ("round", Round, Box::new(elem::round)),
            ("sign", Sign, Box::new(elem::sign)),
        ];
        v.extend(more.into_iter().map(|(n, f, b)| (n.to_string(), f, b)));
    }
    v.into_iter()
        .map(|(n, f, b)| (format!("{n}[{unit:?}]"), f, b))
        .collect()
}

fn run_ops(n_random: usize, seed: u64) -> Tally {
    init();
    let mut rng = Rng(seed);
    let bxs = boxes(&mut rng, n_random);
    let mut t = Tally::default();
    for unit in [TrigUnit::Radians, TrigUnit::Degrees, TrigUnit::Grads] {
        for (name, f, op) in unary_ops(unit) {
            let truth = move |x: &Float| mp_unary(f, x, unit);
            let crit = move |iv: Interval| critical(f, iv, unit);
            check_unary(
                &mut t,
                &name,
                &*op,
                &truth,
                &crit,
                tightness(f, unit),
                &bxs,
                &mut rng,
            );
        }
    }
    // Powers.
    for n in [-3, -2, -1, 0, 2, 3, 5, 6] {
        let op = move |x: &DecInterval| elem::powi(x, n);
        let truth = move |x: &Float| mp_powi(x, n);
        check_unary(
            &mut t,
            &format!("powi({n})"),
            &op,
            &truth,
            &|_| Vec::new(),
            2,
            &bxs,
            &mut rng,
        );
    }
    for (p, q) in [
        (1, 3),
        (2, 3),
        (-1, 3),
        (1, 2),
        (3, 2),
        (-1, 2),
        (5, 7),
        (-2, 5),
        (7, 4),
    ] {
        let op = move |x: &DecInterval| elem::pow_rational(x, p, q);
        let truth = move |x: &Float| mp_powrat(x, p, q);
        check_unary(
            &mut t,
            &format!("pow({p}/{q})"),
            &op,
            &truth,
            &|_| Vec::new(),
            4,
            &bxs,
            &mut rng,
        );
    }
    // Binary operations on a smaller set of pairs.
    let small: Vec<Interval> = bxs.iter().copied().step_by(7).collect();
    let mut pairs = Vec::new();
    for (i, a) in small.iter().enumerate() {
        for b in small.iter().skip(i % 5).step_by(9) {
            pairs.push((*a, *b));
        }
    }
    let bin: Vec<(
        &str,
        Box<dyn Fn(&DecInterval, &DecInterval) -> DecInterval>,
        Truth2,
        u64,
    )> = vec![
        (
            "add",
            Box::new(elem::add),
            Box::new(|a: &Float, b: &Float| Some(Float::with_val(P, a + b))),
            1,
        ),
        (
            "sub",
            Box::new(elem::sub),
            Box::new(|a: &Float, b: &Float| Some(Float::with_val(P, a - b))),
            1,
        ),
        // (Below 2⁻⁹⁶⁹ a product's error isn't checked for exactness: 2 ulps.)
        (
            "mul",
            Box::new(elem::mul),
            Box::new(|a: &Float, b: &Float| Some(Float::with_val(P, a * b))),
            2,
        ),
        (
            "div",
            Box::new(elem::div),
            Box::new(|a: &Float, b: &Float| {
                if b.is_zero() {
                    None
                } else {
                    Some(Float::with_val(P, a / b))
                }
            }),
            // (Beyond 10³⁰⁶ the residual check is skipped: 2 ulps.)
            2,
        ),
        (
            "pow",
            Box::new(elem::pow),
            Box::new(|a: &Float, b: &Float| mp_pow(a, b)),
            2,
        ),
        (
            "min",
            Box::new(elem::min),
            Box::new(|a: &Float, b: &Float| Some(a.clone().min(b))),
            0,
        ),
        (
            "max",
            Box::new(elem::max),
            Box::new(|a: &Float, b: &Float| Some(a.clone().max(b))),
            0,
        ),
        (
            "mod",
            Box::new(elem::modulo),
            Box::new(|a: &Float, b: &Float| mp_mod(a, b)),
            8,
        ),
        (
            "log_base",
            Box::new(elem::log_base),
            Box::new(|b: &Float, x: &Float| {
                if *b <= 0 || *b == 1 || *x <= 0 {
                    None
                } else {
                    Some(Float::with_val(P, x.clone().ln() / b.clone().ln()))
                }
            }),
            8,
        ),
    ];
    for (name, op, truth, limit) in &bin {
        check_binary(&mut t, name, &**op, &**truth, &pairs, *limit, &mut rng);
    }
    t
}

fn report(t: &Tally) {
    eprintln!(
        "interval oracle: {} cases, {} samples, {} violations",
        t.cases,
        t.samples,
        t.violations.len()
    );
    let mut w: Vec<_> = t.widest.iter().collect();
    w.sort();
    let s: Vec<String> = w.iter().map(|(k, v)| format!("{k} {v}")).collect();
    eprintln!("widest point enclosures (ulps): {}", s.join(", "));
    for v in &t.violations {
        eprintln!("  {v}");
    }
}

#[test]
fn every_operation_encloses_mpfr() {
    let t = run_ops(150, 0x9E37_79B9_7F4A_7C15);
    report(&t);
    assert!(
        t.violations.is_empty(),
        "{} enclosure violations",
        t.violations.len()
    );
}

#[test]
#[ignore = "long: random fuzz of every operation"]
fn every_operation_encloses_mpfr_fuzz() {
    let mut total = Tally::default();
    for seed in 1..=8u64 {
        let t = run_ops(1500, seed.wrapping_mul(0x2545_F491_4F6C_DD1D));
        total.cases += t.cases;
        total.samples += t.samples;
        total.violations.extend(t.violations);
        for (k, v) in t.widest {
            let w = total.widest.entry(k).or_default();
            *w = (*w).max(v);
        }
    }
    report(&total);
    assert!(total.violations.is_empty());
}

#[test]
fn constants_enclose_mpfr() {
    init();
    let pi = elem::pi();
    assert!(inside(&pi_mp(), pi));
    let e = elem::e();
    assert!(inside(&Float::with_val(P, 1u32).exp(), e));
}

// ------------------------------------------------- the Taylor evaluator

/// Decimal literal texts by the double they parse to (plain test inputs).
fn literal_texts(src: &str) -> HashMap<u64, String> {
    let mut out = HashMap::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let glued = i > 0 && (chars[i - 1].is_alphabetic() || chars[i - 1] == '_');
        if chars[i].is_ascii_digit() && !glued {
            let s = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let txt: String = chars[s..i].iter().collect();
            if let Ok(v) = txt.parse::<f64>() {
                out.insert(v.to_bits(), txt);
            }
        } else {
            i += 1;
        }
    }
    out
}

fn syntactic_rational(e: &Expr) -> Option<(i32, i32)> {
    fn int(e: &Expr) -> Option<i64> {
        match e {
            Expr::Num(v) if *v == v.trunc() && v.abs() < 1e6 => Some(*v as i64),
            Expr::Neg(a) => int(a).map(|v| -v),
            _ => None,
        }
    }
    let (p, q) = match e {
        Expr::Neg(a) => {
            let (p, q) = syntactic_rational(a)?;
            return Some((-p, q));
        }
        Expr::Bin(BinOp::Div, a, b) => (int(a)?, int(b)?),
        _ => (int(e)?, 1),
    };
    if q == 0 {
        return None;
    }
    let mut g = (p.abs(), q.abs());
    while g.1 != 0 {
        g = (g.1, g.0 % g.1);
    }
    let g = g.0.max(1);
    let (mut p, mut q) = (p / g, q / g);
    if q < 0 {
        p = -p;
        q = -q;
    }
    Some((p as i32, q as i32))
}

/// The app's function at an exact point, in MPFR.
fn mp_eval(
    e: &Expr,
    x: &Float,
    unit: TrigUnit,
    lits: &HashMap<u64, String>,
    prec: u32,
) -> Option<Float> {
    let ev = |a: &Expr| mp_eval(a, x, unit, lits, prec);
    Some(match e {
        Expr::Num(v) => match lits.get(&v.to_bits()) {
            Some(t) => Float::with_val(prec, Float::parse(t).ok()?),
            None => Float::with_val(prec, *v),
        },
        Expr::Const(Constant::Pi) => Float::with_val(prec, MpConst::Pi),
        Expr::Const(Constant::E) => Float::with_val(prec, 1u32).exp(),
        Expr::X => x.clone(),
        Expr::Y | Expr::Var(_) => return None,
        Expr::Neg(a) => -ev(a)?,
        Expr::Degrees(a) => ev(a)?,
        Expr::Bin(BinOp::Pow, a, b) => {
            let u = ev(a)?;
            if let Some((p, q)) = syntactic_rational(b) {
                return if q == 1 {
                    mp_powi(&u, p)
                } else {
                    mp_powrat(&u, p, q)
                };
            }
            let v = ev(b)?;
            if !b.contains_x() && v.is_integer() && v.clone().abs() < 1e6 {
                return mp_powi(&u, v.to_f64() as i32);
            }
            return mp_pow(&u, &v);
        }
        Expr::Bin(op, a, b) => {
            let (u, v) = (ev(a)?, ev(b)?);
            match op {
                BinOp::Add => Float::with_val(prec, u + v),
                BinOp::Sub => Float::with_val(prec, u - v),
                BinOp::Mul => Float::with_val(prec, u * v),
                BinOp::Div => {
                    if v.is_zero() {
                        return None;
                    }
                    Float::with_val(prec, u / v)
                }
                BinOp::Pow => unreachable!(),
            }
        }
        Expr::Call(f, args) => {
            let a0 = ev(&args[0])?;
            match f {
                Func::LogBase => {
                    let x2 = ev(&args[1])?;
                    if a0 <= 0 || a0 == 1 || x2 <= 0 {
                        return None;
                    }
                    Float::with_val(prec, x2.ln() / a0.ln())
                }
                Func::Mod => mp_mod(&a0, &ev(&args[1])?)?,
                Func::Root => {
                    let n = ev(&args[1])?;
                    if n.is_integer() && n.clone().abs() < 1e6 && !n.is_zero() {
                        let k = n.to_f64() as i32;
                        return if k > 0 {
                            mp_powrat(&a0, 1, k)
                        } else {
                            mp_powrat(&a0, -1, -k)
                        };
                    }
                    return mp_pow(&a0, &Float::with_val(prec, n.recip()));
                }
                Func::Min | Func::Max => {
                    let mut acc = a0;
                    for a in &args[1..] {
                        let b = ev(a)?;
                        acc = if *f == Func::Min {
                            acc.min(&b)
                        } else {
                            acc.max(&b)
                        };
                    }
                    acc
                }
                f => {
                    let r = mp_unary(*f, &a0, unit)?;
                    Float::with_val(prec, r)
                }
            }
        }
    })
    .and_then(defined)
}

/// Expressions covering every function, with their units.
fn taylor_cases() -> Vec<(&'static str, TrigUnit)> {
    use TrigUnit::*;
    vec![
        ("x^2-2x+1/(x-1)", Radians),
        ("sin(x)*cos(3x)+sin(5x)/x", Radians),
        ("tan(x)+sec(x)-csc(x)+cot(x)", Radians),
        ("sin(x)+cos(x)+tan(x)", Degrees),
        ("sin(x)/cos(x)", Grads),
        ("asin(x/2)+acos(x/3)+atan(x)", Radians),
        ("asin(x/2)+atan(x)", Degrees),
        ("asec(x)+acsc(x)+acot(x)", Radians),
        ("acot(x)", Degrees),
        ("sinh(x)+cosh(x)-tanh(x)", Radians),
        ("sech(x)+csch(x)+coth(x)", Radians),
        ("asinh(x)+acosh(x+2)+atanh(x/3)", Radians),
        ("asech(x/4)+acsch(x)+acoth(x+2)", Radians),
        ("sqrt(x)+cbrt(x)+root(x,5)", Radians),
        ("x^(1/3)+x^(2/3)-x^(-1/3)", Radians),
        ("x^x+2^x", Radians),
        ("ln(x)+log(x)+log(3,x)", Radians),
        ("exp(-x^2)+e^x", Radians),
        ("abs(x-1)+floor(x)+ceil(x)+round(x)+sign(x)", Radians),
        ("mod(x,3)+min(x,1)+max(x,2)", Radians),
        ("(x-1000000000000)/(x-1000000000000.0625)", Radians),
        ("0.000001*x+0.1", Radians),
        ("1/(1+(x-1522756)^2/0.000000000001)+exp(-x^2)", Radians),
        ("exp(-(x-100)^2)+exp(-x^2)", Radians),
        ("x^(-2)+x^3-x^(-3)", Radians),
        ("(x-1)^2/(x-1)^2", Radians),
    ]
}

/// f at ξ, and its first three derivatives by central differences in MPFR
/// at 512 bits (errors far below a double's ulp).
fn mp_derivs(
    e: &Expr,
    xi: &Float,
    unit: TrigUnit,
    lits: &HashMap<u64, String>,
) -> Option<[Float; 4]> {
    let pr = 1024;
    PREC.set(pr);
    let r = mp_derivs_at(e, xi, unit, lits, pr);
    PREC.set(P);
    r
}

fn mp_derivs_at(
    e: &Expr,
    xi: &Float,
    unit: TrigUnit,
    lits: &HashMap<u64, String>,
    pr: u32,
) -> Option<[Float; 4]> {
    // Every evaluation at the full precision (an MPFR function keeps its
    // argument's).
    let xi = &Float::with_val(pr, xi);
    let f = |dx: f64, h: &Float| -> Option<Float> {
        let at = Float::with_val(pr, xi + Float::with_val(pr, h * dx));
        mp_eval(e, &at, unit, lits, pr)
    };
    // Absolute steps (the features here are far wider), at a precision
    // where x + h is exact for any double x.
    let h1 = Float::with_val(pr, 2f64.powi(-120));
    let h2 = Float::with_val(pr, 2f64.powi(-100));
    let h3 = Float::with_val(pr, 2f64.powi(-90));
    let f0 = mp_eval(e, xi, unit, lits, pr)?;
    let d1 = Float::with_val(pr, (f(1.0, &h1)? - f(-1.0, &h1)?) / (2u32 * h1.clone()));
    let d2 = Float::with_val(
        pr,
        (f(1.0, &h2)? - 2u32 * f0.clone() + f(-1.0, &h2)?) / h2.clone().square(),
    );
    let d3 = Float::with_val(
        pr,
        (f(2.0, &h3)? - 2u32 * f(1.0, &h3)? + 2u32 * f(-1.0, &h3)? - f(-2.0, &h3)?)
            / (2u32 * Float::with_val(pr, h3.clone().pow(3u32))),
    );
    // Taylor coefficients: f, f', f''/2, f'''/6.
    Some([
        f0,
        d1,
        Float::with_val(pr, d2 / 2u32),
        Float::with_val(pr, d3 / 6u32),
    ])
}

/// Containment for a finite-difference estimate: 10⁻⁴⁰ relative covers its
/// truncation error (h ≤ 2⁻⁹⁰), and |f|·2⁻⁷⁰⁰ its rounding noise (1024 bits
/// over h³ = 2⁻²⁷⁰) where the coefficient is 0.
fn inside_fd(v: &Float, iv: Interval, f0: &Float) -> bool {
    let pr = 1024;
    let noise = Float::with_val(
        pr,
        Float::with_val(pr, f0.clone().abs() + 1u32) * Float::with_val(pr, 2f64.powi(-700)),
    );
    let tol = Float::with_val(pr, v.clone().abs() * Float::with_val(pr, 1e-40)) + noise;
    let lo_ok = iv.lo() == -INF || Float::with_val(pr, v + &tol) >= iv.lo();
    let hi_ok = iv.hi() == INF || Float::with_val(pr, v - &tol) <= iv.hi();
    lo_ok && hi_ok
}

fn run_taylor(n_boxes: usize, seed: u64) -> Tally {
    init();
    let mut rng = Rng(seed);
    let mut t = Tally::default();
    for (src, unit) in taylor_cases() {
        let text = format!("y={src}");
        let eq = Equation::parse(&text).unwrap_or_else(|e| panic!("{src}: {e:?}"));
        let (_, ast) = eq.explicit().expect("explicit");
        let lits = Literals::of(&text, ParseOptions::default()).unwrap();
        let mp_lits = literal_texts(&text);
        let opts = CompileOptions {
            trig_unit: unit,
            ..CompileOptions::default()
        };
        let ctx = Ctx::new(opts, &lits);
        let mut bxs: Vec<Interval> = Vec::new();
        for &c in &[
            0.3,
            1.7,
            -2.4,
            3.0,
            47.0,
            1522756.0,
            1000000000000.0625,
            100.0,
            -0.6,
            2.5,
        ] {
            bxs.push(Interval::point(c));
            bxs.push(Interval::new(c, c + 1e-6 * c.abs().max(1.0)));
            bxs.push(Interval::new(c - 0.01, c + 0.01));
        }
        for _ in 0..n_boxes {
            let c = (rng.unit() - 0.5) * 20.0;
            let w = 10f64.powf(-rng.unit() * 10.0);
            bxs.push(Interval::new(c, c + w));
        }
        for b in bxs {
            let s = taylor(ast, b, 3, &ctx);
            t.cases += 1;
            let ok_d = derivs_valid(&s, 3);
            for xi in samples(b, &mut rng, &[]) {
                t.samples += 1;
                match mp_eval(ast, &xi, unit, &mp_lits, P) {
                    Some(v) => {
                        if !inside(&v, s[0].iv) {
                            t.fail(format!(
                                "taylor {src} [{:e},{:e}]: c0 [{:e},{:e}] misses f({}) = {}",
                                b.lo(),
                                b.hi(),
                                s[0].lo(),
                                s[0].hi(),
                                xi.to_string_radix(10, Some(20)),
                                v.to_string_radix(10, Some(20))
                            ));
                        }
                    }
                    None => {
                        if s[0].dec >= Dec::Def {
                            t.fail(format!(
                                "taylor {src} [{:e},{:e}] claims {:?} but undefined at {}",
                                b.lo(),
                                b.hi(),
                                s[0].dec,
                                xi.to_string_radix(10, Some(20))
                            ));
                        }
                    }
                }
                if ok_d && let Some(d) = mp_derivs(ast, &xi, unit, &mp_lits) {
                    for k in 1..=3 {
                        if !inside_fd(&d[k], s[k].iv, &d[0]) {
                            t.fail(format!(
                                "taylor {src} [{:e},{:e}]: c{k} [{:e},{:e}] misses {} at {}",
                                b.lo(),
                                b.hi(),
                                s[k].lo(),
                                s[k].hi(),
                                d[k].to_string_radix(10, Some(20)),
                                xi.to_string_radix(10, Some(20))
                            ));
                        }
                    }
                }
            }
        }
    }
    t
}

#[test]
fn taylor_coefficients_enclose_mpfr() {
    let t = run_taylor(40, 0xD1B5_4A32_D192_ED03);
    report(&t);
    assert!(
        t.violations.is_empty(),
        "{} Taylor violations",
        t.violations.len()
    );
}

#[test]
#[ignore = "long: random fuzz of the Taylor evaluator"]
fn taylor_coefficients_enclose_mpfr_fuzz() {
    let t = run_taylor(1500, 0x8CB9_2BA7_2F3D_8DD7);
    report(&t);
    assert!(t.violations.is_empty());
}

/// A literal that is a double is exact: the pole of
/// (x − 10¹²)/(x − 1000000000000.0625) is at exactly that double.
#[test]
fn exact_literals_place_poles_exactly() {
    let text = "y=(x-1000000000000)/(x-1000000000000.0625)";
    let eq = Equation::parse(text).unwrap();
    let (_, ast) = eq.explicit().unwrap();
    let lits = Literals::of(text, ParseOptions::default()).unwrap();
    let ctx = Ctx::new(CompileOptions::default(), &lits);
    let r = graphing::interval::enclose(ast, Interval::point(1000000000000.0625), &ctx);
    assert!(r.is_empty() && r.dec <= Dec::Trv, "{r:?}");
    let r = graphing::interval::enclose(ast, Interval::point(1000000000000.0), &ctx);
    assert_eq!(r.iv, Interval::point(0.0));
}
