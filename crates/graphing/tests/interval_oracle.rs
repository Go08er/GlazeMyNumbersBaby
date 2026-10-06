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
use graphing::ast::{BinOp, Constant, Expr, Func, Lit};
use graphing::compile::CompileOptions;
use graphing::functions::TrigUnit;
use graphing::interval::{Ctx, Dec, DecInterval, Interval, derivs_valid, elem, taylor};
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
        let s = if self.next().is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
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
type UnaryOp = Box<dyn Fn(&DecInterval) -> DecInterval>;
type BinaryOp = Box<dyn Fn(&DecInterval, &DecInterval) -> DecInterval>;

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

/// Integer power with 0 to a power ≤ 0 undefined (0⁰ included: the
/// graphing rule, after the TI-84 Plus CE).
fn mp_powi(x: &Float, n: i32) -> Option<Float> {
    if x.is_zero() && n <= 0 {
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

/// x^(p/q) in lowest terms for p and q of any size, real-root semantics:
/// sign(x)ᵖ·|x|^(p/q), a negative x only for an odd q.
fn mp_powratio(x: &Float, p: &rug::Integer, q: &rug::Integer) -> Option<Float> {
    if x.is_zero() {
        return if *p > 0 { Some(mp(0.0)) } else { None };
    }
    let neg = x.is_sign_negative();
    if neg && q.is_even() {
        return None;
    }
    let a = x.clone().abs();
    let pr = PREC.get();
    let e = Float::with_val(pr, rug::Rational::from((p.clone(), q.clone())));
    let m = Float::with_val(pr, a.pow(&e));
    Some(if neg && p.is_odd() { -m } else { m })
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

/// root(x, n) for a degree n ≠ 0 of any size: an odd integer takes a
/// negative x (sign(x)·|x|^(1/n)), anything else needs x ≥ 0 (x > 0 for
/// n < 0).
fn mp_root(x: &Float, n: &Float, prec: u32) -> Option<Float> {
    if n.is_zero() {
        return None;
    }
    let e = Float::with_val(prec, n.clone().recip());
    let odd = n.to_integer().is_some_and(|k| n.is_integer() && k.is_odd());
    if odd && *x < 0 {
        let m = mp_pow(&Float::with_val(prec, x.clone().abs()), &e)?;
        return Some(-m);
    }
    mp_pow(x, &e)
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
        Factorial => factorial_extrema()
            .iter()
            .filter(|e| iv.contains(e.to_f64()))
            .take(6)
            .cloned()
            .collect(),
        _ => Vec::new(),
    }
}

/// Where n! = Γ(n + 1) has its extrema: ψ(n + 1) = 0, once on (0, ∞) and
/// once between each two poles (the first 60), by bisection in MPFR.
fn factorial_extrema() -> &'static [Float] {
    static E: std::sync::OnceLock<Vec<Float>> = std::sync::OnceLock::new();
    E.get_or_init(|| {
        init();
        let root = |lo: f64, hi: f64| {
            // ψ increases on each branch.
            let (mut a, mut b) = (Float::with_val(P, lo), Float::with_val(P, hi));
            for _ in 0..P + 8 {
                let m = Float::with_val(P, &a + &b) / 2u32;
                if Float::with_val(P, m.digamma_ref()) < 0 {
                    a = m;
                } else {
                    b = m;
                }
            }
            Float::with_val(P, &a - 1u32)
        };
        let mut v = vec![root(1.0, 2.0)];
        for n in 0..60 {
            v.push(root(-(n as f64) - 1.0, -(n as f64)));
        }
        v
    })
}

/// nCr(n, r) and nPr(n, r) as `functions` defines them: the exact count
/// for whole n ≥ 0 and whole r (0 outside 0 ≤ r ≤ n), else through Γ,
/// undefined where an argument of Γ is at a pole.
fn mp_ncr(n: &Float, r: &Float, perm: bool) -> Option<Float> {
    if n.is_integer() && r.is_integer() && *n >= 0 && (*r < 0 || *r > *n) {
        return Some(mp(0.0));
    }
    // Each argument exactly (a sum of doubles needs up to ~2100 bits; at
    // the working precision −1 + 8·10⁻²⁰² would round onto Γ's pole), and
    // at the working precision where that is exact (fast).
    let g = |v: Float| {
        if v <= 0 && v.is_integer() {
            return None;
        }
        let short = Float::with_val(p(), &v);
        Some(if short == v {
            short.gamma()
        } else {
            Float::with_val(p(), v.gamma_ref())
        })
    };
    let n1 = Float::with_val(4200, n + 1u32);
    let num = g(n1.clone())?;
    let den = g(Float::with_val(4200, &n1 - r))?;
    let mut q = Float::with_val(p(), num / den);
    if !perm {
        q /= g(Float::with_val(4200, r + 1u32))?;
    }
    defined(q)
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

#[allow(clippy::too_many_arguments)]
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

fn unary_ops(unit: TrigUnit) -> Vec<(String, Func, UnaryOp)> {
    use Func::*;
    let u = unit;
    let mut v: Vec<(String, Func, UnaryOp)> = vec![
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
        let more: Vec<(&str, Func, UnaryOp)> = vec![
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
            ("factorial", Factorial, Box::new(elem::factorial)),
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
    let bin: Vec<(&str, BinaryOp, Truth2, u64)> = vec![
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

/// Γ, n!, nCr and nPr over boxes on every branch: between and next to the
/// poles, around the extrema, past the overflow, and nCr/nPr in n with a
/// count r (a polynomial there) or a fractional r (Γ quotients).
#[test]
fn factorials_enclose_mpfr() {
    init();
    let mut rng = Rng(0x5EED_F00D);
    let mut bxs = Vec::new();
    for k in -25..=25 {
        let k = k as f64;
        for (a, b) in [
            (0.01, 0.99),
            (0.4, 0.6),
            (1e-9, 1e-6),
            (1.0 - 1e-6, 1.0 - 1e-9),
            (0.25, 0.5),
            (0.5, 0.75),
            (-0.5, 0.5),
            (0.0, 1.0),
        ] {
            bxs.push(Interval::new(k + a, k + b));
        }
        bxs.push(Interval::point(k + 0.5));
        bxs.push(Interval::point(k));
    }
    for e in factorial_extrema() {
        let c = e.to_f64();
        bxs.push(Interval::new(c - 1e-3, c + 1e-3));
        bxs.push(Interval::new(c, c + 1e-12));
    }
    for (a, b) in [
        (169.0, 170.0),
        (170.0, 171.5),
        (171.0, 180.0),
        (1e-300, 1e-200),
        (5e-324, 1e-300),
        (-171.7, -171.6),
        (-180.5, -180.4),
        (-1000.6, -1000.4),
        (0.0, 3.0),
        (2.0, INF),
    ] {
        bxs.push(Interval::new(a, b));
    }
    let mut t = Tally::default();
    let truth = |x: &Float| mp_unary(Func::Factorial, x, TrigUnit::Radians);
    let crit = |iv: Interval| critical(Func::Factorial, iv, TrigUnit::Radians);
    check_unary(
        &mut t,
        "factorial",
        &elem::factorial,
        &truth,
        &crit,
        16,
        &bxs,
        &mut rng,
    );
    let ns: Vec<Interval> = bxs
        .iter()
        .copied()
        .filter(|b| b.lo().abs() <= 60.0 && b.hi().abs() <= 60.0)
        .collect();
    let rs = [0.0, 1.0, 2.0, 3.0, 5.0, 0.5, 2.5, -1.5];
    let mut pairs = Vec::new();
    for n in &ns {
        for &r in &rs {
            pairs.push((*n, Interval::point(r)));
        }
    }
    for perm in [false, true] {
        let op = move |a: &DecInterval, b: &DecInterval| elem::ncr_npr(a, b, perm);
        let truth = move |a: &Float, b: &Float| mp_ncr(a, b, perm);
        let name = if perm { "nPr" } else { "nCr" };
        check_binary(&mut t, name, &op, &truth, &pairs, 64, &mut rng);
    }
    report(&t);
    assert!(t.violations.is_empty(), "{:#?}", t.violations);
}

/// An exact count, against the evaluator's value (rounded once to the
/// nearest double) and the interval core's enclosure of it (the doubles
/// either side, a point when it is one).
fn check_count(
    t: &mut Tally,
    name: &str,
    exact: &rug::Integer,
    value: f64,
    enclosure: Option<DecInterval>,
) {
    t.cases += 1;
    let v = Float::with_val(8192, exact);
    let nearest = Float::with_val(53, exact).to_f64();
    if value.to_bits() != nearest.to_bits() {
        t.fail(format!(
            "{name} = {value:e}, not {nearest:e} (rounded once)"
        ));
    }
    let Some(r) = enclosure else { return };
    if r.dec < Dec::Def || !inside(&v, r.iv) {
        t.fail(format!(
            "{name} enclosed as [{:e}, {:e}] {:?}, missing {}",
            r.lo(),
            r.hi(),
            r.dec,
            exact
        ));
    } else if nearest.is_finite() && r.hi() > r.lo().next_up() {
        t.fail(format!(
            "{name} enclosed as [{:e}, {:e}], wider than the doubles either side",
            r.lo(),
            r.hi()
        ));
    }
}

/// n!!, nCr and nPr of whole numbers: the evaluator rounds the exact count
/// once, and the interval core encloses it by the doubles either side
/// (review 12, R12-M-03: 99!! was a rounded running product widened by an
/// ulp, which missed the true value).
#[test]
fn counting_functions_are_exact() {
    use graphing::functions::{double_factorial, ncr, npr};
    use rug::Integer;
    let mut t = Tally::default();
    let pt = DecInterval::point;
    for n in -1i32..=320 {
        let exact = if n < 0 {
            Integer::from(1)
        } else {
            Integer::from(Integer::factorial_2(n as u32))
        };
        let v = n as f64;
        check_count(
            &mut t,
            &format!("{n}!!"),
            &exact,
            double_factorial(v),
            Some(elem::double_factorial(&pt(v))),
        );
    }
    // The review's case: 99!! is above 6625061298371663·2²⁰⁸.
    let r = elem::double_factorial(&pt(99.0));
    assert!(r.hi() > 6625061298371663.0 * 2f64.powi(208), "{r:?}");
    let ns = [
        61.0,
        100.0,
        1000.0,
        1021.0,
        1030.0,
        1e6,
        9007199254740992.0,
        1e17,
        1e300,
    ];
    let rs = [
        0.0, 1.0, 2.0, 3.0, 5.0, 10.0, 50.0, 100.0, 500.0, 510.0, 515.0, 999.0,
    ];
    let mut pairs: Vec<(f64, f64)> = Vec::new();
    for n in 0..=130 {
        for r in -1..=(n + 2).max(62) {
            pairs.push((n as f64, r as f64));
        }
    }
    for &n in &ns {
        for &r in &rs {
            pairs.push((n, r));
        }
        // Next to n, and past it (a count of 0, review 13: nCr(61, 62) was
        // Γ(0) to the interval core, and empty).
        for d in [-2.0, -1.0, 0.0, 1.0, 2.0, 1e3] {
            if n + d >= 0.0 {
                pairs.push((n, n + d));
            }
        }
        pairs.push((n, -1.0));
        pairs.push((n, -1e300));
        pairs.push((n, 1e300));
    }
    for (n, r) in pairs {
        let ni = Integer::from_f64(n).expect("whole");
        let ri = Integer::from_f64(r).expect("whole");
        for perm in [false, true] {
            // The exact count, or None past the doubles (more than 1100
            // bits: the evaluator says +∞ there).
            let exact = if ri < 0 || ri > ni {
                Some(Integer::new())
            } else if perm {
                // n!/(n − r)! as a product of r factors, stopped once
                // beyond the doubles.
                let mut p = Integer::from(1);
                let mut i = Integer::new();
                while i < ri && p.significant_bits() <= 1100 {
                    p *= Integer::from(&ni - &i);
                    i += 1;
                }
                (p.significant_bits() <= 1100).then_some(p)
            } else {
                // C(n, k) = C(n, n − k) ≥ 2^k for the smaller k: past the
                // doubles once k is over 1100.
                let k = Integer::from(&ni - &ri).min(ri.clone());
                k.to_u32()
                    .filter(|&k| k <= 1100 || ni.significant_bits() <= 12)
                    .map(|k| Integer::from(ni.binomial_ref(k)))
                    .filter(|c| c.significant_bits() <= 1100)
            };
            let name = format!("{}({n}, {r})", if perm { "nPr" } else { "nCr" });
            let value = if perm { npr(n, r) } else { ncr(n, r) };
            let iv = elem::ncr_npr(&pt(n), &pt(r), perm);
            let Some(exact) = exact else {
                if value != f64::INFINITY {
                    t.fail(format!("{name} = {value:e}, not +∞"));
                }
                t.cases += 1;
                if iv.dec < Dec::Def || iv.lo() != f64::MAX || iv.hi() != f64::INFINITY {
                    t.fail(format!("{name} past the doubles enclosed as {iv:?}"));
                }
                continue;
            };
            check_count(&mut t, &name, &exact, value, Some(iv));
        }
    }
    // Arguments that aren't whole numbers ≥ 0 go through Γ: the enclosure
    // is empty only where the evaluator is undefined too (Γ at a pole).
    let vals = [
        -3.0, -2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 61.0, 61.5, 62.0, 62.5, 100.25,
    ];
    for &n in &vals {
        for &r in &vals {
            for perm in [false, true] {
                let value = if perm { npr(n, r) } else { ncr(n, r) };
                let iv = elem::ncr_npr(&pt(n), &pt(r), perm);
                t.cases += 1;
                if iv.is_empty() && !value.is_nan() {
                    t.fail(format!(
                        "{}({n}, {r}) = {value:e}, but enclosed as empty",
                        if perm { "nPr" } else { "nCr" }
                    ));
                }
                if value.is_finite() && !iv.is_empty() && !(iv.lo() <= value && value <= iv.hi()) {
                    let w = (value.abs() * 1e-12).max(1e-300);
                    if !(iv.lo() - w <= value && value <= iv.hi() + w) {
                        t.fail(format!(
                            "{}({n}, {r}) = {value:e} outside {iv:?}",
                            if perm { "nPr" } else { "nCr" }
                        ));
                    }
                }
            }
        }
    }
    report(&t);
    assert!(t.violations.is_empty(), "{:#?}", t.violations);
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

/// An exponent written as a ratio of integers, each an integer as typed
/// (`1.0000000000000001` is none, though its double is 1: each number of
/// the tree carries the decimal typed, `Lit`), of any size, in lowest
/// terms (review 15, R15-M-01: 1000001/3 is one, 1000001/3000003 is 1/3).
fn written_ratio(e: &Expr) -> Option<(rug::Integer, rug::Integer)> {
    fn int(e: &Expr) -> Option<rug::Integer> {
        match e {
            Expr::Num(v, Lit::Exact) if *v == v.trunc() => rug::Integer::from_f64(*v),
            Expr::Num(_, Lit::Decimal(d)) if !d.contains('.') => d.parse().ok(),
            Expr::Neg(a) => int(a).map(|v| -v),
            _ => None,
        }
    }
    let (p, q) = match e {
        Expr::Neg(a) => {
            let (p, q) = written_ratio(a)?;
            return Some((-p, q));
        }
        Expr::Bin(BinOp::Div, a, b) => (int(a)?, int(b)?),
        _ => (int(e)?, rug::Integer::from(1)),
    };
    if q == 0 {
        return None;
    }
    Some(rug::Rational::from((p, q)).into_numer_denom())
}

/// The app's function at an exact point, in MPFR.
fn mp_eval(e: &Expr, x: &Float, unit: TrigUnit, prec: u32) -> Option<Float> {
    let ev = |a: &Expr| mp_eval(a, x, unit, prec);
    Some(match e {
        // The decimal typed, as the tree records it for each number.
        Expr::Num(v, lit) => match lit.digits() {
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
            if let Some((p, q)) = written_ratio(b) {
                return match (p.to_i32(), q.to_i32()) {
                    (Some(p), Some(1)) => mp_powi(&u, p),
                    (Some(p), Some(q)) => mp_powrat(&u, p, q),
                    _ => mp_powratio(&u, &p, &q),
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
                Func::NCr | Func::NPr => mp_ncr(&a0, &ev(&args[1])?, *f == Func::NPr)?,
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
                    return mp_root(&a0, &n, prec);
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
        // min and max are undefined wherever either argument is, even
        // where the other one wins (review 12, R12-M-02).
        ("min((x+1)^2,2+sqrt(sin(x)))", Radians),
        ("max(-(x+1)^2,-2-sqrt(sin(x)))", Radians),
        // A root of varying degree: at x = 2 its degree is 2, but its
        // derivative is x^(1/x)'s (review 12, R12-L-02).
        ("root(x,x)", Radians),
        // Literals are the decimals typed (review 12, R12-M-01): no
        // integer power, and 1.0000000000000001 − cos x never 0.
        ("x^1.0000000000000001", Radians),
        ("2/(1.0000000000000001-cos(x))", Radians),
        // A degree odd as typed, though both doubles about it are even
        // (review 13, R13-M-04), and ones that aren't odd as typed.
        ("root(x,9007199254740993)", Radians),
        ("root(x,-9007199254740993)", Radians),
        (
            "root(x,18014398509481986)+root(x,3.0000000000000001)",
            Radians,
        ),
        // Nowhere defined (√ of −10⁻³⁰), though the exponent's or the
        // degree's enclosure is a whole number: possibly undefined, not
        // an integer power's or root's value alone (review 13, R13-M-02).
        ("x^floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-30)))", Radians),
        (
            "root(x,1+floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-30))))",
            Radians,
        ),
        (
            "2+0*root(x,2+floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-30))))",
            Radians,
        ),
        // A value constant wherever defined, at a point with no
        // neighbourhood (review 16, R16-L-01): just left of 2, ⌊x − 1⌋⁰ is
        // 0⁰ and nCr(⌊x − 2⌋, 0) at Γ's pole, and √(x − 2) ends at 2; and
        // the winners of min and max beside such losers. (At the end of
        // the list: the cases before keep their random boxes.)
        ("floor(x-1)^0", Radians),
        ("min(x,floor(x-1)^0+100)", Radians),
        ("max(x,-floor(x-1)^0-100)", Radians),
        ("nCr(floor(x-2),0)+nPr(floor(x-2),0)", Radians),
        ("min(x,nCr(floor(x-2),0)+100)", Radians),
        ("max(x,-nCr(floor(x-2),0)-100)", Radians),
        ("(sqrt(x-2)+1)^0+floor(sqrt(x-2)+0.5)", Radians),
        ("min(x,floor(sqrt(x-2)+0.5)+5)", Radians),
        // ⌊x − 1⌋ + 0.5 jumps at 2 where the outer floor doesn't.
        ("2*floor(floor(x-1)+0.5)", Radians),
        ("min(x,2*floor(floor(x-1)+0.5)-0.5)", Radians),
        // Controls: the operand clear beside the box (a kink, not a jump).
        (
            "(abs(x-2)+1)^0+floor(abs(x-2)+0.5)+nCr(abs(x-2),0)",
            Radians,
        ),
    ]
}

/// f at ξ, and its first three derivatives by central differences in MPFR
/// at 512 bits (errors far below a double's ulp).
fn mp_derivs(e: &Expr, xi: &Float, unit: TrigUnit) -> Option<[Float; 4]> {
    let pr = 1024;
    PREC.set(pr);
    let r = mp_derivs_at(e, xi, unit, pr);
    PREC.set(P);
    r
}

fn mp_derivs_at(e: &Expr, xi: &Float, unit: TrigUnit, pr: u32) -> Option<[Float; 4]> {
    // Every evaluation at the full precision (an MPFR function keeps its
    // argument's).
    let xi = &Float::with_val(pr, xi);
    let f = |dx: f64, h: &Float| -> Option<Float> {
        let at = Float::with_val(pr, xi + Float::with_val(pr, h * dx));
        mp_eval(e, &at, unit, pr)
    };
    // Absolute steps (the features here are far wider), at a precision
    // where x + h is exact for any double x.
    let h1 = Float::with_val(pr, 2f64.powi(-120));
    let h2 = Float::with_val(pr, 2f64.powi(-100));
    let h3 = Float::with_val(pr, 2f64.powi(-90));
    let f0 = mp_eval(e, xi, unit, pr)?;
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
        let opts = CompileOptions {
            trig_unit: unit,
            ..CompileOptions::default()
        };
        let ctx = Ctx::new(opts);
        let mut bxs: Vec<Interval> = Vec::new();
        for &c in &[
            0.3,
            1.7,
            2.0,
            -1.0,
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
                let value = mp_eval(ast, &xi, unit, P);
                match &value {
                    Some(v) => {
                        if !inside(v, s[0].iv) {
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
                if !ok_d {
                    continue;
                }
                let Some(d) = mp_derivs(ast, &xi, unit) else {
                    // Defined at ξ but not within 2⁻⁸⁹ of it: no
                    // derivatives there to claim (review 16, R16-L-01:
                    // ⌊x⌋⁰ at 1, 0⁰ just left of it).
                    if value.is_some() {
                        t.fail(format!(
                            "taylor {src} [{:e},{:e}] claims derivatives at {}, \
                             where f is undefined beside it",
                            b.lo(),
                            b.hi(),
                            xi.to_string_radix(10, Some(20))
                        ));
                    }
                    continue;
                };
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
    let ctx = Ctx::new(CompileOptions::default());
    let r = graphing::interval::enclose(ast, Interval::point(1000000000000.0625), &ctx);
    assert!(r.is_empty() && r.dec <= Dec::Trv, "{r:?}");
    let r = graphing::interval::enclose(ast, Interval::point(1000000000000.0), &ctx);
    assert_eq!(r.iv, Interval::point(0.0));
}

/// root(x, x) at 2: degree 2 there, but varying: f′(2) is x^(1/x)'s,
/// 2^(1/2)(1 − ln 2)/4 ≈ 0.10849, not the square root's 0.35355 (review
/// 12, R12-L-02).
#[test]
fn a_varying_degree_is_no_constant_root() {
    let text = "y=root(x,x)";
    let eq = Equation::parse(text).unwrap();
    let (_, ast) = eq.explicit().unwrap();
    let ctx = Ctx::new(CompileOptions::default());
    let s = taylor(ast, Interval::point(2.0), 3, &ctx);
    let two = Float::with_val(P, 2u32);
    let f = Float::with_val(P, two.clone().sqrt());
    let d1 = Float::with_val(P, &f * Float::with_val(P, 1u32 - two.clone().ln())) / 4u32;
    assert!(inside(&f, s[0].iv) && derivs_valid(&s, 3), "{s:?}");
    assert!(inside(&d1, s[1].iv), "f′ {:?}, not {}", s[1], d1.to_f64());
    // And the square root itself keeps its series.
    let text = "y=root(x,2)";
    let eq = Equation::parse(text).unwrap();
    let (_, ast) = eq.explicit().unwrap();
    let s = taylor(ast, Interval::point(2.0), 3, &ctx);
    assert!(derivs_valid(&s, 3));
    let half = Float::with_val(P, f.clone().recip()) / 2u32;
    assert!(inside(&half, s[1].iv), "{s:?}");
}

/// min and max where one argument decides the value and the other is
/// undefined (review 12, R12-M-02): undefined, not the winner's series.
#[test]
fn min_max_keep_the_loser_undefined() {
    let series = |src: &str, b: Interval| {
        let text = format!("y={src}");
        let eq = Equation::parse(&text).unwrap();
        let (_, ast) = eq.explicit().unwrap();
        let ctx = Ctx::new(CompileOptions::default());
        taylor(ast, b, 3, &ctx)
    };
    for src in [
        "min((x+1)^2,2+sqrt(sin(x)))",
        "max(-(x+1)^2,-2-sqrt(sin(x)))",
        "min(x,sqrt(x+0.5))",
        "max(x,-sqrt(x+0.5))",
    ] {
        // At −1 the losing argument is undefined; about it, in part.
        for b in [
            Interval::point(-1.0),
            Interval::new(-1.25, -0.75),
            Interval::new(-1.0, -0.25),
        ] {
            let s = series(src, b);
            assert!(
                s[0].dec <= Dec::Trv,
                "{src} on [{}, {}]: {:?}",
                b.lo(),
                b.hi(),
                s[0]
            );
            assert!(!derivs_valid(&s, 1), "{src}: derivatives kept");
        }
        let s = series(src, Interval::point(-1.0));
        assert!(s[0].is_empty(), "{src} at −1: {:?}", s[0]);
    }
    // A loser defined throughout the box changes nothing, kink and all:
    // min(x, x + 1 + |x − 3|) is x there, derivative and all.
    let s = series("min(x,x+1+abs(x-3))", Interval::new(2.5, 3.4));
    assert!(s[0].dec >= Dec::Dac && derivs_valid(&s, 1), "{s:?}");
    assert_eq!(s[1].iv, Interval::point(1.0));
}

/// A value constant wherever it is defined (u⁰, nCr(u, 0), a step), at a
/// point where it is defined but not just left of it (review 16, R16-L-01:
/// ⌊x⌋⁰ at 1 is 0⁰ just left of 1; nCr(⌊x⌋, 0) at 0 is at Γ's pole just
/// left of 0): MPFR finds f defined at the point and undefined 2⁻⁶⁰ left
/// of it, and the interval core gives the value with no derivatives, nor
/// does min or max beside it. Where the operand stays clear beside the
/// box, the derivatives are claimed and MPFR's central differences agree.
#[test]
fn a_value_alone_has_no_derivatives_at_a_domain_end() {
    init();
    let parse = |src: &str| {
        let eq = Equation::parse(&format!("y={src}")).unwrap();
        eq.explicit().unwrap().1.clone()
    };
    let ctx = Ctx::new(CompileOptions::default());
    for (src, x) in [
        ("floor(x)^0", 1.0),
        ("min(x,floor(x)^0+100)", 1.0),
        ("max(x,-floor(x)^0-100)", 1.0),
        ("min(floor(x)^0+100,x)", 1.0),
        ("nCr(floor(x),0)", 0.0),
        ("min(x,nCr(floor(x),0)+100)", 0.0),
        ("max(x,-nCr(floor(x),0)-100)", 0.0),
        ("min(nCr(floor(x),0)+100,x)", 0.0),
        ("nPr(floor(x),0)", 0.0),
        ("max(x,-nPr(floor(x),0)-100)", 0.0),
        ("(sqrt(x)+1)^0", 0.0),
        ("floor(x)^(1-1)", 1.0),
        ("floor(sqrt(x)+0.5)", 0.0),
        ("min(x,floor(sqrt(x)+0.5)+5)", 0.0),
        ("max(x,-round(sqrt(x))-5)", 0.0),
    ] {
        let e = parse(src);
        let at = mp_eval(&e, &mp(x), TrigUnit::Radians, P);
        let left = Float::with_val(P, mp(x) - Float::with_val(P, 2f64.powi(-60)));
        assert!(at.is_some(), "{src} undefined at {x}");
        assert!(
            mp_eval(&e, &left, TrigUnit::Radians, P).is_none(),
            "{src} defined left of {x}"
        );
        let s = taylor(&e, Interval::point(x), 3, &ctx);
        assert!(
            s[0].dec >= Dec::Dac && inside(&at.unwrap(), s[0].iv),
            "{src} at {x}: {:?}",
            s[0]
        );
        assert!(!derivs_valid(&s, 1), "{src} at {x}: derivatives {s:?}");
    }
    for (src, x) in [
        ("floor(x)^0", 1.5),
        ("min(x,floor(x)^0+100)", 1.5),
        ("max(x,-floor(x)^0-100)", 1.5),
        ("nCr(floor(x),0)", 0.5),
        ("min(x,nCr(floor(x),0)+100)", 0.5),
        ("max(x,-nCr(floor(x),0)-100)", 0.5),
        ("min(x,(abs(x)+1)^0+100)", 0.0),
        ("max(x,-floor(abs(x)+0.5)-5)", 0.0),
        ("min(x,nCr(abs(x),0)+100)", 0.0),
        ("floor(sqrt(x)+0.5)", 1.0),
    ] {
        let e = parse(src);
        let s = taylor(&e, Interval::point(x), 3, &ctx);
        assert!(derivs_valid(&s, 3), "{src} at {x}: {s:?}");
        let d = mp_derivs(&e, &mp(x), TrigUnit::Radians).expect("defined beside");
        for k in 1..=3 {
            assert!(inside_fd(&d[k], s[k].iv, &d[0]), "{src} at {x}: c{k} {s:?}");
        }
    }
}

/// root(x, n) where n is beyond 2⁵³, every double there even (review 13,
/// R13-M-04): a box of degrees holds the odd integers between its doubles,
/// so a negative x is possibly defined there, never proven undefined; a
/// point degree is that even double; and a degree typed as an odd integer
/// is odd, whatever doubles enclose it.
#[test]
fn large_root_degrees_keep_their_odd_integers() {
    init();
    const BIG: f64 = 9007199254740992.0;
    let pt = DecInterval::point;
    let truth = |x: f64, n: &rug::Integer| -> Float {
        // (10³⁰⁰ + 1 needs about a thousand bits.)
        let n = Float::with_val(2048, n);
        mp_root(&mp(x), &n, P).expect("an odd degree")
    };
    let big = |s: &str| s.parse::<rug::Integer>().unwrap();
    // Boxes, each with an odd integer inside.
    for (lo, hi, odd) in [
        (BIG, BIG + 2.0, big("9007199254740993")),
        (BIG - 1.0, BIG, big("9007199254740991")),
        (BIG - 2.0, BIG, big("9007199254740991")),
        (-BIG - 2.0, -BIG, big("-9007199254740993")),
        (2.0 * BIG, 2.0 * BIG + 4.0, big("18014398509481985")),
        (
            1e300,
            1e300f64.next_up(),
            rug::Integer::from_f64(1e300).unwrap() + 1u32,
        ),
    ] {
        for x in [-8.0, -0.5, -1e-300] {
            let r = elem::root(&pt(x), &DecInterval::new(Interval::new(lo, hi)));
            let v = truth(x, &odd);
            assert!(
                !r.is_empty() && inside(&v, r.iv),
                "root({x}, [{lo}, {hi}]) = {r:?} misses {} at the odd {odd}",
                v.to_f64()
            );
            // Possibly defined only: the box's other numbers aren't odd.
            assert!(r.dec <= Dec::Trv, "root({x}, [{lo}, {hi}]): {r:?}");
        }
    }
    let r = elem::root(&pt(-8.0), &DecInterval::new(Interval::new(-INF, -BIG)));
    assert!(!r.is_empty() && r.dec <= Dec::Trv, "{r:?}");
    // A point degree is that double: even beyond 2⁵³, odd below it.
    assert!(elem::root(&pt(-8.0), &pt(BIG)).is_empty());
    assert!(elem::root(&pt(-8.0), &pt(2.0 * BIG + 4.0)).is_empty());
    let r = elem::root(&pt(-8.0), &pt(BIG - 1.0));
    assert!(r.dec == Dec::Com && inside(&truth(-8.0, &big("9007199254740991")), r.iv));
    let r = elem::root(&pt(-8.0), &pt(1000001.0));
    assert!(r.dec == Dec::Com && inside(&truth(-8.0, &big("1000001")), r.iv));
    // Degrees as typed, through the Taylor evaluator.
    let at = |src: &str, x: f64| {
        let text = format!("y={src}");
        let eq = Equation::parse(&text).unwrap();
        let (_, ast) = eq.explicit().unwrap();
        let ctx = Ctx::new(CompileOptions::default());
        taylor(ast, Interval::point(x), 3, &ctx)
    };
    for (src, n) in [
        ("root(x,9007199254740993)", "9007199254740993"),
        ("root(x,-9007199254740993)", "-9007199254740993"),
        ("root(x,18014398509481987)", "18014398509481987"),
        ("root(x,1000001)", "1000001"),
    ] {
        for x in [-8.0, -1.5, -1e-300, 2.0] {
            let s = at(src, x);
            let v = truth(x, &big(n));
            assert!(
                s[0].dec == Dec::Com && inside(&v, s[0].iv),
                "{src} at {x}: {:?} misses {}",
                s[0],
                v.to_f64()
            );
        }
    }
    let s = at("root(-8,9007199254740993)", 0.0);
    let v = truth(-8.0, &big("9007199254740993"));
    assert!(s[0].dec == Dec::Com && inside(&v, s[0].iv), "{:?}", s[0]);
    // Even, or no integer, as typed: a negative x is undefined.
    for src in [
        "root(x,9007199254740994)",
        "root(x,18014398509481986)",
        "root(x,3.0000000000000001)",
        "root(x,-18014398509481986)",
    ] {
        let s = at(src, -8.0);
        assert!(s[0].is_empty(), "{src} at −8: {:?}", s[0]);
        let s = at(src, 8.0);
        assert!(s[0].dec == Dec::Com, "{src} at 8: {:?}", s[0]);
    }
}

/// root(x, n) over degree boxes far below −10³⁰⁰ (review 14, R14-M-04):
/// the odd-integer test clipped a box's lower end to −10³⁰⁰, which put
/// its first candidate above [−10³⁰², −10³⁰¹], and root(−8, n) over the
/// box (each of its parts too) was empty. Every box there wider than a
/// point holds odd integers, as does one reaching −∞; and a degree typed
/// as the odd −(10³⁰¹ + 1) is odd, whatever its enclosure.
#[test]
fn far_negative_root_degree_boxes_keep_their_odd_integers() {
    init();
    let pt = DecInterval::point;
    let truth = |x: f64, n: &rug::Integer| -> Float {
        let n = Float::with_val(4096, n);
        mp_root(&mp(x), &n, P).expect("an odd degree")
    };
    // An odd integer in [lo, hi] for lo a double past 2⁵³ (an even
    // integer) below hi: lo + 1.
    let odd_in = |lo: f64| -> rug::Integer { rug::Integer::from_f64(lo).unwrap() + 1u32 };
    let ctx = Ctx::new(CompileOptions::default());
    let eq = Equation::parse("y=root(-8,x)").unwrap();
    let (_, root_of_x) = eq.explicit().unwrap();
    for (lo, hi) in [
        (-1e302, -1e301),
        (-1e308, -1e307),
        (-f64::MAX, -1e300),
        (-1e301, -1e300),
        ((-1e300f64).next_down().next_down(), -1e300),
        (1e301, 1e302),
        (1e307, f64::MAX),
    ] {
        for x in [-8.0, -0.5, -1e-300] {
            let r = elem::root(&pt(x), &DecInterval::new(Interval::new(lo, hi)));
            let v = truth(x, &odd_in(lo));
            assert!(
                !r.is_empty() && inside(&v, r.iv),
                "root({x}, [{lo:e}, {hi:e}]) = {r:?} misses {}",
                v.to_f64()
            );
            assert!(r.dec <= Dec::Trv, "root({x}, [{lo:e}, {hi:e}]): {r:?}");
        }
        // Through the Taylor evaluator, the degree varying over the box,
        // whole and in ten parts.
        let parts = (0..10).map(|i| {
            let at = |k: f64| lo + (hi - lo) / 10.0 * k;
            (at(i as f64), if i == 9 { hi } else { at(i as f64 + 1.0) })
        });
        // (A part that is a point is that even double: empty.)
        for (a, b) in std::iter::once((lo, hi))
            .chain(parts)
            .filter(|(a, b)| a < b)
        {
            let s = taylor(root_of_x, Interval::new(a, b), 1, &ctx);
            let v = truth(-8.0, &odd_in(a));
            assert!(
                !s[0].is_empty() && inside(&v, s[0].iv) && s[0].dec <= Dec::Trv,
                "root(-8, x) on [{a:e}, {b:e}]: {:?}",
                s[0]
            );
        }
    }
    // Boxes reaching −∞, and the whole line.
    for (lo, hi) in [
        (-INF, -1e300),
        (-INF, -1e302),
        (-INF, INF),
        (-INF, -f64::MAX),
    ] {
        let r = elem::root(&pt(-8.0), &DecInterval::new(Interval::new(lo, hi)));
        assert!(
            !r.is_empty() && r.dec <= Dec::Trv,
            "[{lo:e}, {hi:e}]: {r:?}"
        );
    }
    // A point degree is that (even) double.
    assert!(elem::root(&pt(-8.0), &pt(-1e302)).is_empty());
    assert!(elem::root(&pt(-8.0), &pt(-f64::MAX)).is_empty());
    // A degree typed as the odd −(10³⁰¹ + 1), and its even neighbour.
    let odd = -(rug::Integer::from(rug::Integer::u_pow_u(10, 301)) + 1u32);
    let at = |src: &str, x: f64| {
        let text = format!("y={src}");
        let eq = Equation::parse(&text).unwrap();
        let (_, ast) = eq.explicit().unwrap();
        taylor(ast, Interval::point(x), 3, &ctx)
    };
    let odd_src = format!("root(x,-1{}1)", "0".repeat(300));
    let even_src = format!("root(x,-1{}2)", "0".repeat(300));
    for x in [-8.0, -1.5, -1e-300, 2.0] {
        let s = at(&odd_src, x);
        let v = truth(x, &odd);
        assert!(
            s[0].dec == Dec::Com && inside(&v, s[0].iv),
            "root(x, −(10³⁰¹ + 1)) at {x}: {:?} misses {}",
            s[0],
            v.to_f64()
        );
        let s = at(&even_src, x);
        if x < 0.0 {
            assert!(s[0].is_empty(), "an even degree at {x}: {:?}", s[0]);
        } else {
            assert!(s[0].dec == Dec::Com, "an even degree at {x}: {:?}", s[0]);
        }
    }
    let s = at(&format!("root(-8,-1{}1)", "0".repeat(300)), 0.0);
    let v = truth(-8.0, &odd);
    assert!(
        s[0].dec == Dec::Com && inside(&v, s[0].iv) && s[0].hi() < 0.0,
        "{:?}",
        s[0]
    );
}

/// Exponents written as ratios of integers of 10⁶ or more (review 15,
/// R15-M-01): the Taylor evaluator's enclosures hold the real root's power
/// sign(x)ᵖ·|x|^(p/q), and its derivative, at points and over boxes on
/// both sides of 0 and of ±1, at every digit limit (to 5 digits 1000001
/// is the even 1000000); an even denominator leaves x < 0 undefined; and
/// a ratio too long to carry is left any value for x ≤ 0, possibly
/// undefined, never claimed undefined.
#[test]
fn written_ratios_of_any_size_are_enclosed() {
    init();
    use graphing::lexer::ParseOptions;
    let one_minus = 1.0f64.next_down();
    let one_plus = 1.0f64.next_up();
    let points = [
        -2.0, -one_plus, -1.0, -one_minus, -0.5, -1e-300, 0.0, 1e-300, 0.5, one_minus, 1.0,
        one_plus, 2.0,
    ];
    let boxes = [
        (-2.0, -0.5),
        (-one_plus, -one_minus),
        (-1.0, 1.0),
        (-0.5, 0.0),
        (0.0, 0.5),
        (0.5, 2.0),
        (one_minus, one_plus),
    ];
    for digits in [Some(5), Some(14), Some(20), None] {
        for src in [
            "x^(1000001/3)",
            "x^(1/1000001)",
            "x^(-1000001/3)",
            "x^(2000002/3)",
            "x^(1000001/3000003)",
            "x^(-1/1000002)",
            "x^(3/2000000)",
            "x^(1/9007199254740993)",
            "(x-1)^(1000001/3)",
            // Whole numbers of 10⁶ or more, q = 1.
            "x^1000001",
            "x^(-1000002)",
            "x^2000000",
            "x^(-1000001)",
            "x^9007199254740993",
        ] {
            let opts = ParseOptions {
                literal_digits: digits,
                ..Default::default()
            };
            let eq = Equation::parse_with(&format!("y={src}"), opts).unwrap();
            let (_, f) = eq.explicit().unwrap();
            let ctx = Ctx::new(CompileOptions::default());
            let truth = |x: f64| mp_eval(f, &mp(x), TrigUnit::Radians, P);
            for x in points {
                let s = taylor(f, Interval::point(x), 1, &ctx);
                match truth(x) {
                    Some(v) => {
                        assert!(
                            inside(&v, s[0].iv) && s[0].dec >= Dec::Def,
                            "{src} ({digits:?}) at {x:e}: {:?} misses {}",
                            s[0],
                            v.to_f64()
                        );
                        // f′ = (p/q)·f/x, where it is valid.
                        if derivs_valid(&s, 1) && x != 0.0 {
                            let (p, q) = written_ratio(match f {
                                Expr::Bin(BinOp::Pow, _, b) => b,
                                _ => unreachable!(),
                            })
                            .unwrap();
                            let r = Float::with_val(P, rug::Rational::from((p, q)));
                            let base = mp_eval(
                                match f {
                                    Expr::Bin(BinOp::Pow, a, _) => a,
                                    _ => unreachable!(),
                                },
                                &mp(x),
                                TrigUnit::Radians,
                                P,
                            )
                            .unwrap();
                            if !base.is_zero() {
                                let d = Float::with_val(P, r * &v) / base;
                                assert!(
                                    inside(&d, s[1].iv),
                                    "{src} ({digits:?}) f′ at {x:e}: {:?} misses {}",
                                    s[1],
                                    d.to_f64()
                                );
                            }
                        }
                    }
                    None => assert!(
                        s[0].is_empty() || s[0].dec <= Dec::Trv,
                        "{src} ({digits:?}) at {x:e}: {:?}, undefined",
                        s[0]
                    ),
                }
            }
            for (lo, hi) in boxes {
                let s = taylor(f, Interval::new(lo, hi), 1, &ctx);
                for x in [lo, hi, (lo + hi) / 2.0] {
                    if let Some(v) = truth(x) {
                        assert!(
                            inside(&v, s[0].iv),
                            "{src} ({digits:?}) on [{lo:e}, {hi:e}] at {x:e}: {:?} misses {}",
                            s[0],
                            v.to_f64()
                        );
                    } else {
                        assert!(
                            s[0].dec <= Dec::Trv,
                            "{src} ({digits:?}) on [{lo:e}, {hi:e}]: {:?}, undefined at {x:e}",
                            s[0]
                        );
                    }
                }
            }
        }
    }
    // Too long to carry (an odd numerator of 5000 digits over 3: no typed
    // equation is that long, but a tree may be): at −1 its value is −1,
    // which the enclosure holds, possibly undefined.
    let digits = format!("1{}1", "0".repeat(4998));
    let long = Expr::Num(f64::INFINITY, Lit::Decimal(digits.as_str().into()));
    let f = Expr::Bin(
        BinOp::Pow,
        Box::new(Expr::X),
        Box::new(Expr::Bin(
            BinOp::Div,
            Box::new(long),
            Box::new(Expr::Num(3.0, Lit::Exact)),
        )),
    );
    let f = &f;
    let ctx = Ctx::new(CompileOptions::default());
    for (lo, hi) in [(-1.0, -1.0), (-2.0, -0.5), (-1.0, 1.0)] {
        let s = taylor(f, Interval::new(lo, hi), 1, &ctx);
        assert!(
            s[0].dec <= Dec::Trv && inside(&mp(-1.0), s[0].iv) && inside(&mp(1.0), s[0].iv),
            "on [{lo:e}, {hi:e}]: {:?}",
            s[0]
        );
        assert!(!derivs_valid(&s, 1));
    }
}

/// x^n for an exponent typed as the odd 2⁵³ + 1, which its enclosure
/// (2⁵³ and the double after it, both even) can't tell from its even
/// neighbours (review 14, R14-M-02): the Taylor evaluator's enclosure
/// holds the odd power, sign(x)·|x|^n, at points and over boxes on both
/// sides of 0 and of ±1 (on the negative side it abstains: possibly
/// undefined, any value), and claims nothing it can't.
#[test]
fn an_odd_exponent_past_the_doubles_is_enclosed() {
    init();
    let n = rug::Integer::from(9007199254740993u64);
    let truth = |x: &Float| -> Float {
        let m = Float::with_val(P, x.clone().abs()).pow(&n);
        if *x < 0 { -m } else { m }
    };
    let eq = Equation::parse("y=x^9007199254740993").unwrap();
    let (_, f) = eq.explicit().unwrap();
    let ctx = Ctx::new(CompileOptions::default());
    let one_minus = 1.0f64.next_down();
    let one_plus = 1.0f64.next_up();
    let points = [
        -2.0, -one_plus, -1.0, -one_minus, -0.5, -1e-300, 0.0, 1e-300, 0.5, one_minus, 1.0,
        one_plus, 2.0,
    ];
    for x in points {
        let s = taylor(f, Interval::point(x), 1, &ctx);
        let v = truth(&mp(x));
        assert!(
            inside(&v, s[0].iv),
            "at {x:e}: {:?} misses {}",
            s[0],
            v.to_f64()
        );
    }
    for (lo, hi) in [
        (-2.0, -0.5),
        (-one_plus, -one_minus),
        (-1.0, 1.0),
        (0.5, 2.0),
        (one_minus, one_plus),
    ] {
        let s = taylor(f, Interval::new(lo, hi), 1, &ctx);
        for x in [lo, hi, (lo + hi) / 2.0] {
            let v = truth(&mp(x));
            assert!(
                inside(&v, s[0].iv),
                "on [{lo:e}, {hi:e}] at {x:e}: {:?} misses {}",
                s[0],
                v.to_f64()
            );
        }
    }
}
