//! An independent consistency check of what function analysis shows (not
//! run in CI).
//!
//! `cargo run --release -p graphing --example oracle [-- FILTER] [--all]`
//! checks the pool (or the functions whose text contains FILTER) and exits
//! with status 1 if it finds a contradiction. `-- --dump EXPR` prints one
//! function's analysis and findings, with the same exit status. `--
//! --selftest` plants known false answers into real analyses and exits
//! with status 1 unless every check catches its own.
//!
//! The sweep (`examples/sweep.rs`) runs the runtime gate's own checks, so a
//! clean sweep can't vouch for those checks. This one shares none of them:
//! it reads only the public analysis result (what the panel shows, and the
//! numbers behind it), the panel's formatter (`format_nonzero`), the
//! compiled program, and the reference evaluator
//! `analysis::truth::reval_typed` for *values* (a double with an unbounded
//! exponent where an intermediate leaves the doubles), both reading the
//! literals as the decimals typed, as the app does. Nothing from
//! `analysis::verify` is used: no noise estimate, rounding bound, sample
//! set, rule or tolerance of the gate's. What it does share with the app is
//! arithmetic: CORE-MATH's functions (`crates/crmath`), `dd.rs` under both
//! evaluators, and the exact folding of literal arithmetic (`big.rs`,
//! exact rationals rounded once). A wrong primitive
//! both use agrees with itself, so the pool also compares equivalent
//! spellings (below).
//!
//! # What is checked
//!
//! Every *definite* answer, against f sampled densely near 0 (steps of
//! 1/64 to ±10, 1/4 to ±100, integers to ±2000), log-spaced (24 a decade)
//! from 10⁻⁸ out to 10¹⁵, around the value of every x-free subtree of the
//! expression and its negative (at most 64, each with ±1 … 2²⁰ floats,
//! 10⁻¹² … 0.5 relative and 0.01 … 100 absolute steps, and a 1/16 grid
//! over ±8), and around every reported feature (at most 256). A
//! contradiction is reported by class:
//!
//! * the y-intercept, range (a sample outside it), domain (an undefined
//!   sample inside it or a defined one outside), each claimed isolated
//!   exclusion (f defined there and around it, changing by less than 10⁻³
//!   of itself out to four resolution steps, and nothing it could be
//!   undefined through — a divisor, a function's argument, the cos/sin of a
//!   tan/sec/cot/csc argument, a log base less 1, a power's base unless the
//!   exponent is a positive whole number — at 0, undefined or changing sign
//!   there), zeros (each reported one, and sign changes or exact zeros
//!   between samples that none matches), extremum values and their
//!   locality, turns between samples that no reported extremum matches,
//!   parity (a witness against Odd or Even), the period (f(x + P) against
//!   f(x)), monotonicity (a reversal inside a claimed interval), vertical
//!   asymptotes (a pole between samples, or an undefined sample with
//!   growth beside it, that none matches);
//! * one to one: two crossings, or two turns of a kind, both matched to one
//!   reported point and told apart by something resolvably different
//!   between them (f away from 0; f below or above both);
//! * the shapes the panel writes, comparing no digits with anything: a
//!   range written as one value ({c}) needs f the same at every sample,
//!   within their rounding; two elements of a set (excluded points or
//!   families), or the two bounds of an interval (a monotone piece of
//!   each period too), that read alike are a contradiction whatever the
//!   numbers behind them; and across a row's pieces, two adjacent ones
//!   sharing a closed end that reads alike, or ends that read alike about
//!   a gap the panel's own numbers leave where f is undefined at a sample
//!   inside it (a domain's, or the monotone pieces' about it);
//! * an open range bound reached at a turn, to within f's tolerance;
//! * horizontal and oblique asymptotes: |f − line| no smaller at all three
//!   decades further out (the largest over nine points a decade: 10¹² and
//!   10¹⁵ for a horizontal line, 10⁶ and 10⁹ for an oblique one, or from
//!   100× the expression's largest number if further), beyond tolerance and
//!   moving the line as the panel shows it;
//! * inflection points: f's value there, and the second difference at
//!   x ∓ 2h keeping its sign across x at every resolvable scale (at least
//!   three; h = 10⁻¹⁵ … 10³ and 10⁻⁶ … 10⁻² of |x|, at least 1000 floats,
//!   at most a quarter of the way to another reported inflection;
//!   resolvable means beyond 64× the rounding bounds of the three values);
//! * the period's being the least: f(x + P/2) or f(x + P/3) equal to f(x)
//!   within tolerance at every one of at least 200 samples (|x| ≤ 100) over
//!   which f varies by 10⁶ times that tolerance;
//! * the evaluators: the compiled value and the reference's within 8 ulp
//!   of the reference rounded to a double (subnormals by their own
//!   spacing); 0 against a nonzero double, opposite signs, undefined
//!   against defined and a finite double against a value beyond the
//!   largest are never within it. Such a point is used for nothing else;
//! * equivalent spellings (`check_pair`): one spelling's compiled value (what
//!   the app plots and analyses) against the other's reference value,
//!   beyond the second's tolerance, 8 ulp and twice the first's rounding
//!   bound, where the panel writes them differently (skipped where the
//!   first's bound can't be computed); and, for spellings equal
//!   everywhere, every pair of definite answers (one function can't have
//!   two; values compared as the panel writes them or to 10⁻⁹, asymptotes
//!   not compared for a constant, whose own line is a convention).
//!
//! Notes, listed but not failures (sampling can neither confirm nor refute
//! them): a closed range bound no sample, turn or reported value comes to;
//! "neither" parity with no sample against even or odd (beyond the doubles
//! compared exactly; a one-sided undefined point counts); "not periodic"
//! for an f that keeps a circular period at every sample; and how many
//! samples the reference can't evaluate (Unknown: nothing there is checked,
//! and the compiled value is not taken for the truth).
//!
//! # Tolerances, all of them
//!
//! * `tol(x)`: 4 ulp of f(x), plus twice a first-order bound on the
//!   rounding of each of f's operations at x (`F::error`, written here),
//!   plus f's largest change over ±1…4, ±16…64 and ±256…1024 floats and
//!   over two of its resolution steps (`F::res`: how far x must move, up to
//!   2⁴² floats, for f to change; a shifted frame is a staircase). Values
//!   closer than that are the same.
//! * A point within 8 ulp of a reported end, excluded point or pole (plus
//!   10⁻¹² of x and four resolution steps for interval ends) is the
//!   precision of that point, not a claim about it.
//! * Values (range bounds, extremum values, the y-intercept) are only
//!   different if the panel's formatter also writes them differently.
//!   Shapes (domain, zeros, parity, period, monotonicity, asymptotes) get
//!   no such allowance.
//! * A reported zero or zero value holds if f is exactly 0 there, changes
//!   sign within eight resolution steps, or is no bigger than four times
//!   its change over those (a zero between floats), or a crossing within
//!   10⁻⁶ relative is written the same; beside a reported pole, closer
//!   than f resolves, it can't be told. Beyond the doubles this is judged
//!   on the reference's exact sign and magnitude.
//! * A found crossing, turn or pole matches a reported one within 10⁻⁶
//!   relative (the panel's 6 digits), or anywhere between the samples that
//!   bracket it; for one-to-one, the nearest reported point within 10⁻⁶ of
//!   the found position. Crossings in a reported zero's rounding noise (f
//!   within tolerance all the way to it) are not reported missing.
//! * Beyond 10⁷ from 0 and 10⁴ from the expression's numbers, sign
//!   changes and turns are not classified (floats there are too sparse
//!   against sawtooth and plateau features).
//! * Caps: 400 bisected sign changes, 500 undefined samples tested for a
//!   pole, 300 refined turns per kind (nearest the origin first), 4000
//!   samples per period comparison.
//!
//! # Known false alarms
//!
//! * `evaluator-disagrees` at `(x/0.001)^(1/3)` and `^(2/3)`, x = 5e-324:
//!   the reference's `pow_rat` on that near-subnormal quotient is about 60
//!   ulp off (the compiled value is right to the last bit, by 80-digit
//!   decimal). Reported so it is seen until the reference is fixed.
//! * In principle: a part of f below the doubles' resolution with a longer
//!   period (`period-not-fundamental`), a residual approaching a horizontal
//!   line more slowly than any power of log log x (`asymptote-not-approached`),
//!   and an inflection whose sign change only shows below 1000 floats of x.
//!   None occurs in the pool.

use std::cmp::Ordering;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering as AtOrd};

use graphing::analysis::format::{format_decimal, format_nonzero};
use graphing::analysis::truth::{R, Xf, reval, reval_typed};
use graphing::analysis::{
    AnalysisError, Family, Interval, KeyGraphFeatures, Monotonicity, Parity, Periodicity, analyze,
    flags,
};
use graphing::ast::Expr;
use graphing::compile::{CompileOptions, Program};
use graphing::interval::Literals;
use graphing::lexer::ParseOptions;
use graphing::{Equation, TrigUnit};

// ---------------------------------------------------------------- floats

fn ulp(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::INFINITY;
    }
    let a = x.abs();
    let n = f64::from_bits(a.to_bits() + 1);
    if n.is_finite() {
        n - a
    } else {
        a - f64::from_bits(a.to_bits() - 1)
    }
}

/// The float `k` steps from x.
fn step(x: f64, k: i64) -> f64 {
    if !x.is_finite() || k == 0 {
        return x;
    }
    let o = |x: f64| {
        let b = x.to_bits() as i64;
        if b < 0 { -(b & i64::MAX) } else { b }
    };
    let n = o(x).saturating_add(k);
    if n < 0 {
        f64::from_bits(n.unsigned_abs() | (1 << 63))
    } else {
        f64::from_bits(n as u64)
    }
}

/// Exact order of reference values (not through doubles, which saturate).
fn cmp_xf(a: Xf, b: Xf) -> Ordering {
    let (sa, sb) = (a.sign(), b.sign());
    if sa != sb {
        return sa.partial_cmp(&sb).unwrap_or(Ordering::Equal);
    }
    if sa == 0.0 {
        return Ordering::Equal;
    }
    let mag = if a.e != b.e {
        a.e.cmp(&b.e)
    } else {
        a.m.abs().partial_cmp(&b.m.abs()).unwrap_or(Ordering::Equal)
    };
    if sa > 0.0 { mag } else { mag.reverse() }
}

fn fmt(v: f64) -> String {
    format_nonzero(v)
}

/// Two values the panel would show alike: the same closed form or text,
/// or the same six significant digits (an approximate value is written
/// to at most six, its form chosen from the rounded value: 1000000 and
/// 999999.9999982 are both 1×10⁶ there, though 1000000 alone is a
/// closed form).
fn alike(a: f64, b: f64) -> bool {
    fmt(a) == fmt(b) || format_decimal(a) == format_decimal(b)
}

/// A value the doubles hold (0 included): compared absolutely, against
/// tolerances; anything else only relatively, exactly.
fn in_doubles(v: &Val) -> bool {
    !v.sat && v.v.is_finite() && (v.xf.is_zero() || v.v.abs() >= f64::MIN_POSITIVE)
}

/// A value for a report: the double, or m·2^e beyond the doubles.
fn show(v: &Val) -> String {
    match v.k {
        K::Def if v.v.is_finite() && (v.v != 0.0 || v.xf.is_zero()) => format!("{:e}", v.v),
        K::Def if v.sat => format!(
            "{}(nonzero, too small to size)",
            if v.xf.sign() > 0.0 { "+" } else { "-" }
        ),
        K::Def => format!("{}·2^{}", v.xf.m * 2.0, v.xf.e - 1),
        k => format!("{k:?}"),
    }
}

// ---------------------------------------------------------------- the function

#[derive(Clone, Copy, Debug, PartialEq)]
enum K {
    Def,
    Undef,
    /// Neither evaluator can tell.
    Unknown,
    /// The compiled program and the reference disagree.
    Disagree,
}

#[derive(Clone, Copy, Debug)]
struct Val {
    k: K,
    /// The value as a double (±∞ or ±0 beyond the doubles).
    v: f64,
    /// The value exactly (for comparisons beyond the doubles).
    xf: Xf,
    /// The compiled program's value.
    c: f64,
    /// The reference's value is nonzero but too small to size (`Xf::lost`):
    /// its sign holds, its size can't be compared.
    sat: bool,
}

impl Val {
    fn def(&self) -> bool {
        self.k == K::Def
    }
    /// Defined, with a size that can be compared.
    fn cmp_ok(&self) -> bool {
        self.k == K::Def && !self.sat
    }
}

struct F {
    ast: Expr,
    /// The literals as typed: f is what the app computes from them
    /// (`Program::compile_typed`, `truth::reval_typed`).
    lits: Literals,
    prog: Program,
    unit: TrigUnit,
    /// `tol` and `res` by x (each takes up to a hundred evaluations).
    memo: std::cell::RefCell<std::collections::HashMap<(u64, bool), f64>>,
}

impl F {
    fn at(&self, x: f64) -> Val {
        let c = self.prog.eval(x, 0.0);
        let r = reval_typed(&self.ast, x, self.unit, &self.lits);
        let mk = |k, v: f64, xf: Xf| Val {
            k,
            v,
            xf,
            c,
            sat: xf.lost(),
        };
        match r {
            R::V(xf) => {
                let p = xf.f();
                let m = c.abs().max(p.abs());
                if c.is_nan() {
                    // Defined in fact; a double result can't say so only if
                    // the value is beyond the doubles.
                    if p.is_finite() && p.abs() >= f64::MIN_POSITIVE {
                        mk(K::Disagree, p, xf)
                    } else {
                        mk(K::Unknown, p, xf)
                    }
                } else if c.is_infinite() {
                    // ±∞ only for a value beyond the largest double.
                    if p == c || (xf.huge() && xf.sign() == c.signum()) {
                        mk(K::Def, c, xf)
                    } else {
                        mk(K::Disagree, p, xf)
                    }
                } else if (c == 0.0) != (p == 0.0) || c * p < 0.0 || !p.is_finite() {
                    // 0 against a nonzero double, opposite signs, or a finite
                    // double against a value beyond the largest one: never
                    // rounding.
                    mk(K::Disagree, p, xf)
                } else if (c - p).abs() <= 8.0 * ulp(m) {
                    // Within 8 ulp of the reference's value rounded to a
                    // double (subnormals by their own spacing).
                    mk(K::Def, c, xf)
                } else {
                    mk(K::Disagree, p, xf)
                }
            }
            R::Undef => {
                if c.is_nan() {
                    mk(K::Undef, f64::NAN, Xf::ZERO)
                } else {
                    mk(K::Disagree, c, Xf::of(c))
                }
            }
            // The reference can't tell: the point can't be checked (the
            // compiled value is not taken for the truth).
            R::Unknown => mk(K::Unknown, c, Xf::ZERO),
        }
    }

    /// f's resolution at x: how far x must move (doubling from one float,
    /// up to 2⁴² floats) for f to take another value. In a shifted frame,
    /// f(x) = g(x − 10¹²) changes only every so many floats of x.
    fn res(&self, x: f64) -> f64 {
        if let Some(r) = self.memo.borrow().get(&(x.to_bits(), false)) {
            return *r;
        }
        let r = self.res_uncached(x);
        self.memo.borrow_mut().insert((x.to_bits(), false), r);
        r
    }

    fn res_uncached(&self, x: f64) -> f64 {
        let v0 = self.at(x);
        let mut r: f64 = 0.0;
        for s in [-1i64, 1] {
            let mut k: i64 = 1;
            while k <= 1 << 42 {
                let y = step(x, s * k);
                let w = self.at(y);
                if w.k != v0.k || (w.def() && cmp_xf(w.xf, v0.xf) != Ordering::Equal) {
                    r = r.max((y - x).abs());
                    break;
                }
                k *= 2;
            }
        }
        r
    }

    /// A first-order bound on the rounding error of f's value at x: each
    /// operation's own rounding (ε of its result, 2ε for library
    /// functions), plus its operands' errors carried through it (a
    /// function's by its slope there, measured over the error or 10⁻⁸ of
    /// its argument, so a jump within reach counts whole). x and the
    /// literals are exact.
    fn error(&self, e: &Expr, x: f64) -> (f64, f64) {
        use graphing::ast::BinOp;
        let eps = f64::EPSILON;
        let v = match reval_typed(e, x, self.unit, &self.lits) {
            R::V(xf) => xf.f(),
            _ => return (f64::NAN, 0.0),
        };
        if !v.is_finite() {
            return (v, 0.0);
        }
        // ε of the result, but never below the subnormals' spacing.
        let r = v.abs().max(f64::from_bits(1) / eps);
        let er = match e {
            Expr::X | Expr::Num(_) => 0.0,
            Expr::Neg(a) | Expr::Degrees(a) => self.error(a, x).1,
            Expr::Bin(op, a, b) => {
                let ((va, ea), (vb, eb)) = (self.error(a, x), self.error(b, x));
                match op {
                    BinOp::Add | BinOp::Sub => ea + eb + eps * r,
                    BinOp::Mul => va.abs() * eb + vb.abs() * ea + eps * r,
                    BinOp::Div => (ea + r * eb) / vb.abs() + eps * r,
                    BinOp::Pow => {
                        let base = if va == 0.0 {
                            0.0
                        } else {
                            vb.abs() * ea / va.abs()
                        };
                        let expo = if va > 0.0 { va.ln().abs() * eb } else { 0.0 };
                        r * (base + expo) + 2.0 * eps * r
                    }
                }
            }
            Expr::Call(func, args) => {
                let mut total = 2.0 * eps * r;
                for (i, arg) in args.iter().enumerate() {
                    let (va, ea) = self.error(arg, x);
                    if ea == 0.0 || !va.is_finite() {
                        continue;
                    }
                    let h = ea.max(1e-8 * va.abs().max(1.0));
                    let at = |t: f64| {
                        let mut a2 = args.clone();
                        a2[i] = Expr::Num(t);
                        match reval(&Expr::Call(*func, a2), x, self.unit) {
                            R::V(xf) => xf.f(),
                            _ => f64::NAN,
                        }
                    };
                    let slope = ((at(va + h) - at(va - h)) / (2.0 * h)).abs();
                    total += if slope.is_finite() {
                        slope * ea
                    } else {
                        f64::INFINITY
                    };
                }
                total
            }
            _ => eps * r,
        };
        (v, if er.is_nan() { f64::INFINITY } else { er })
    }

    /// The tolerance of f's value at x: 4 ulp, plus twice its rounding
    /// error bound (`error`), plus f's largest change over nearby
    /// floats (the four either side, and 4 at the spacings of 16 and 256
    /// floats: the steps of a shifted frame), plus its change over two of
    /// its resolution steps.
    fn tol(&self, x: f64) -> f64 {
        if let Some(t) = self.memo.borrow().get(&(x.to_bits(), true)) {
            return *t;
        }
        let t = self.tol_uncached(x);
        self.memo.borrow_mut().insert((x.to_bits(), true), t);
        t
    }

    fn tol_uncached(&self, x: f64) -> f64 {
        let v0 = self.at(x);
        if !v0.def() || !v0.v.is_finite() {
            return f64::INFINITY;
        }
        let mut t = 4.0 * ulp(v0.v) + 2.0 * self.error(&self.ast, x).1;
        let mut see = |y: f64| {
            let w = self.at(y);
            if w.def() && w.v.is_finite() {
                t = t.max((w.v - v0.v).abs());
            }
        };
        for j in [0, 4, 8] {
            for i in 1..=4i64 {
                see(step(x, i << j));
                see(step(x, -(i << j)));
            }
        }
        let r = self.res(x);
        for k in [1.0, 2.0] {
            see(x + k * r);
            see(x - k * r);
        }
        t
    }

    /// Whether a reported zero at z holds as the panel shows it: f exactly
    /// 0 there, a sign change within its resolution, |f(z)| no bigger than
    /// 4× its change over that, or a crossing nearby that the panel writes
    /// the same. Beside a reported pole, closer than f can resolve, it can't
    /// be told (`None`).
    fn zero_holds(&self, z: f64, poles: &[Family]) -> Option<bool> {
        let v0 = self.at(z);
        if v0.k == K::Unknown {
            return None;
        }
        if !v0.def() {
            return Some(false);
        }
        if v0.xf.is_zero() {
            return Some(true);
        }
        let r = self.res(z).max(ulp(z)).max(f64::MIN_POSITIVE);
        let reach = 8.0 * r + 1e-12 * z.abs();
        if poles.iter().any(|p| family_in(p, z - reach, z + reach)) {
            return None;
        }
        let mut change = Xf::ZERO;
        for k in [1.0, 2.0, 4.0, 8.0] {
            for y in [z - k * r, z + k * r] {
                let w = self.at(y);
                if !w.def() {
                    continue;
                }
                if w.xf.sign() != v0.xf.sign() {
                    return Some(true);
                }
                let d = w.xf.add(v0.xf.neg()).abs();
                if cmp_xf(d, change) == Ordering::Greater {
                    change = d;
                }
            }
        }
        if !v0.sat && cmp_xf(v0.xf.abs(), change.mul(Xf::of(4.0))) != Ordering::Greater {
            return Some(true);
        }
        // A crossing within the panel's six digits, written the same.
        let w = 1e-6 * z.abs().max(1.0);
        for (a, b) in [(z - w, z), (z, z + w)] {
            if let Some(c) = self.crossing(a, b)
                && alike(c, z)
            {
                return Some(true);
            }
        }
        Some(false)
    }

    /// A sign change or exact zero of f in [a, b], by bisection.
    fn crossing(&self, a: f64, b: f64) -> Option<f64> {
        let (va, vb) = (self.at(a), self.at(b));
        if !(va.def() && vb.def()) {
            return None;
        }
        if va.xf.is_zero() {
            return Some(a);
        }
        if vb.xf.is_zero() {
            return Some(b);
        }
        if va.xf.sign() == vb.xf.sign() {
            return None;
        }
        let (mut l, mut r) = (a, b);
        for _ in 0..2100 {
            let m = l + (r - l) / 2.0;
            if m <= l || m >= r {
                break;
            }
            let v = self.at(m);
            if !v.def() {
                return None;
            }
            if v.xf.is_zero() {
                return Some(m);
            }
            if v.xf.sign() == va.xf.sign() {
                l = m
            } else {
                r = m
            }
        }
        Some(l)
    }
}

// ---------------------------------------------------------------- claims

/// A complete answer: not unknown, and not a partial list (proven items,
/// maybe not all; the certify corpus checks those against the truth).
fn definite(k: &KeyGraphFeatures, flag: u32) -> bool {
    (k.too_complex_features | k.partial_features) & flag == 0
}

/// Members of a family near the origin.
fn members(f: &Family) -> Vec<f64> {
    match f.period {
        Some(p) if p > 0.0 => (-2..=2).map(|k| f.x + k as f64 * p).collect(),
        _ => vec![f.x],
    }
}

/// Whether the family has a member in [a, b] (with the families' own
/// rounding as slack).
fn family_in(f: &Family, a: f64, b: f64) -> bool {
    let slack = |v: f64| 8.0 * ulp(v);
    match f.period {
        Some(p) if p > 0.0 && p.is_finite() => {
            let k = ((a - f.x) / p).ceil();
            let m = f.x + k * p;
            let s = slack(m) + k.abs() * ulp(p) * 2.0;
            m <= b + s || f.x + (k - 1.0) * p >= a - s
        }
        _ => f.x >= a - slack(a) - slack(f.x) && f.x <= b + slack(b) + slack(f.x),
    }
}

fn near(fams: &[Family], x: f64, lo: f64, hi: f64) -> bool {
    member_key(fams, x, lo, hi).is_some()
}

/// Which reported point (family, copy) a feature found at x (between lo and
/// hi) matches, as `near` decides: of those, the one nearest x.
fn member_key(fams: &[Family], x: f64, lo: f64, hi: f64) -> Option<(usize, i64)> {
    let w = 1e-6 * x.abs().max(1.0);
    let mut best: Option<((usize, i64), f64)> = None;
    for (i, f) in fams.iter().enumerate() {
        if !family_in(f, lo.min(x - w), hi.max(x + w)) {
            continue;
        }
        let (k, m) = match f.period {
            Some(p) if p > 0.0 && p.is_finite() => {
                let k = ((x - f.x) / p).round();
                (k as i64, f.x + k * p)
            }
            _ => (0, f.x),
        };
        let dist = (x - m).abs();
        if best.is_none_or(|b| dist < b.1) {
            best = Some(((i, k), dist));
        }
    }
    best.map(|b| b.0)
}

/// What two found features of one kind are told apart by: something
/// resolvably different from both between them.
#[derive(Clone, Copy)]
enum Kind {
    /// Crossings: f resolvably away from 0 between them.
    Zero,
    /// Maxima (minima): f resolvably below (above) both between them.
    Turn(f64),
}

/// Found positions matched to one reported point (`member_key`, by
/// position within the panel's precision): two told apart (`Kind`) are two
/// features where one is reported.
fn merged(f: &F, found: &[((usize, i64), f64)], kind: Kind) -> Vec<(f64, f64)> {
    let mut by: std::collections::BTreeMap<(usize, i64), Vec<f64>> = Default::default();
    for (key, x) in found {
        by.entry(*key).or_default().push(*x);
    }
    let mut out = Vec::new();
    for (_, mut xs) in by {
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        'pairs: for w in xs.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (va, vb) = (f.at(a), f.at(b));
            if !(in_doubles(&va) && in_doubles(&vb)) {
                continue;
            }
            let t = 8.0 * (f.tol(a) + f.tol(b));
            for j in 1..32 {
                let y = a + (b - a) * j as f64 / 32.0;
                let v = f.at(y);
                if !in_doubles(&v) {
                    continue;
                }
                let apart = match kind {
                    Kind::Zero => v.v.abs() > t.max(8.0 * f.tol(y)),
                    Kind::Turn(s) => {
                        s * (va.v.min(vb.v) * (s > 0.0) as u8 as f64
                            + va.v.max(vb.v) * (s < 0.0) as u8 as f64
                            - v.v)
                            > t + 8.0 * f.tol(y)
                    }
                };
                if apart {
                    out.push((a, b));
                    break 'pairs;
                }
            }
        }
    }
    out
}

fn in_interval(i: &Interval, v: f64) -> bool {
    (v > i.lo.value || (i.lo.closed && v == i.lo.value))
        && (v < i.hi.value || (i.hi.closed && v == i.hi.value))
}

/// Where x falls in a set given within one period `[w, w + P)` (or the
/// set itself): x moved into that period, and how far it moved (in
/// periods; its rounding is the slack of any comparison there).
fn reduce(parts: &[Interval], period: Option<f64>, x: f64) -> (f64, f64) {
    let whole = parts.iter().any(|i| i.lo.value == f64::NEG_INFINITY)
        || parts.iter().any(|i| i.hi.value == f64::INFINITY);
    match period {
        Some(p) if p > 0.0 && !whole && parts[0].lo.value.is_finite() => {
            let w = parts[0].lo.value;
            let k = ((x - w) / p).floor();
            (x - k * p, k)
        }
        _ => (x, 0.0),
    }
}

/// Whether x is in the set (given within one period if periodic), and
/// whether it is too close to an end of it to say.
fn member(
    parts: &[Interval],
    period: Option<f64>,
    x: f64,
    extra: &dyn Fn(f64) -> f64,
) -> (bool, bool) {
    let (y, k) = reduce(parts, period, x);
    let p = period.unwrap_or(0.0);
    // 10⁻¹⁵ absolute too: next to 0, x − 1 can't tell 10⁻³²² from 0.
    let slack = 8.0 * ulp(x) + k.abs() * ulp(p) * 4.0 + 8.0 * ulp(y) + 1e-12 * x.abs() + 1e-15;
    let mut close = false;
    let mut inside = false;
    for i in parts {
        for sh in [-p, 0.0, p] {
            let (lo, hi) = (i.lo.value + sh, i.hi.value + sh);
            if (lo.is_finite() && (y - lo).abs() <= slack + 8.0 * ulp(lo) + extra(i.lo.value))
                || (hi.is_finite() && (y - hi).abs() <= slack + 8.0 * ulp(hi) + extra(i.hi.value))
            {
                close = true;
            }
            let mut j = *i;
            j.lo.value = lo;
            j.hi.value = hi;
            inside |= in_interval(&j, y);
            if p == 0.0 {
                break;
            }
        }
    }
    (inside, close)
}

// ---------------------------------------------------------------- checks

/// Whether f is defined just beside y: within four floats, its resolution,
/// or 10⁻¹² or 10⁻⁹ relative (an undefined point where rounding put y, not
/// an undefined stretch).
fn defined_nearby(f: &F, y: f64) -> bool {
    let r = f.res(y);
    let s = y.abs().max(1.0);
    (-4..=4).any(|j| f.at(step(y, j)).def())
        || [r, 2.0 * r, 1e-12 * s, 1e-9 * s]
            .iter()
            .any(|&d| d > 0.0 && (f.at(y - d).def() || f.at(y + d).def()))
}

/// The operands at which f can be undefined at an isolated point (where
/// one is 0): every divisor, every function's argument, the cosine (tan,
/// sec) or sine (cot, csc) of a circular function's argument, a log base
/// less 1, and the base of a power whose exponent isn't a positive whole
/// number (a square is defined where its base is 0).
fn guarded(e: &Expr, out: &mut Vec<Expr>) {
    use graphing::ast::{BinOp, Func};
    if let Expr::Call(func, args) = e
        && let Some(a) = args.first()
    {
        match func {
            Func::Tan | Func::Sec => out.push(Expr::Call(Func::Cos, vec![a.clone()])),
            Func::Cot | Func::Csc => out.push(Expr::Call(Func::Sin, vec![a.clone()])),
            Func::LogBase => out.push(Expr::bin(BinOp::Sub, a.clone(), Expr::Num(1.0))),
            _ => {}
        }
    }
    if !e.contains_x() {
        return;
    }
    match e {
        Expr::Neg(a) | Expr::Degrees(a) => guarded(a, out),
        Expr::Bin(op, a, b) => {
            match op {
                BinOp::Div => out.push((**b).clone()),
                BinOp::Pow => {
                    let whole = matches!(**b, Expr::Num(n) if n >= 1.0 && n.fract() == 0.0);
                    if !whole {
                        out.push((**a).clone());
                    }
                }
                _ => {}
            }
            guarded(a, out);
            guarded(b, out);
        }
        Expr::Call(_, args) => {
            for a in args {
                out.push(a.clone());
                guarded(a, out);
            }
        }
        _ => {}
    }
}

/// Points at and around x: x, the 8 floats either side, and floats at
/// doubling distances out to `n`.
fn around(x: f64, n: i64) -> Vec<f64> {
    let mut v: Vec<f64> = (-8..=8).map(|j| step(x, j)).collect();
    let mut k = 16;
    while k <= n {
        v.push(step(x, k));
        v.push(step(x, -k));
        k *= 2;
    }
    v
}

/// Whether a claimed isolated exclusion at e is contradicted: f is defined
/// at e and all around it, changing smoothly there (by less than 10⁻³ of
/// itself: no pole or jump between floats), and nothing f could be
/// undefined through (`guarded`) is 0, undefined or changes sign there.
/// Around means out to `slack` floats (where e is, given its rounding),
/// and to four of f's resolution steps (a shifted frame changes only
/// every so many floats).
fn exclusion_unsupported(f: &F, e: f64, slack: i64) -> Option<Val> {
    let v = f.at(e);
    if !v.def() || !v.v.is_finite() || v.sat {
        return None;
    }
    let res = (4.0 * f.res(e) / ulp(e).max(f64::MIN_POSITIVE)).min(1e15) as i64;
    let pts = around(e, slack.max(res).max(8));
    for &y in &pts {
        let w = f.at(y);
        if !w.def() || !w.v.is_finite() || w.sat {
            return None;
        }
        if (w.v - v.v).abs() > 1e-3 * w.v.abs().max(v.v.abs()) {
            return None;
        }
    }
    let mut g = Vec::new();
    guarded(&f.ast, &mut g);
    for sub in &g {
        let mut sign = None;
        for &y in &pts {
            match reval_typed(sub, y, f.unit, &f.lits) {
                R::V(xf) if !xf.is_zero() => {
                    let s = xf.sign();
                    if sign.is_some_and(|t| t != s) {
                        return None;
                    }
                    sign = Some(s);
                }
                _ => return None,
            }
        }
    }
    Some(v)
}

struct Finding {
    class: &'static str,
    expr: String,
    detail: String,
    rank: f64,
    /// Not a contradiction: an answer sampling can neither confirm nor
    /// refute, or points the reference can't evaluate (listed, not
    /// failed).
    note: bool,
}

struct Ctx<'a> {
    expr: &'a str,
    out: Vec<Finding>,
}

impl Ctx<'_> {
    fn fail(&mut self, class: &'static str, detail: String) {
        self.fail_at(class, 0.0, detail);
    }

    fn fail_at(&mut self, class: &'static str, x: f64, detail: String) {
        self.push(class, x, detail, false);
    }

    fn note(&mut self, class: &'static str, x: f64, detail: String) {
        self.push(class, x, detail, true);
    }

    /// Keeps the three witnesses per class and function nearest the origin.
    fn push(&mut self, class: &'static str, x: f64, detail: String, note: bool) {
        self.out.push(Finding {
            class,
            expr: self.expr.to_string(),
            detail,
            rank: x.abs(),
            note,
        });
        let mut mine: Vec<usize> = (0..self.out.len())
            .filter(|&i| self.out[i].class == class)
            .collect();
        if mine.len() > 3 {
            mine.sort_by(|&a, &b| self.out[a].rank.total_cmp(&self.out[b].rank));
            let worst = mine[3];
            self.out.remove(worst);
        }
    }
}

/// Every x-free subtree's value.
fn constants(e: &Expr, unit: TrigUnit, out: &mut Vec<f64>) {
    if !e.contains_x()
        && let R::V(xf) = reval(e, 0.0, unit)
    {
        let v = xf.f();
        if v.is_finite() && v != 0.0 {
            out.push(v);
        }
    }
    match e {
        Expr::Neg(a) | Expr::Degrees(a) => constants(a, unit, out),
        Expr::Bin(_, a, b) => {
            constants(a, unit, out);
            constants(b, unit, out);
        }
        Expr::Call(_, args) => args.iter().for_each(|a| constants(a, unit, out)),
        _ => {}
    }
}

fn probes(p: f64, window: bool, xs: &mut Vec<f64>) {
    if !p.is_finite() {
        return;
    }
    xs.push(p);
    for k in [1, 2, 3, 4, 8, 16, 64, 256, 4096, 1 << 20] {
        xs.push(step(p, k));
        xs.push(step(p, -k));
    }
    let s = p.abs().max(1.0);
    for e in -12..=-1 {
        for m in [1.0, 2.5, 5.0] {
            let d = m * 10f64.powi(e) * s;
            xs.push(p + d);
            xs.push(p - d);
        }
    }
    for d in [0.01, 0.0625, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0] {
        xs.push(p + d);
        xs.push(p - d);
    }
    if window {
        for i in -128..=128 {
            xs.push(p + i as f64 / 16.0);
        }
    }
}

fn samples(f: &F, k: &KeyGraphFeatures, extra: &[f64], wide: bool) -> Vec<f64> {
    let mut xs: Vec<f64> = Vec::new();
    for i in -640..=640 {
        xs.push(i as f64 / 64.0);
    }
    for i in -400..=400 {
        xs.push(i as f64 / 4.0);
    }
    for i in -2000..=2000 {
        xs.push(i as f64);
    }
    for e in (-8 * 24)..=(15 * 24) {
        let v = 10f64.powf(e as f64 / 24.0);
        xs.push(v);
        xs.push(-v);
    }
    if wide {
        for i in -40000..=40000 {
            xs.push(i as f64 / 2.0);
        }
    }
    let mut cs = extra.to_vec();
    constants(&f.ast, f.unit, &mut cs);
    cs.sort_by(f64::total_cmp);
    cs.dedup();
    for &c in cs.iter().take(64) {
        probes(c, true, &mut xs);
        probes(-c, true, &mut xs);
    }
    let d = &k.data;
    let mut pts: Vec<f64> = Vec::new();
    for fam in d
        .zeros
        .iter()
        .chain(&d.excluded)
        .chain(&d.vertical_asymptotes)
        .chain(d.minima.iter().map(|m| &m.0))
        .chain(d.maxima.iter().map(|m| &m.0))
        .chain(d.inflection_points.iter().map(|m| &m.0))
    {
        pts.extend(members(fam));
    }
    for i in d.domain.iter().chain(d.monotonicity.iter().map(|m| &m.0)) {
        pts.extend(
            [i.lo.value, i.hi.value]
                .into_iter()
                .filter(|v| v.is_finite()),
        );
    }
    for &p in pts.iter().take(256) {
        probes(p, false, &mut xs);
    }
    xs.retain(|x| x.is_finite() && x.abs() <= 1e15);
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    xs
}

fn check(expr: &str, unit: TrigUnit, extra: &[f64], wide: bool) -> Vec<Finding> {
    check_with(expr, unit, extra, wide, &|_| {})
}

/// `check`, with the analysis changed by `plant` first (the self-test
/// plants known false answers).
fn check_with(
    expr: &str,
    unit: TrigUnit,
    extra: &[f64],
    wide: bool,
    plant: &dyn Fn(&mut KeyGraphFeatures),
) -> Vec<Finding> {
    let mut cx = Ctx {
        expr,
        out: Vec::new(),
    };
    let opts = CompileOptions {
        trig_unit: unit,
        ..CompileOptions::default()
    };
    let Ok(eq) = Equation::parse(&format!("y={expr}")) else {
        cx.fail("does-not-parse", String::new());
        return cx.out;
    };
    let Some((_, ast)) = eq.explicit() else {
        return cx.out;
    };
    let lits = Literals::of(&format!("y={expr}"), ParseOptions::default()).unwrap_or_default();
    let Ok(prog) = Program::compile_typed(ast, &opts, &lits) else {
        return cx.out;
    };
    let f = F {
        ast: ast.clone(),
        lits,
        prog,
        unit,
        memo: Default::default(),
    };
    let mut k = analyze(&eq, &opts);
    plant(&mut k);
    if k.analysis_error != AnalysisError::NoError {
        return cx.out;
    }
    let d = &k.data;
    let xs = samples(&f, &k, extra, wide);
    let mut centres = extra.to_vec();
    constants(&f.ast, f.unit, &mut centres);
    let vals: Vec<Val> = xs.iter().map(|&x| f.at(x)).collect();

    // The evaluators.
    for (x, v) in xs.iter().zip(&vals) {
        if v.k == K::Disagree {
            cx.fail_at(
                "evaluator-disagrees",
                *x,
                format!(
                    "x={x:e}: compiled {:e}, reference {:?}",
                    v.c,
                    reval_typed(&f.ast, *x, unit, &f.lits)
                ),
            );
        }
    }
    // Where the reference can't tell, nothing is checked (the compiled value
    // is not taken for the truth): how many such points.
    let unverifiable = vals.iter().filter(|v| v.k == K::Unknown).count();
    if unverifiable > 0 {
        cx.note(
            "unverifiable-points",
            0.0,
            format!(
                "{unverifiable} of {} samples: the reference can't evaluate f there",
                xs.len()
            ),
        );
    }

    // Y-intercept.
    if definite(&k, flags::Y_INTERCEPT) {
        let v0 = f.at(0.0);
        match (d.y_intercept, v0.k) {
            (None, K::Def) => cx.fail("y-intercept-missing", format!("f(0)={:e}", v0.v)),
            (Some(y), K::Undef) => cx.fail("y-intercept-undefined", format!("claimed {y}")),
            (Some(y), K::Def) => {
                // f(0) is one exact value: 0 only if it is 0, or within its
                // rounding of 0 among the normal doubles.
                let beyond = v0.sat || (v0.v.abs() < f64::MIN_POSITIVE && !v0.xf.is_zero());
                let wrong = if y == 0.0 {
                    !v0.xf.is_zero() && (beyond || v0.v.abs() > f.tol(0.0))
                } else {
                    (y - v0.v).abs() > f.tol(0.0) && !alike(y, v0.v)
                };
                if wrong {
                    cx.fail(
                        if beyond {
                            "y-intercept-underflow-shown-as-0"
                        } else {
                            "y-intercept-wrong"
                        },
                        format!("claimed {} ({y:e}), f(0) = {}", fmt(y), show(&v0)),
                    );
                }
            }
            _ => {}
        }
    }

    // Range.
    if definite(&k, flags::RANGE) && !d.range.is_empty() {
        for (x, v) in xs.iter().zip(&vals) {
            if !v.def() || d.range.iter().any(|i| in_interval(i, v.v)) {
                continue;
            }
            let b = d
                .range
                .iter()
                .flat_map(|i| [i.lo.value, i.hi.value])
                .filter(|b| b.is_finite())
                .min_by(|a, b| (a - v.v).abs().total_cmp(&(b - v.v).abs()));
            let Some(b) = b else { continue };
            if (v.v - b).abs() > f.tol(*x) && !alike(v.v, b) {
                cx.fail_at(
                    "range-excludes-value",
                    *x,
                    format!("{}: f({x:e}) = {} ({:e})", k.range, fmt(v.v), v.v),
                );
            }
        }
    }

    // An end of an interval is known to f's resolution there.
    let mut slacks: Vec<(f64, f64)> = Vec::new();
    for i in d.domain.iter().chain(d.monotonicity.iter().map(|m| &m.0)) {
        for e in [i.lo.value, i.hi.value] {
            if e.is_finite() && !slacks.iter().any(|s| s.0 == e) && slacks.len() < 256 {
                slacks.push((e, 4.0 * f.res(e)));
            }
        }
    }
    let end_slack =
        |e: f64| 1e-12 * e.abs() + slacks.iter().find(|s| s.0 == e).map_or(0.0, |s| s.1);

    // Domain.
    if definite(&k, flags::DOMAIN) && !d.domain.is_empty() {
        for (x, v) in xs.iter().zip(&vals) {
            if !(v.def() || v.k == K::Undef) || (d.period.is_some() && x.abs() > 1e6) {
                continue;
            }
            let (claimed, close) = member(&d.domain, d.period, *x, &end_slack);
            // An excluded point is known to f's resolution there, like an
            // end: 1/(1 + cos x) is undefined in doubles for every x within
            // 2·10⁻⁸ of π, where 1 + cos x touches 0.
            let s = (1e-15 * x.abs().max(1.0)).max(4.0 * f.res(*x));
            if close || d.excluded.iter().any(|fam| family_in(fam, x - s, x + s)) {
                continue;
            }
            if claimed && v.k == K::Undef {
                cx.fail_at(
                    "domain-includes-undefined",
                    *x,
                    format!("{}: f({x:e}) undefined", k.domain),
                );
            } else if !claimed && v.def() {
                cx.fail_at(
                    "domain-excludes-defined",
                    *x,
                    format!("{}: f({x:e}) = {:e}", k.domain, v.v),
                );
            }
        }
        // Each claimed isolated exclusion: f at the point itself (the
        // samples above step around it).
        for fam in &d.excluded {
            let p = fam.period.unwrap_or(0.0);
            for (j, e) in members(fam).into_iter().enumerate() {
                if !e.is_finite() || e.abs() > 1e15 {
                    continue;
                }
                // A copy k periods out is rounded by about k·ulp(P).
                let kp = (j as f64 - 2.0).abs() * if fam.period.is_some() { 1.0 } else { 0.0 };
                let slack = ((kp * 4.0 * ulp(p)) / ulp(e).max(f64::MIN_POSITIVE)).min(1e6) as i64;
                if let Some(v) = exclusion_unsupported(&f, e, slack) {
                    cx.fail_at(
                        "excluded-point-defined",
                        e,
                        format!(
                            "{}: f({e:e}) = {}, defined and bounded around it, nothing undefined there",
                            k.domain,
                            show(&v)
                        ),
                    );
                }
            }
        }
    }

    // Zeros.
    let mut poles: Vec<Family> = d.vertical_asymptotes.clone();
    poles.extend(d.excluded.iter().copied());
    let zero_points =
        definite(&k, flags::ZEROS) && !(d.zeros.is_empty() && !k.x_intercept.is_empty());
    if zero_points {
        for z in &d.zeros {
            for x in members(z) {
                if f.zero_holds(x, &poles) == Some(false) {
                    cx.fail(
                        "zero-not-zero",
                        format!("{}: f({x:e}) = {}", k.x_intercept, show(&f.at(x))),
                    );
                }
            }
        }
    }

    // Extremum values, and whether each is a local one.
    for (flag, list, sign, name) in [
        (flags::MINIMA, &d.minima, 1.0, "minimum"),
        (flags::MAXIMA, &d.maxima, -1.0, "maximum"),
    ] {
        if !definite(&k, flag) {
            continue;
        }
        for (fam, y) in list {
            for x in members(fam) {
                let v = f.at(x);
                if v.k == K::Undef {
                    cx.fail("extremum-undefined", format!("{name} ({x:e}, {y})"));
                    continue;
                }
                if !v.def() {
                    continue;
                }
                let wrong = if *y == 0.0 {
                    f.zero_holds(x, &poles) == Some(false)
                } else {
                    (y - v.v).abs() > f.tol(x) && !alike(*y, v.v)
                };
                if wrong {
                    cx.fail(
                        "extremum-value-wrong",
                        format!("{name} ({x:e}, {}) but f there = {}", fmt(*y), show(&v)),
                    );
                }
                // On each side, at the first distance where f's change can be
                // told from its tolerance: lower there (for a minimum), and at
                // the next two doublings, and where the panel would show
                // another position, means this is no local minimum.
                if v.sat {
                    continue;
                }
                let resolvable = |w: &Val, y: f64| {
                    let normal = in_doubles;
                    if normal(&v) && normal(w) {
                        (w.v - v.v).abs() > 2.0 * (f.tol(x) + f.tol(y))
                    } else {
                        let dif = w.xf.add(v.xf.neg()).abs();
                        cmp_xf(dif, v.xf.abs().mul(Xf::of(1e-9))) == Ordering::Greater
                    }
                };
                let better = |w: &Val| match cmp_xf(w.xf, v.xf) {
                    Ordering::Less => sign > 0.0,
                    Ordering::Greater => sign < 0.0,
                    Ordering::Equal => false,
                };
                for s in [-1.0, 1.0] {
                    let mut dd = 4.0 * ulp(x).max(f64::MIN_POSITIVE);
                    while dd <= 1e-2 * x.abs().max(1.0) {
                        let y = x + s * dd;
                        let w = f.at(y);
                        if !w.cmp_ok() {
                            break;
                        }
                        if !resolvable(&w, y) {
                            dd *= 2.0;
                            continue;
                        }
                        // Only where f is resolved near the claim: a first
                        // resolvable change far out may be another basin.
                        if dd > 1e-6 * x.abs().max(1.0) {
                            break;
                        }
                        let on = [2.0, 4.0].iter().all(|m| {
                            let w2 = f.at(x + s * dd * m);
                            w2.cmp_ok() && better(&w2) && resolvable(&w2, x + s * dd * m)
                        });
                        if better(&w) && on && fmt(x + s * dd * 4.0) != fmt(x) {
                            cx.fail_at(
                                "extremum-not-local",
                                x,
                                format!(
                                    "{name} at {x:e} (f = {}): f({:e}) = {} beyond it",
                                    show(&v),
                                    x + s * dd * 4.0,
                                    show(&f.at(x + s * dd * 4.0))
                                ),
                            );
                        }
                        break;
                    }
                }
            }
        }
    }

    // Parity.
    if definite(&k, flags::PARITY) && matches!(k.parity, Parity::Odd | Parity::Even) {
        let odd = k.parity == Parity::Odd;
        for (x, v) in xs.iter().zip(&vals) {
            if *x <= 0.0 {
                continue;
            }
            let w = f.at(-x);
            if v.sat || w.sat {
                continue;
            }
            if v.def() && w.def() {
                let want = if odd { -v.v } else { v.v };
                let mirror = if odd { v.xf.neg() } else { v.xf };
                let differs = if want.is_finite() && w.v.is_finite() && (want != 0.0 || w.v != 0.0)
                {
                    let dif = (w.v - want).abs();
                    dif > 8.0 * ulp(want.abs().max(w.v.abs())) && dif > f.tol(*x) + f.tol(-x)
                } else {
                    // Beyond the doubles: compare exactly, relatively.
                    let r = w.xf.add(mirror.neg()).abs();
                    !r.is_zero()
                        && cmp_xf(r, w.xf.abs().mul(Xf::of(1e-9))) == Ordering::Greater
                        && cmp_xf(r, mirror.abs().mul(Xf::of(1e-9))) == Ordering::Greater
                };
                if differs {
                    cx.fail_at(
                        "parity-witness",
                        *x,
                        format!(
                            "{:?}: f({x:e}) = {}, f(−x) = {}",
                            k.parity,
                            show(v),
                            show(&w)
                        ),
                    );
                }
            } else if v.def() != w.def()
                && (v.def() || v.k == K::Undef)
                && (w.def() || w.k == K::Undef)
            {
                // Defined on one side only, and not just at a boundary.
                let (u, e) = if v.def() { (-x, *x) } else { (*x, -x) };
                let edge = defined_nearby(&f, u) || (-4..=4).any(|j| !f.at(step(e, j)).def());
                if !edge {
                    cx.fail_at(
                        "parity-witness",
                        *x,
                        format!("{:?}: defined at {e:e} only", k.parity),
                    );
                }
            }
        }
    }

    // Period.
    if definite(&k, flags::PERIODICITY)
        && k.periodicity_direction == Periodicity::Periodic
        && let Some(p) = d.period
    {
        let mut n = 0;
        for (x, v) in xs.iter().zip(&vals) {
            if x.abs() > 1e6 || !v.def() || n > 4000 {
                continue;
            }
            n += 1;
            let y = x + p;
            let w = f.at(y);
            if w.def() {
                let dif = (w.v - v.v).abs();
                // x + P is rounded: f(x) between f at the floats beside it is
                // consistent (at a pole that is a long way).
                let beside: Vec<f64> = (-2..=2)
                    .map(|j| f.at(step(y, j)))
                    .filter(|u| u.def())
                    .map(|u| u.v)
                    .collect();
                let between = beside.iter().any(|&u| u <= v.v) && beside.iter().any(|&u| u >= v.v);
                // A pole within the float spacing at x + P: no float there
                // stands for x + P.
                let (lo, hi) = beside.iter().fold((f64::INFINITY, 0.0f64), |(l, h), &u| {
                    (l.min(u.abs()), h.max(u.abs()))
                });
                let pole = beside.iter().any(|&u| u < 0.0) && beside.iter().any(|&u| u > 0.0)
                    || hi > 1e3 * lo.max(f64::MIN_POSITIVE);
                let between = between || pole;
                if !between
                    && dif > 8.0 * ulp(v.v.abs().max(w.v.abs()))
                    && dif > f.tol(*x) + f.tol(y)
                {
                    cx.fail_at(
                        "period-witness",
                        *x,
                        format!("P={p:e}: f({x:e}) = {:e}, f(x+P) = {:e}", v.v, w.v),
                    );
                }
            } else if w.k == K::Undef && !defined_nearby(&f, y) {
                cx.fail_at(
                    "period-witness",
                    *x,
                    format!("P={p:e}: f({x:e}) defined, f(x+P) not"),
                );
            }
        }
    }

    // Monotonicity.
    if definite(&k, flags::MONOTONE_INTERVALS) && !d.monotonicity.is_empty() {
        let mut breaks: Vec<f64> = Vec::new();
        for fam in d.vertical_asymptotes.iter().chain(&d.excluded) {
            breaks.extend(members(fam));
        }
        for (iv, m) in &d.monotonicity {
            let one = std::slice::from_ref(iv);
            // Consecutive comparable samples inside the same copy of iv.
            let mut prev: Option<(f64, Val, f64)> = None;
            for (x, v) in xs.iter().zip(&vals) {
                if d.period.is_some() && x.abs() > 1e6 {
                    continue;
                }
                let (inside, close) = member(one, d.period, *x, &end_slack);
                let copy = reduce(one, d.period, *x).1;
                if !inside || close {
                    continue;
                }
                if !v.cmp_ok() {
                    prev = None;
                    continue;
                }
                if let Some((x1, v1, c1)) = prev
                    && c1 == copy
                    && !breaks.iter().any(|b| *b > x1 && *b < *x)
                {
                    let ord = cmp_xf(v.xf, v1.xf);
                    let bad = match m {
                        Monotonicity::Increasing => ord == Ordering::Less,
                        Monotonicity::Decreasing => ord == Ordering::Greater,
                        Monotonicity::Constant => ord != Ordering::Equal,
                        Monotonicity::Unknown => false,
                    };
                    if bad && (v.v - v1.v).abs() > f.tol(x1) + f.tol(*x) {
                        cx.fail_at(
                            "monotonicity-reversal",
                            *x,
                            format!(
                                "{m:?} on {:?}: f({x1:e}) = {}, f({x:e}) = {}",
                                (iv.lo.value, iv.hi.value),
                                show(&v1),
                                show(v)
                            ),
                        );
                    }
                }
                prev = Some((*x, *v, copy));
            }
        }
    }

    let mut poles_all: Vec<Family> = d.vertical_asymptotes.clone();
    poles_all.extend(d.excluded.iter().copied());

    // Horizontal and oblique asymptotes, by the far tails: f − (m·x + b)
    // must shrink there. Reported where its largest size over a decade (nine
    // points) is no smaller at all three decades further out, is beyond f's
    // tolerance there, and moves the line as the panel shows it. The
    // decades are 10¹² and 10¹⁵ for a horizontal line, 10⁶ and 10⁹ for an
    // oblique one (whose m·x rounds coarsely further out), or from 100× the
    // expression's largest number if that is further (a shifted function's
    // tail starts beyond its shift).
    {
        use graphing::analysis::AsymptoteSide;
        let reach = centres
            .iter()
            .fold(0.0f64, |m, c| m.max(c.abs()))
            .max(1.0)
            .log10()
            .ceil() as i32
            + 2;
        let mut lines: Vec<(f64, f64, AsymptoteSide, &str, i32)> = Vec::new();
        if definite(&k, flags::HORIZONTAL_ASYMPTOTES) {
            for &(c, side) in &d.horizontal_asymptotes {
                lines.push((0.0, c, side, "horizontal", 12.max(reach)));
            }
        }
        if definite(&k, flags::OBLIQUE_ASYMPTOTES) {
            for &(m, b, side) in &d.oblique_asymptotes {
                lines.push((m, b, side, "oblique", 6.max(reach)));
            }
        }
        for (m, b, side, what, k1) in lines {
            let sides: &[f64] = match side {
                AsymptoteSide::PositiveInfinity => &[1.0],
                AsymptoteSide::NegativeInfinity => &[-1.0],
                _ => &[1.0, -1.0],
            };
            for &s in sides {
                let resid = |k: i32| -> Option<(f64, f64)> {
                    let (mut r, mut t) = (0.0f64, 0.0f64);
                    for j in 0..=8 {
                        let x = s * 10f64.powi(k) * (1.0 + j as f64 / 8.0);
                        // A value too small to size is its double, 0.
                        let v = f.at(x);
                        if !v.def() || !v.v.is_finite() {
                            return None;
                        }
                        let line = m * x + b;
                        r = r.max((v.v - line).abs());
                        t = t.max(f.tol(x) + 8.0 * ulp(line) + 8.0 * ulp(m * x));
                    }
                    Some((r, t))
                };
                let (Some((r1, _)), Some((r2, t2))) = (resid(k1), resid(k1 + 3)) else {
                    continue;
                };
                if r2 > 8.0 * t2 && r2 >= r1 && !same(b, b + r2) && !same(b, b - r2) {
                    cx.fail(
                        "asymptote-not-approached",
                        format!(
                            "{what} y = {}{} as x → {}∞: |f − line| is {r1:e} near 10^{k1}, {r2:e} near 10^{}",
                            if m != 0.0 { format!("{m}·x + ") } else { String::new() },
                            fmt(b),
                            if s > 0.0 { "+" } else { "−" },
                            k1 + 3
                        ),
                    );
                }
            }
        }
    }

    // Inflection points: f's value there, and a change of concavity. The
    // second difference f(t + h) − 2f(t) + f(t − h) at t = x ∓ 2h, at
    // scales h = 10⁻¹⁵ … 10³ and 10⁻⁶ … 10⁻² of |x| (each at least 1000
    // floats of x, and a quarter of the way to any other reported
    // inflection), where it is told from rounding (64× the rounding bounds
    // of the three values): the same sign on both sides at every such
    // scale, at least three of them, is no inflection.
    if definite(&k, flags::INFLECTION_POINTS) {
        for (fam, y) in &d.inflection_points {
            for x in members(fam) {
                let v = f.at(x);
                if v.k == K::Undef {
                    cx.fail("inflection-undefined", format!("({x:e}, {y})"));
                    continue;
                }
                if !v.cmp_ok() || !v.v.is_finite() {
                    continue;
                }
                if (y - v.v).abs() > f.tol(x) && !alike(*y, v.v) {
                    cx.fail(
                        "inflection-value-wrong",
                        format!("({x:e}, {}) but f there = {}", fmt(*y), show(&v)),
                    );
                }
                let s = x.abs().max(1.0);
                // Rounding only (4 ulp plus twice the bound), not f's change
                // over nearby floats: that is the signal here.
                let rounding = |t: f64, v: &Val| 4.0 * ulp(v.v) + 2.0 * f.error(&f.ast, t).1;
                let d2 = |t: f64, h: f64| -> Option<f64> {
                    let (a, c, e) = (f.at(t - h), f.at(t), f.at(t + h));
                    if !(in_doubles(&a) && in_doubles(&c) && in_doubles(&e)) {
                        return None;
                    }
                    let q = a.v - 2.0 * c.v + e.v;
                    let noise =
                        64.0 * (rounding(t - h, &a) + 2.0 * rounding(t, &c) + rounding(t + h, &e));
                    (noise.is_finite() && q.abs() > noise).then_some(q)
                };
                let mut scales: Vec<f64> = (-15..=3).map(|e| 10f64.powi(e)).collect();
                scales.extend((-6..=-2).map(|e| 10f64.powi(e) * s));
                // Only scales with no other reported inflection within
                // reach (two between x ∓ 2h would leave the same concavity).
                let other = d
                    .inflection_points
                    .iter()
                    .flat_map(|p| members(&p.0))
                    .map(|m| (m - x).abs())
                    .filter(|&g| g > 0.0)
                    .fold(f64::INFINITY, f64::min);
                let (mut told, mut same_side) = (0, true);
                for h in scales {
                    if h < 1e3 * ulp(x)
                        || 4.0 * h > other
                        || near(&poles_all, x, x - 4.0 * h, x + 4.0 * h)
                    {
                        continue;
                    }
                    if let (Some(l), Some(r)) = (d2(x - 2.0 * h, h), d2(x + 2.0 * h, h)) {
                        told += 1;
                        same_side &= l.signum() == r.signum();
                    }
                }
                if told >= 3 && same_side {
                    cx.fail(
                        "inflection-not-inflection",
                        format!(
                            "({x:e}, {}): f is concave the same way on both sides",
                            fmt(*y)
                        ),
                    );
                }
            }
        }
    }

    // A periodic f's period is the least one: P/2 and P/3 must not be
    // periods too. Reported where f(x + P/n) equals f(x) within tolerance
    // at every one of at least 200 samples (|x| ≤ 100) over which f varies
    // by 10⁶ times that tolerance (an invisible part of f with the longer period, below the
    // doubles' resolution, would make this a false alarm: none is known in
    // the pool).
    if definite(&k, flags::PERIODICITY)
        && k.periodicity_direction == Periodicity::Periodic
        && let Some(p) = d.period
    {
        for n in [2.0, 3.0] {
            let q = p / n;
            let (mut compared, mut lo, mut hi) = (0, f64::INFINITY, f64::NEG_INFINITY);
            let mut differs = false;
            let mut tmax: f64 = 0.0;
            for (x, v) in xs.iter().zip(&vals) {
                if x.abs() > 100.0 || !in_doubles(v) || compared > 4000 {
                    continue;
                }
                let w = f.at(x + q);
                if !in_doubles(&w) {
                    continue;
                }
                compared += 1;
                lo = lo.min(v.v);
                hi = hi.max(v.v);
                let t = f.tol(*x) + f.tol(x + q);
                tmax = tmax.max(t);
                if (w.v - v.v).abs() > t {
                    differs = true;
                    break;
                }
            }
            if !differs && compared >= 200 && hi - lo > 1e6 * tmax {
                cx.fail(
                    "period-not-fundamental",
                    format!(
                        "period {} claimed, but f(x + {}) = f(x) at {compared} samples (f varies by {:e})",
                        k.periodicity_expression,
                        fmt(q),
                        hi - lo
                    ),
                );
                break;
            }
        }
    }

    // What sampling can't refute, noted when it isn't even supported:
    // "neither" parity without a witness against each of even and odd, and
    // "not periodic" for an f that keeps a circular period at every sample.
    if definite(&k, flags::PARITY) && k.parity == Parity::Neither {
        let (mut not_even, mut not_odd) = (false, false);
        for (x, v) in xs.iter().zip(&vals) {
            if *x <= 0.0 || (not_even && not_odd) {
                continue;
            }
            let w = f.at(-x);
            if in_doubles(v) && in_doubles(&w) {
                let t = f.tol(*x) + f.tol(-x);
                not_even |= (v.v - w.v).abs() > t;
                not_odd |= (v.v + w.v).abs() > t;
            } else if v.cmp_ok() && w.cmp_ok() {
                // Beyond the doubles: exactly, relatively.
                let big = if cmp_xf(v.xf.abs(), w.xf.abs()) == Ordering::Greater {
                    v.xf.abs()
                } else {
                    w.xf.abs()
                };
                let apart = |d: Xf| cmp_xf(d.abs(), big.mul(Xf::of(1e-9))) == Ordering::Greater;
                not_even |= apart(v.xf.add(w.xf.neg()));
                not_odd |= apart(v.xf.add(w.xf));
            } else if v.def() != w.def()
                && (v.def() || v.k == K::Undef)
                && (w.def() || w.k == K::Undef)
            {
                // Defined on one side only: the domain isn't symmetric (an
                // isolated hole is enough).
                not_even = true;
                not_odd = true;
            }
        }
        if !(not_even && not_odd) {
            cx.note(
                "neither-unwitnessed",
                0.0,
                format!(
                    "parity Neither, but no sample shows f {}",
                    if !not_even { "isn't even" } else { "isn't odd" }
                ),
            );
        }
    }
    if definite(&k, flags::PERIODICITY) && k.periodicity_direction == Periodicity::NotPeriodic {
        let turn = match unit {
            TrigUnit::Radians => std::f64::consts::TAU,
            TrigUnit::Degrees => 360.0,
            TrigUnit::Grads => 400.0,
        };
        for q in [turn, turn / 2.0] {
            let (mut compared, mut lo, mut hi, mut tmax) =
                (0, f64::INFINITY, f64::NEG_INFINITY, 0.0f64);
            let mut differs = false;
            for (x, v) in xs.iter().zip(&vals) {
                if x.abs() > 100.0 || !in_doubles(v) || compared > 4000 {
                    continue;
                }
                let w = f.at(x + q);
                if !in_doubles(&w) {
                    continue;
                }
                compared += 1;
                lo = lo.min(v.v);
                hi = hi.max(v.v);
                let t = f.tol(*x) + f.tol(x + q);
                tmax = tmax.max(t);
                if (w.v - v.v).abs() > t {
                    differs = true;
                    break;
                }
            }
            if !differs && compared >= 200 && hi - lo > 1e6 * tmax {
                cx.note(
                    "not-periodic-looks-periodic",
                    0.0,
                    format!(
                        "not periodic, but f(x + {}) = f(x) at {compared} samples",
                        fmt(q)
                    ),
                );
                break;
            }
        }
    }

    // Poles, crossings and turns between the samples.
    let va_definite = definite(&k, flags::VERTICAL_ASYMPTOTES);
    let pole_at = |xu: f64| -> bool {
        let u = ulp(xu).max(f64::MIN_POSITIVE);
        let dist = (1e-3 * xu.abs().max(1.0)).max(1e4 * u);
        [-1.0, 1.0].iter().any(|s| {
            let far = f.at(xu + s * dist);
            if !far.def() || !far.v.is_finite() {
                return false;
            }
            let h = |j: i32| {
                let w = f.at(xu + s * u * 2f64.powi(j));
                if w.def() {
                    Some((w.v - far.v).abs())
                } else {
                    None
                }
            };
            let (Some(h1), Some(h2), Some(h3)) = (h(1), h(3), h(5)) else {
                return false;
            };
            h1 == f64::INFINITY
                || (h1 > 2.5 * h2
                    && h2 > 2.5 * h3
                    && h1 > 64.0 * f.tol(xu + s * dist)
                    && h1 >= 8.0 * far.v.abs())
        })
    };
    if va_definite {
        let mut tested = 0;
        for i in 0..xs.len() {
            if vals[i].k != K::Undef || tested > 500 {
                continue;
            }
            let edge = (i > 0 && vals[i - 1].def()) || (i + 1 < xs.len() && vals[i + 1].def());
            if !edge {
                continue;
            }
            tested += 1;
            let xu = xs[i];
            if pole_at(xu) && !near(&d.vertical_asymptotes, xu, xu, xu) {
                cx.fail_at(
                    "vertical-asymptote-missing",
                    xu,
                    format!(
                        "pole at {xu:e} (undefined there); claimed: {:?}",
                        k.vertical_asymptotes
                    ),
                );
            }
        }
    }
    // Sign changes between neighbouring defined samples: a crossing, a pole
    // or a jump, told apart at adjacent floats.
    let zeros_definite = zero_points;
    let mut bisected = 0;
    // Crossings matched to a reported zero, to tell two from one.
    let mut matched: Vec<((usize, i64), f64)> = Vec::new();
    let zero_fams: Vec<Family> = d.zeros.clone();
    // Within the noise of a reported zero: f stays within its tolerance of 0
    // all the way to it (an expanded (x − 1)⁴ is ±10⁻¹⁶ for a while).
    let in_noise_of_reported = |x: f64| {
        d.zeros.iter().flat_map(members).any(|z| {
            (z - x).abs() <= 1e-2 * x.abs().max(1.0)
                && (0..=32).all(|j| {
                    let y = x + (z - x) * j as f64 / 32.0;
                    let w = f.at(y);
                    w.def() && w.v.is_finite() && w.v.abs() <= f.tol(y)
                })
        })
    };
    // A sample where f is exactly 0, with nonzero samples either side: a
    // zero there (a crossing on a sample has no sign change beside it).
    if zeros_definite {
        for i in 1..xs.len().saturating_sub(1) {
            let (a, v, b) = (&vals[i - 1], &vals[i], &vals[i + 1]);
            if !(v.cmp_ok() && v.xf.is_zero() && a.cmp_ok() && b.cmp_ok())
                || a.xf.is_zero()
                || b.xf.is_zero()
            {
                continue;
            }
            let x = xs[i];
            let resolved = x.abs() <= 1e7 || centres.iter().any(|c| (x - c).abs() <= 1e4);
            if !resolved || near(&poles, x, x, x) {
                continue;
            }
            if near(&zero_fams, x, x, x) {
                if let Some(key) = member_key(&zero_fams, x, x, x) {
                    matched.push((key, x));
                }
            } else if !in_noise_of_reported(x) {
                cx.fail_at(
                    "zero-missing",
                    x,
                    format!("f({x:e}) = 0 exactly; claimed: {}", k.x_intercept),
                );
            }
        }
    }
    for i in 1..xs.len() {
        let (a, b) = (&vals[i - 1], &vals[i]);
        if !(a.def() && b.def()) || a.xf.sign() * b.xf.sign() >= 0.0 || bisected > 400 {
            continue;
        }
        if !(zeros_definite || va_definite) {
            break;
        }
        bisected += 1;
        let (mut l, mut r) = (xs[i - 1], xs[i]);
        let sl = a.xf.sign();
        let mut undefined = false;
        for _ in 0..2100 {
            let m = l + (r - l) / 2.0;
            if m <= l || m >= r {
                break;
            }
            let v = f.at(m);
            if !v.def() {
                undefined = true;
                break;
            }
            if v.xf.is_zero() {
                l = m;
                r = m;
                break;
            }
            if v.xf.sign() == sl { l = m } else { r = m }
        }
        if undefined {
            continue;
        }
        let (vl, vr) = (f.at(l), f.at(r));
        if l == r {
            // An exact zero.
            if zeros_definite {
                match member_key(&zero_fams, l, l, l) {
                    Some(key) => matched.push((key, l)),
                    None if !in_noise_of_reported(l) => cx.fail_at(
                        "zero-missing",
                        l,
                        format!("f({l:e}) = 0; claimed: {}", k.x_intercept),
                    ),
                    None => {}
                }
            }
            continue;
        }
        if vl.sat || vr.sat || a.sat || b.sat {
            continue;
        }
        // Exactly (beyond the doubles too): a pole if far bigger at the
        // adjacent floats than at the samples; a crossing if no bigger and
        // the step across is like f's steps beside it (or tiny against the
        // samples); else a jump.
        let mag = |v: &Val| v.xf.abs();
        let bigger = |p: Xf, q: Xf| cmp_xf(p, q) == Ordering::Greater;
        let ends = if bigger(mag(a), mag(b)) {
            mag(a)
        } else {
            mag(b)
        };
        let pole =
            !bigger(ends.mul(Xf::of(64.0)), mag(&vl)) && !bigger(ends.mul(Xf::of(64.0)), mag(&vr));
        let beside = |y: f64, v: &Val| {
            let w = f.at(y);
            if w.def() && !w.sat {
                w.xf.add(v.xf.neg()).abs()
            } else {
                Xf::ZERO
            }
        };
        let mut local = beside(step(l, -1), &vl);
        for c in [
            beside(step(r, 1), &vr),
            Xf::of(4.0 * ulp(vl.v)),
            Xf::of(4.0 * ulp(vr.v)),
        ] {
            if bigger(c, local) {
                local = c;
            }
        }
        let across = vr.xf.add(vl.xf.neg()).abs();
        let top = if bigger(mag(&vl), mag(&vr)) {
            mag(&vl)
        } else {
            mag(&vr)
        };
        let crossing = !pole
            && !bigger(top, ends)
            && (!bigger(across, local.mul(Xf::of(16.0))) || !bigger(top, ends.mul(Xf::of(1e-3))));
        // Where floats are far apart against f's features (beyond 10⁷, away
        // from the expression's own numbers), or beside a reported pole, a
        // sign change can't be classified.
        let resolved = l.abs() <= 1e7 || centres.iter().any(|c| (l - c).abs() <= 1e4);
        if !resolved || near(&poles, l, xs[i - 1], xs[i]) {
            continue;
        }
        if crossing {
            if zeros_definite {
                if near(&zero_fams, l, xs[i - 1], xs[i]) {
                    // Matched; by position (not the samples' bracket) for
                    // telling two from one (crossings in rounding noise are
                    // not told apart there).
                    if let Some(key) = member_key(&zero_fams, l, l, l) {
                        matched.push((key, l));
                    }
                } else if !in_noise_of_reported(l) {
                    cx.fail_at(
                        "zero-missing",
                        l,
                        format!(
                            "sign change at {l:e}; claimed: {}",
                            if k.x_intercept.is_empty() {
                                "none"
                            } else {
                                &k.x_intercept
                            }
                        ),
                    );
                }
            }
        } else if va_definite && pole && !near(&d.vertical_asymptotes, l, xs[i - 1], xs[i]) {
            cx.fail_at(
                "vertical-asymptote-missing",
                l,
                format!(
                    "pole between {l:e} and {r:e} (f = {:e}, {:e}); claimed: {:?}",
                    vl.v, vr.v, k.vertical_asymptotes
                ),
            );
        }
    }
    for (a, b) in merged(&f, &matched, Kind::Zero) {
        cx.fail_at(
            "zeros-merged",
            a,
            format!(
                "crossings at {a:e} and {b:e} are both matched to one reported zero; claimed: {}",
                k.x_intercept
            ),
        );
    }
    // Turns: a sample with no higher (lower) one between it and samples
    // beyond f's tolerance below (above) it on either side. Each is refined
    // and must be a continuous turn; then it is a reported extremum (one
    // per reported point), and it tells whether an open range bound is
    // reached.
    let range_definite = definite(&k, flags::RANGE) && !d.range.is_empty();
    // Values of found turns, for the range's closed bounds.
    let mut turn_values: Vec<(f64, f64)> = Vec::new();
    for (flag, list, sign, name) in [
        (flags::MAXIMA, &d.maxima, 1.0, "maximum"),
        (flags::MINIMA, &d.minima, -1.0, "minimum"),
    ] {
        let ext_definite = definite(&k, flag);
        if !(ext_definite || range_definite) {
            continue;
        }
        let mut matched: Vec<((usize, i64), f64)> = Vec::new();
        let fams: Vec<Family> = list.iter().map(|m| m.0).collect();
        let mut poles: Vec<Family> = d.vertical_asymptotes.clone();
        poles.extend(d.excluded.iter().copied());
        let higher = |a: &Val, b: &Val| {
            let o = cmp_xf(a.xf, b.xf);
            if sign > 0.0 {
                o == Ordering::Greater
            } else {
                o == Ordering::Less
            }
        };
        // a above b by more than f's tolerance at x (relatively, beyond the
        // doubles).
        let well_above = |a: &Val, b: &Val, x: f64| {
            if !higher(a, b) {
                return false;
            }
            let normal = in_doubles;
            if normal(a) && normal(b) {
                (a.v - b.v).abs() > 8.0 * f.tol(x)
            } else {
                let dif = a.xf.add(b.xf.neg()).abs();
                let big = if cmp_xf(a.xf.abs(), b.xf.abs()) == Ordering::Greater {
                    a.xf.abs()
                } else {
                    b.xf.abs()
                };
                cmp_xf(dif, big.mul(Xf::of(1e-6))) == Ordering::Greater
            }
        };
        // Runs of equal values (start, end), nearest the origin first: the
        // refinements are capped, and features near 0 matter most.
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut i = 0;
        while i < xs.len() {
            let c = i;
            i += 1;
            if !vals[c].cmp_ok() || !vals[c].v.is_finite() {
                continue;
            }
            while i < xs.len()
                && vals[i].cmp_ok()
                && cmp_xf(vals[i].xf, vals[c].xf) == Ordering::Equal
            {
                i += 1;
            }
            runs.push((c, i));
        }
        runs.sort_by(|a, b| xs[a.0].abs().total_cmp(&xs[b.0].abs()));
        let mut refined = 0;
        for (c, i) in runs {
            if refined > 300 {
                break;
            }
            let cv = vals[c];
            // Outward, until a value well below; nothing above on the way.
            let walk = |dir: i64| -> Option<usize> {
                let mut j = c as i64 + if dir < 0 { -1 } else { (i - c) as i64 };
                let mut n = 0;
                while j >= 0 && (j as usize) < xs.len() && n < 4000 {
                    let w = &vals[j as usize];
                    if !w.cmp_ok() || higher(w, &cv) {
                        return None;
                    }
                    if well_above(&cv, w, xs[c]) {
                        return Some(j as usize);
                    }
                    j += dir;
                    n += 1;
                }
                None
            };
            let (Some(l), Some(r)) = (walk(-1), walk(1)) else {
                continue;
            };
            let (xl, xr) = (xs[l], xs[r]);
            if near(&poles, xs[c], xl, xr) {
                continue;
            }
            let bracketed = ext_definite && near(&fams, xs[c], xl, xr);
            // A plateau (ceil(sin x) = 1 on a whole stretch) is no turn; a
            // top flattened by rounding is narrow against its bracket.
            if xs[i - 1] - xs[c] > 0.25 * (xr - xl) {
                continue;
            }
            // Refine within the bracket, and require a continuous turn.
            refined += 1;
            let g = 0.381_966_011_250_105_1;
            let (mut a, mut b) = (xl, xr);
            let better = |p: f64, q: f64| {
                let (vp, vq) = (f.at(p), f.at(q));
                vp.cmp_ok() && (!vq.cmp_ok() || higher(&vp, &vq))
            };
            for _ in 0..300 {
                if b - a <= 4.0 * ulp(a).max(ulp(b)) {
                    break;
                }
                let (m1, m2) = (a + g * (b - a), b - g * (b - a));
                if better(m1, m2) { b = m2 } else { a = m1 }
            }
            // The search must find the turn (at least the sampled value),
            // within reach of the floats' meaning (beyond 10⁷, jumps of a
            // sawtooth are a few hundred floats apart), with f defined
            // around it (not an isolated point of a sparse domain).
            // A search that ends below the sampled value (it can stop a few
            // floats off a narrow top) leaves the sampled top: never a
            // poorer point, and never nothing.
            let (mut xm, mut vm) = (a + (b - a) / 2.0, f.at(a + (b - a) / 2.0));
            if !vm.cmp_ok() || higher(&cv, &vm) {
                (xm, vm) = (xs[c], cv);
            }
            if xm.abs() > 1e7 && !centres.iter().any(|c| (xm - c).abs() <= 1e4) {
                continue;
            }
            if !(f.at(xm - (xr - xl) / 8.0).cmp_ok() && f.at(xm + (xr - xl) / 8.0).cmp_ok()) {
                continue;
            }
            // A plateau: f the same out to beyond what rounding flattens
            // (a top 2·e^(−d²) reads 2 for |d| < 10⁻⁸).
            let flat = |s: f64| {
                let mut dd = ulp(xm).max(f64::MIN_POSITIVE);
                while dd <= (xr - xl) / 4.0 {
                    let w = f.at(xm + s * dd);
                    if !w.cmp_ok() || cmp_xf(w.xf, vm.xf) != Ordering::Equal {
                        return dd;
                    }
                    dd *= 2.0;
                }
                dd
            };
            if flat(-1.0).max(flat(1.0)) > (1u64 << 27) as f64 * ulp(xm) {
                continue;
            }
            // A jump, not a turn: a float beside it worse by half the turn's
            // depth, or the best value at the very end of the bracket (a
            // supremum at a jump).
            let depth = |w: &Val| {
                let normal = in_doubles;
                if normal(&vm) && normal(w) && normal(&vals[l]) && normal(&vals[r]) {
                    let d = (vm.v - vals[l].v).abs().min((vm.v - vals[r].v).abs());
                    higher(&vm, w) && (vm.v - w.v).abs() > d / 2.0
                } else {
                    well_above(&vm, w, xm)
                }
            };
            let rm = f.res(xm);
            let jump = [1, 2, 4, 8, 16, 64, 256].iter().any(|&j| {
                [step(xm, -j), step(xm, j)].iter().any(|&y| {
                    let w = f.at(y);
                    !w.cmp_ok() || depth(&w)
                })
            }) || (rm > 0.0
                && [-1.0, 1.0].iter().any(|&s| {
                    // In a staircase of values a jump can be steps away: one
                    // step there bigger than half the turn's depth.
                    let d = (vm.v - vals[l].v).abs().min((vm.v - vals[r].v).abs());
                    (0..16).any(|j| {
                        let (p, q) = (
                            f.at(xm + s * j as f64 * rm),
                            f.at(xm + s * (j + 1) as f64 * rm),
                        );
                        !(p.cmp_ok() && q.cmp_ok())
                            || (in_doubles(&p) && in_doubles(&q) && (p.v - q.v).abs() > d / 2.0)
                    })
                }));
            let edge = (xm - xl).min(xr - xm) <= 1e-9 * (xr - xl);
            if jump || edge {
                continue;
            }
            // And a turn indeed: nothing well beyond it nearby (the search
            // can stop on a slope between widely spaced samples).
            let mut slope = false;
            let mut dd = ulp(xm).max(f64::MIN_POSITIVE);
            while dd <= (xr - xl) / 4.0 && !slope {
                slope = [xm - dd, xm + dd].iter().any(|&y| {
                    let w = f.at(y);
                    w.cmp_ok() && well_above(&w, &vm, xm)
                });
                dd *= 2.0;
            }
            if slope || near(&poles, xm, xm, xm) {
                continue;
            }
            // A turn: f reaches vm there (a continuous function attains its
            // local extremes).
            if in_doubles(&vm) {
                turn_values.push((xm, vm.v));
            }
            // An open range bound reached at a turn, to within f's
            // tolerance there.
            if range_definite && in_doubles(&vm) {
                for i in &d.range {
                    let end = if sign > 0.0 { i.hi } else { i.lo };
                    if !end.closed && end.value.is_finite() && (vm.v - end.value).abs() <= f.tol(xm)
                    {
                        cx.fail_at(
                            "range-open-bound-reached",
                            xm,
                            format!(
                                "{}: the {name} at {xm:e} is {} (f = {:e}), the open bound",
                                k.range,
                                fmt(end.value),
                                vm.v
                            ),
                        );
                    }
                }
            }
            if !ext_definite {
                continue;
            }
            // Matched (generously, as for crossings); by position for telling
            // two from one.
            match (bracketed || near(&fams, xm, xm, xm), member_key(&fams, xm, xm, xm)) {
                (true, Some(key)) => matched.push((key, xm)),
                (true, None) => {}
                (false, _) => cx.fail_at(
                    "extremum-missing",
                    xm,
                    format!(
                        "{name} near {xm:e} (f = {}; {} and {} at {xl:e} and {xr:e}); claimed: {:?}",
                        show(&vm),
                        show(&vals[l]),
                        show(&vals[r]),
                        list.iter().map(|m| (m.0.x, m.1)).collect::<Vec<_>>()
                    ),
                ),
            }
        }
        for (a, b) in merged(&f, &matched, Kind::Turn(sign)) {
            cx.fail_at(
                "extrema-merged",
                a,
                format!(
                    "{name}s at {a:e} and {b:e} are both matched to one reported {name}; claimed: {:?}",
                    list.iter().map(|m| (m.0.x, m.1)).collect::<Vec<_>>()
                ),
            );
        }
    }

    // A closed range bound no sample, turn or reported value comes to: not
    // a contradiction (it may be reached between samples), a note.
    if range_definite {
        let reported: Vec<f64> = d
            .minima
            .iter()
            .chain(&d.maxima)
            .map(|m| m.1)
            .chain(d.y_intercept)
            .collect();
        for i in &d.range {
            for end in [i.lo, i.hi] {
                if !end.closed || !end.value.is_finite() {
                    continue;
                }
                let b = end.value;
                let reached = reported.iter().any(|&v| same(v, b))
                    || turn_values
                        .iter()
                        .any(|&(x, v)| (v - b).abs() <= f.tol(x) || same(v, b))
                    || xs.iter().zip(&vals).any(|(x, v)| {
                        v.cmp_ok()
                            && v.v.is_finite()
                            && (v.v - b).abs() <= 1e-5 * b.abs().max(1e-300)
                            && (alike(v.v, b) || (v.v - b).abs() <= f.tol(*x))
                    });
                if !reached {
                    cx.note(
                        "closed-bound-unreached",
                        0.0,
                        format!("{}: no sample or turn comes to {}", k.range, fmt(b)),
                    );
                }
            }
        }
    }
    structure(&mut cx, &f, &k, &xs, &vals);
    cx.out
}

/// The elements of each `{…}` set in a panel text (families aside).
fn set_elements(text: &str) -> Vec<Vec<&str>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        let Some(j) = rest[i..].find('}') else { break };
        let inner = &rest[i + 1..i + j];
        if !inner.contains("k ∈ ℤ") && !inner.contains('|') {
            out.push(inner.split(", ").collect());
        }
        rest = &rest[i + j + 1..];
    }
    out
}

/// The members `x₀ + kP` of each `{… | k ∈ ℤ}` set of families in a
/// panel text (a domain's excluded families).
fn family_elements(text: &str) -> Vec<Vec<&str>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        let Some(j) = rest[i..].find('}') else { break };
        if let Some((fams, _)) = rest[i + 1..i + j].split_once(" | ") {
            out.push(fams.split(", ").collect());
        }
        rest = &rest[i + j + 1..];
    }
    out
}

/// One piece `(a, b]`, `[a, b)`, … (or a monotone piece of each period,
/// `(a + kP, b + kP), k ∈ ℤ`): its ends' texts, each with whether it is
/// closed.
fn piece_ends(p: &str) -> Option<(&str, bool, &str, bool)> {
    let p = p.strip_suffix(", k ∈ ℤ").unwrap_or(p);
    let lo_closed = match p.chars().next()? {
        '[' => true,
        '(' => false,
        _ => return None,
    };
    let hi_closed = match p.chars().last()? {
        ']' => true,
        ')' => false,
        _ => return None,
    };
    let (a, b) = p[1..p.len() - 1].split_once(", ")?;
    (!b.contains(", ")).then_some((a, lo_closed, b, hi_closed))
}

/// The pieces of a set written `var ∈ A ∪ B ∪ …`, in order (`{a}`: a,
/// closed, as both ends); `None` for one written otherwise (ℝ, ℝ \ {…},
/// excluded families, a set of single values).
fn set_pieces(text: &str) -> Option<Vec<(&str, bool, &str, bool)>> {
    let (_, body) = text.split_once(" ∈ ")?;
    if body.contains('ℝ') || body.contains('|') || body.contains('\\') {
        return None;
    }
    body.split(" ∪ ")
        .map(
            |p| match p.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                Some(a) => (!a.contains(", ")).then_some((a, true, a, true)),
                None => piece_ends(p),
            },
        )
        .collect()
}

/// The two bounds of each interval `(a, b)`, `[a, b]`, … in a panel text
/// (families aside).
fn interval_bounds(text: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(['(', '[']) {
        let Some(j) = rest[i..].find([')', ']']) else {
            break;
        };
        let inner = &rest[i + 1..i + j];
        if let Some((a, b)) = inner.split_once(", ")
            && !b.contains(", ")
            && !inner.contains('k')
        {
            out.push((a, b));
        }
        rest = &rest[i + 1..];
    }
    out
}

/// Structural checks of the sets and intervals the panel writes, with no
/// digits compared to anything (review 12, R12-M-04: equal rounded texts
/// made a real range a singleton, and two poles one): a range written as
/// one value needs f to take one value at every sample, within its
/// rounding; two excluded points, or the two bounds of an interval, that
/// read alike are a contradiction whatever the numbers; so are the ends
/// of two adjacent pieces of a row that read alike where they can't be
/// one number (review 13, R13-M-06).
fn structure(cx: &mut Ctx<'_>, f: &F, k: &KeyGraphFeatures, xs: &[f64], vals: &[Val]) {
    // A one-value range: f the same everywhere it is defined.
    let singleton = set_elements(&k.range)
        .first()
        .is_some_and(|s| s.len() == 1 && k.range.starts_with("y ∈ {"));
    if singleton {
        let defined: Vec<(f64, f64)> = xs
            .iter()
            .zip(vals)
            .filter(|(_, v)| v.def() && v.v.is_finite())
            .map(|(x, v)| (*x, v.v))
            .collect();
        let lo = defined.iter().min_by(|a, b| a.1.total_cmp(&b.1));
        let hi = defined.iter().max_by(|a, b| a.1.total_cmp(&b.1));
        if let (Some(&(xl, vl)), Some(&(xh, vh))) = (lo, hi)
            && vh - vl > f.tol(xl) + f.tol(xh)
        {
            cx.fail_at(
                "range-singleton-varies",
                xh,
                format!(
                    "{}: f({xl:e}) = {vl:e} and f({xh:e}) = {vh:e}, apart beyond their rounding",
                    k.range
                ),
            );
        }
    }
    // Excluded points (any set's elements), or excluded families, that
    // read alike.
    for text in [&k.domain, &k.range] {
        for set in set_elements(text).into_iter().chain(family_elements(text)) {
            if set.iter().enumerate().any(|(i, e)| set[..i].contains(e)) {
                cx.fail(
                    "set-elements-alike",
                    format!("{text}: two elements read alike"),
                );
            }
        }
    }
    // Interval bounds that read alike: a nonempty interval has two
    // different ends (a single point is written {a}); a monotone piece of
    // each period too.
    let mono: Vec<&str> = k.monotonicity.iter().map(|(t, _)| t.as_str()).collect();
    for text in [k.domain.as_str(), k.range.as_str()]
        .into_iter()
        .chain(mono.iter().copied())
    {
        let periodic = piece_ends(text)
            .filter(|_| text.ends_with("k ∈ ℤ"))
            .map(|(a, _, b, _)| (a, b));
        for (a, b) in interval_bounds(text).into_iter().chain(periodic) {
            if a == b {
                cx.fail(
                    "interval-bounds-alike",
                    format!("{text}: ({a}, {b}) reads as one point"),
                );
            }
        }
    }
    // The ends of adjacent pieces of a row that read alike (review 13,
    // R13-M-06: `(−∞, ≈0.841471] ∪ [≈0.841471, ∞)` hid a gap 10⁻⁷ wide):
    // two pieces can't share a closed end (they would be one piece), and
    // where the panel's own numbers leave a gap between them, f undefined
    // at a number inside it (a domain's gap, and the monotone pieces'
    // about it) shows the two ends are different numbers written alike.
    // Ends that may be one number may read alike (an excluded point
    // between two pieces; two limits enclosed apart, csch x + 1's
    // `(−∞, ≈1) ∪ (≈1, ∞)`), and no digits are compared.
    let bounds = |v: &[Interval]| -> Vec<(f64, f64)> {
        v.iter().map(|i| (i.lo.value, i.hi.value)).collect()
    };
    let mono_data: Vec<Interval> = k.data.monotonicity.iter().map(|(i, _)| *i).collect();
    let rows = [
        (
            k.domain.as_str(),
            set_pieces(&k.domain),
            bounds(&k.data.domain),
            true,
        ),
        (
            k.range.as_str(),
            set_pieces(&k.range),
            bounds(&k.data.range),
            false,
        ),
        (
            "monotonicity",
            mono.iter().map(|t| piece_ends(t)).collect(),
            bounds(&mono_data),
            true,
        ),
    ];
    for (text, pieces, vals, x_row) in rows {
        let Some(pieces) = pieces.filter(|p| p.len() == vals.len()) else {
            continue;
        };
        for i in 1..pieces.len() {
            let (_, _, b, b_closed) = pieces[i - 1];
            let (c, c_closed, _, _) = pieces[i];
            if b != c || b.contains('∞') {
                continue;
            }
            if b_closed && c_closed {
                cx.fail(
                    "row-ends-alike",
                    format!("{text}: two pieces share the closed end {b}"),
                );
                continue;
            }
            let (lo, hi) = (vals[i - 1].1, vals[i].0);
            if !(x_row && lo < hi && lo.is_finite() && hi.is_finite()) {
                continue;
            }
            if let Some(t) = (1..8)
                .map(|j| lo + (hi - lo) * j as f64 / 8.0)
                .find(|&t| lo < t && t < hi && f.at(t).k == K::Undef)
            {
                cx.fail_at(
                    "row-ends-alike",
                    t,
                    format!(
                        "{text}: the ends {lo:e} and {hi:e} of a gap (f undefined at {t:e}) both read {b}"
                    ),
                );
            }
        }
    }
}

/// Two numbers the panel would show as the same claim.
fn same(a: f64, b: f64) -> bool {
    a == b || alike(a, b) || (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}

fn same_intervals(a: &[Interval], b: &[Interval]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(p, q)| {
            p.lo.closed == q.lo.closed
                && p.hi.closed == q.hi.closed
                && same(p.lo.value, q.lo.value)
                && same(p.hi.value, q.hi.value)
        })
}

fn same_families(a: &[Family], b: &[Family]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(p, q)| {
            same(p.x, q.x)
                && match (p.period, q.period) {
                    (Some(s), Some(t)) => same(s, t),
                    (None, None) => true,
                    _ => false,
                }
        })
}

/// Two spellings of one function: `lhs`'s compiled value (what the app
/// plots and analyses) against `rhs`'s reference value, where `x > 0` if
/// `positive` (else everywhere); and, for spellings equal everywhere, every
/// answer both analyses give: one function can't have two.
fn check_pair(lhs: &str, rhs: &str, positive: bool) -> Vec<Finding> {
    let name = format!("{lhs}  ≡  {rhs}");
    let mut cx = Ctx {
        expr: &name,
        out: Vec::new(),
    };
    let opts = CompileOptions::default();
    let load = |s: &str| -> Option<(Equation, F)> {
        let eq = Equation::parse(&format!("y={s}")).ok()?;
        let (_, ast) = eq.explicit()?;
        let lits = Literals::of(&format!("y={s}"), ParseOptions::default()).unwrap_or_default();
        let prog = Program::compile_typed(ast, &opts, &lits).ok()?;
        let f = F {
            ast: ast.clone(),
            lits,
            prog,
            unit: TrigUnit::Radians,
            memo: Default::default(),
        };
        Some((eq, f))
    };
    let (Some((el, fl)), Some((er, fr))) = (load(lhs), load(rhs)) else {
        cx.fail("does-not-parse", String::new());
        return cx.out;
    };
    let (kl, kr) = (analyze(&el, &opts), analyze(&er, &opts));

    // Values.
    let mut xs = samples(&fl, &kl, &[], false);
    xs.extend(samples(&fr, &kr, &[], false));
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    for x in xs {
        if positive && x <= 0.0 {
            continue;
        }
        let t = fr.at(x);
        if !t.cmp_ok() || !t.v.is_finite() {
            continue;
        }
        let c = fl.prog.eval(x, 0.0);
        if c.is_nan() {
            cx.fail_at(
                "spelling-value-differs",
                x,
                format!(
                    "x={x:e}: the app's {lhs} is undefined, {rhs} = {}",
                    show(&t)
                ),
            );
            continue;
        }
        // The app's own rounding at x; where it can't be bounded (the
        // reference can't evaluate that spelling), the point can't be told.
        let (ev, el) = fl.error(&fl.ast, x);
        if ev.is_nan() || !el.is_finite() {
            continue;
        }
        let allow = fr.tol(x) + 8.0 * ulp(c.abs().max(t.v.abs())) + 2.0 * el;
        if (c - t.v).abs() > allow && !alike(c, t.v) {
            cx.fail_at(
                "spelling-value-differs",
                x,
                format!(
                    "x={x:e}: the app's {lhs} = {c:e}, {rhs} = {} (allowance {allow:e})",
                    show(&t)
                ),
            );
        }
    }

    // Answers (equal everywhere only).
    if positive
        || kl.analysis_error != AnalysisError::NoError
        || kr.analysis_error != AnalysisError::NoError
    {
        return cx.out;
    }
    let (dl, dr) = (&kl.data, &kr.data);
    let both = |flag: u32| definite(&kl, flag) && definite(&kr, flag);
    let mut differ = |what: &str, a: String, b: String| {
        cx.fail(
            "equivalent-answers-differ",
            format!("{what}: {lhs} says {a}, {rhs} says {b}"),
        );
    };
    if both(flags::Y_INTERCEPT) {
        let ok = match (dl.y_intercept, dr.y_intercept) {
            (Some(a), Some(b)) => same(a, b),
            (None, None) => true,
            _ => false,
        };
        if !ok {
            differ(
                "y-intercept",
                kl.y_intercept.clone(),
                kr.y_intercept.clone(),
            );
        }
    }
    if both(flags::RANGE) && !same_intervals(&dl.range, &dr.range) {
        differ("range", kl.range.clone(), kr.range.clone());
    }
    if both(flags::DOMAIN)
        && !(same_intervals(&dl.domain, &dr.domain) && same_families(&dl.excluded, &dr.excluded))
    {
        differ("domain", kl.domain.clone(), kr.domain.clone());
    }
    if both(flags::ZEROS)
        && kl.x_intercept != kr.x_intercept
        && !same_families(&dl.zeros, &dr.zeros)
    {
        differ(
            "x-intercepts",
            kl.x_intercept.clone(),
            kr.x_intercept.clone(),
        );
    }
    if both(flags::PARITY) && kl.parity != kr.parity {
        differ(
            "parity",
            format!("{:?}", kl.parity),
            format!("{:?}", kr.parity),
        );
    }
    if both(flags::PERIODICITY) {
        let ok = match (dl.period, dr.period) {
            (Some(a), Some(b)) => same(a, b),
            (None, None) => kl.periodicity_direction == kr.periodicity_direction,
            _ => false,
        };
        if !ok {
            differ(
                "period",
                kl.periodicity_expression.clone(),
                kr.periodicity_expression.clone(),
            );
        }
    }
    for (flag, a, b, what) in [
        (flags::MINIMA, &dl.minima, &dr.minima, "minima"),
        (flags::MAXIMA, &dl.maxima, &dr.maxima, "maxima"),
        (
            flags::INFLECTION_POINTS,
            &dl.inflection_points,
            &dr.inflection_points,
            "inflection points",
        ),
    ] {
        let ok = a.len() == b.len()
            && a.iter().zip(b.iter()).all(|(p, q)| {
                same_families(std::slice::from_ref(&p.0), std::slice::from_ref(&q.0))
                    && same(p.1, q.1)
            });
        if both(flag) && !ok {
            differ(what, format!("{a:?}"), format!("{b:?}"));
        }
    }
    if both(flags::VERTICAL_ASYMPTOTES)
        && !same_families(&dl.vertical_asymptotes, &dr.vertical_asymptotes)
    {
        differ(
            "vertical asymptotes",
            format!("{:?}", kl.vertical_asymptotes),
            format!("{:?}", kr.vertical_asymptotes),
        );
    }
    // Whether a constant has its own value as an asymptote is a convention
    // (the panel gives a literal constant none): not compared.
    let point = |d: &graphing::analysis::AnalysisData| {
        d.range.len() == 1 && d.range[0].lo.value == d.range[0].hi.value
    };
    let constant = point(dl) || point(dr);
    if both(flags::HORIZONTAL_ASYMPTOTES) && !constant {
        let (a, b) = (&dl.horizontal_asymptotes, &dr.horizontal_asymptotes);
        let ok = a.len() == b.len() && a.iter().zip(b).all(|(p, q)| same(p.0, q.0) && p.1 == q.1);
        if !ok {
            differ(
                "horizontal asymptotes",
                format!("{:?}", kl.horizontal_asymptotes),
                format!("{:?}", kr.horizontal_asymptotes),
            );
        }
    }
    if both(flags::OBLIQUE_ASYMPTOTES) && !constant {
        let (a, b) = (&dl.oblique_asymptotes, &dr.oblique_asymptotes);
        let ok = a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(p, q)| same(p.0, q.0) && same(p.1, q.1) && p.2 == q.2);
        if !ok {
            differ(
                "oblique asymptotes",
                format!("{:?}", kl.oblique_asymptotes),
                format!("{:?}", kr.oblique_asymptotes),
            );
        }
    }
    cx.out
}

// ---------------------------------------------------------------- the pool

/// Review 10's cases and more of their shapes.
fn adversarial() -> Vec<String> {
    let mut v: Vec<String> = [
        "1+sqrt(-exp(-(x+800)))",
        "sqrt(-exp(-(x+800)))^0",
        "sqrt(exp(-(x+800)))*exp((x+800)/2)",
        "exp(-1000)*(x+1)*exp(500)*exp(500)",
        "((x+2^1023)+(x+2^1023))/(x+2^1023)",
        "exp(-(x-100)^2)+exp(-x^2)",
        "exp(-(x-1000)^2)+exp(-x^2)",
        "exp(-(x-1234)^2)+exp(-x^2)",
        "(x-1000000000000)/(x-1000000000000.0625)",
        "(x-1000000000)/(x-1000000000.0625)",
        "2*exp(-(x-1522756)^2)+exp(-x^2)",
        "exp(-(x-1234*1234)^2)+exp(-x^2)",
        "sin(x)+10000000000000",
        "cos(x)+10000000000000",
        "2*sin(x)+10000000000000",
        // Separated bumps, centres as products and powers.
        "exp(-(x-50)^2)+exp(-(x+70)^2)",
        "3*exp(-(x-2^20)^2)+exp(-x^2)",
        "exp(-(x-10^7)^2)-exp(-x^2)",
        "exp(-(x-1000*1000)^2)+exp(-(x-999*1001)^2)",
        "1/(1+(x-3^13)^2)+1/(1+x^2)",
        "exp(-(x-7*11*13)^2/0.01)+x^2/10^9",
        // Close quotients.
        "(x-1000000)/(x-1000000.0625)",
        "(x-2^40)/(x-2^40-1/16)",
        "1/(x-1000000000000.0625)+1/(x-1000000000000)",
        "(x-1000*1000)/(x-1000*1000-1/8)",
        // Huge offsets of bounded functions.
        "sin(x)+1000000000000",
        "atan(x)+100000000000000",
        "tanh(x)+10^13",
        "cos(2*x)*3+10^12",
        "floor(sin(x))+10^12",
        // Hidden underflow inside sqrt, ln, pow.
        "ln(exp(-(x+800)))",
        "sqrt(exp(-(x+800)))",
        "(exp(-(x+800)))^0.5*exp(400)",
        "ln(1+exp(-(x+800)))",
        "exp(-(x+800))^0",
        "1/sqrt(exp(-(x+800)))",
        "sqrt(-exp(-x^2-800))+1",
        "ln(-exp(-(x+800)))+1",
        "(-exp(-(x+800)))^(1/2)+2",
        "x^2*exp(-1000)*exp(1000)",
        // Review 13: a gap between pieces 10⁻⁷ wide (R13-M-06); an
        // exponent and a degree nowhere defined, enclosed by whole numbers
        // (R13-M-02); whole counts past 60, r > n (R13-M-03).
        "sqrt((x-sin(1))*(x-sin(1)-0.0000001))",
        "ln((x-sin(1))*(x-sin(1)-0.0000001))",
        "x^floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-30)))",
        "root(x,1+floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-30))))",
        "nCr(61,62)",
        "nPr(61,62)",
        "abs(nCr(61,62))+1",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for c in [100.0, 1000.0, 1234.0, 123456.0] {
        v.push(format!("exp(-(x-{c})^2)+exp(-x^2)"));
        v.push(format!("exp(-(x-{c})^2)+exp(-(x+{c})^2)"));
    }
    // Review 11: narrow peaks far out (widths √w), Gaussian and rational,
    // beside an ordinary one at 0; a rational peak's centre is defined.
    for c in ["1234", "1522756", "1234*1234"] {
        for w in ["0.001", "0.000001", "0.000000001", "0.000000000001"] {
            v.push(format!("2*exp(-(x-{c})^2/{w})+exp(-x^2)"));
            v.push(format!("1/(1+(x-{c})^2/{w})+exp(-x^2)"));
            v.push(format!("2/(1+(x-{c})^2/{w})+1/(1+x^2)"));
        }
    }
    // Review 11's arithmetic cases (and see `spellings`).
    for e in [
        "acot(1000000000000000)*1000000000000000",
        "acot(10^17)*10^17",
        "acot(x)*1000000000000000",
        "ln(exp(1000000000000000)^4)-4000000000000000",
        "ln(exp(1000000000000000)*exp(1000000000000000)*exp(1000000000000000)*exp(1000000000000000))-4000000000000000",
    ] {
        v.push(e.to_string());
    }
    v
}

/// Planted false answers: each must be caught by the named check (the
/// checks' own discrimination test). Returns whether all were.
fn selftest() -> bool {
    use graphing::analysis::{AsymptoteSide, Bound};
    type Plant = Box<dyn Fn(&mut KeyGraphFeatures)>;
    let one = |x: f64| Family::single(x);
    let cases: Vec<(&str, &str, Plant)> = vec![
        (
            "x^3",
            "inflection-not-inflection",
            Box::new(move |k| k.data.inflection_points.push((one(1.0), 1.0))),
        ),
        (
            "x^3",
            "inflection-value-wrong",
            Box::new(|k| {
                for p in &mut k.data.inflection_points {
                    p.1 += 0.5;
                }
            }),
        ),
        (
            "atan(x)",
            "asymptote-not-approached",
            Box::new(|k| {
                for a in &mut k.data.horizontal_asymptotes {
                    a.0 *= 0.99;
                }
            }),
        ),
        (
            "x+1/x",
            "asymptote-not-approached",
            Box::new(|k| {
                for a in &mut k.data.oblique_asymptotes {
                    a.1 += 1.0;
                }
            }),
        ),
        (
            "sin(x)",
            "period-not-fundamental",
            Box::new(|k| {
                k.data.period = k.data.period.map(|p| 2.0 * p);
            }),
        ),
        (
            "sin(x)",
            "range-open-bound-reached",
            Box::new(|k| {
                for i in &mut k.data.range {
                    i.hi = Bound {
                        value: i.hi.value,
                        closed: false,
                    };
                }
            }),
        ),
        (
            "(x-1.1)*(x-1.1000003)",
            "zeros-merged",
            Box::new(move |k| k.data.zeros = vec![one(1.1)]),
        ),
        (
            "(x-1)^2*(x-1.000001)^2",
            "extrema-merged",
            Box::new(move |k| k.data.minima = vec![(one(1.0), 0.0)]),
        ),
        (
            "x^2+1",
            "excluded-point-defined",
            Box::new(move |k| k.data.excluded.push(one(2.0))),
        ),
        (
            "x^2",
            "zero-missing",
            Box::new(|k| {
                k.data.zeros.clear();
                k.x_intercept.clear();
            }),
        ),
        (
            "x^2",
            "neither-unwitnessed",
            Box::new(|k| k.parity = Parity::Neither),
        ),
        (
            "1/x",
            "vertical-asymptote-missing",
            Box::new(|k| {
                k.data.vertical_asymptotes.clear();
                k.data.excluded.clear();
            }),
        ),
        (
            "exp(-x^2)",
            "asymptote-not-approached",
            Box::new(|k| {
                k.data.horizontal_asymptotes = vec![(0.5, AsymptoteSide::AnyInfinity)];
            }),
        ),
        // Review 12, R12-M-04: a range written as one value it isn't
        // (the numbers behind it left as they were: only the text lies).
        (
            "sin(1)+(sin(x)+2)/10000000",
            "range-singleton-varies",
            Box::new(|k| k.range = "y ∈ {≈0.841471}".into()),
        ),
        // Two excluded points, and an interval's two bounds, that read
        // alike (the review's were 10⁻⁷ apart; here only the text lies).
        (
            "1/((x-1)*(x-2))",
            "set-elements-alike",
            Box::new(|k| k.domain = "x ∈ ℝ \\ {≈1.5, ≈1.5}".into()),
        ),
        (
            "1/((x-1)*(x-2))",
            "interval-bounds-alike",
            Box::new(|k| {
                if let Some(m) = k.monotonicity.get_mut(1) {
                    m.0 = "(≈1.5, ≈1.5)".into();
                }
            }),
        ),
        // Review 13, R13-M-06: the two ends of a gap between pieces
        // written alike, as they were (only the text lies): closed, they
        // say two pieces share a point; open, f is undefined between the
        // numbers behind them.
        (
            "sqrt((x-sin(1))*(x-sin(1)-0.0000001))",
            "row-ends-alike",
            Box::new(|k| k.domain = "x ∈ (−∞, ≈0.841471] ∪ [≈0.841471, ∞)".into()),
        ),
        (
            "ln((x-sin(1))*(x-sin(1)-0.0000001))",
            "row-ends-alike",
            Box::new(|k| k.domain = "x ∈ (−∞, ≈0.841471) ∪ (≈0.841471, ∞)".into()),
        ),
        (
            "sqrt((x-sin(1))*(x-sin(1)-0.0000001))",
            "row-ends-alike",
            Box::new(|k| {
                if let [a, b] = k.monotonicity.as_mut_slice() {
                    a.0 = "(−∞, ≈0.841471)".into();
                    b.0 = "(≈0.841471, ∞)".into();
                }
            }),
        ),
    ];
    let mut ok = true;
    for (expr, want, plant) in &cases {
        let found = check_with(expr, TrigUnit::Radians, &[], false, plant.as_ref());
        let hit = found.iter().any(|f| f.class == *want);
        ok &= hit;
        println!(
            "{} y={expr}: planted, expecting {want}: {}",
            if hit { "ok  " } else { "MISS" },
            found.iter().map(|f| f.class).collect::<Vec<_>>().join(", ")
        );
    }
    ok
}

/// Review 11's case in degrees.
const DEGREES: &[&str] = &["sin(x+2^-1074)/(x+2^-1074)", "sin(x)/x", "tan(x)", "cot(x)"];

/// Expressions equal to each other where `x > 0` (or everywhere, `false`):
/// the first's compiled value, which the app plots and analyses, against
/// the second's reference value (the truth by another route, where the
/// first's own reference shares the same arithmetic).
fn spellings() -> Vec<(&'static str, &'static str, bool)> {
    vec![
        ("acot(x)", "atan(1/x)", true),
        ("acot(x)*x", "atan(1/x)*x", true),
        (
            "acot(1000000000000000)*1000000000000000",
            "atan(1/1000000000000000)*1000000000000000",
            false,
        ),
        ("acot(10^17)*10^17", "atan(1/10^17)*10^17", false),
        ("ln(exp(1000000000000000)^4)-4000000000000000", "0", false),
        ("ln(exp(x)^4)", "4*x", false),
        ("ln(exp(x)^3)-3*x", "0", false),
        ("ln(exp(x)^2)", "2*x", false),
        ("sqrt(exp(x)^2)", "exp(x)", false),
        ("exp(x)^2/exp(x)", "exp(x)", false),
        ("1/(1+exp(-x))", "exp(x)/(exp(x)+1)", false),
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--selftest") {
        std::process::exit(if selftest() { 0 } else { 1 });
    }
    if args.first().is_some_and(|a| a == "--dump") && args.len() >= 2 {
        // One function: its analysis and what the checks find; the exit
        // status is 1 if they find anything, as for a whole run.
        let opts = CompileOptions::default();
        let eq = Equation::parse(&format!("y={}", args[1])).expect("parses");
        let k = analyze(&eq, &opts);
        println!("{k:#?}");
        let found = check(&args[1], TrigUnit::Radians, &[], false);
        for f in &found {
            println!(
                "{}{}: {}",
                if f.note { "note: " } else { "" },
                f.class,
                f.detail
            );
        }
        if found.iter().any(|f| !f.note) {
            std::process::exit(1);
        }
        return;
    }
    let filter = args.iter().find(|a| !a.starts_with("--")).cloned();
    // (expression, unit, centres, wide grid, the other spelling and whether
    // only x > 0 is compared).
    type Job = (String, TrigUnit, Vec<f64>, bool, Option<(String, bool)>);
    let mut pool: Vec<Job> = Vec::new();
    let mut push = |e: String, u: TrigUnit, c: Vec<f64>, w: bool| {
        if filter.as_ref().is_none_or(|f| e.contains(f.as_str())) {
            pool.push((e, u, c, w, None));
        }
    };
    let transforms = transforms();
    for base in BASES {
        push(base.to_string(), TrigUnit::Radians, vec![], false);
        for t in &transforms {
            push(t.apply(base), TrigUnit::Radians, vec![t.a], false);
        }
    }
    for (e, u, _, _) in UNDEFINED {
        push(e.to_string(), *u, vec![], false);
    }
    for s in specials() {
        push(s.expr, TrigUnit::Radians, s.centres, s.wide);
    }
    for (p, q) in equivalent() {
        push(p, TrigUnit::Radians, vec![], false);
        push(q, TrigUnit::Radians, vec![], false);
    }
    for e in adversarial() {
        push(e, TrigUnit::Radians, vec![], false);
    }
    for e in DEGREES {
        push(e.to_string(), TrigUnit::Degrees, vec![], false);
    }
    pool.sort_by(|a, b| a.0.cmp(&b.0).then((a.1 as u8).cmp(&(b.1 as u8))));
    pool.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    let mut pairs: Vec<(String, String, bool)> = equivalent()
        .into_iter()
        .map(|(p, q)| (p, q, false))
        .collect();
    pairs.extend(
        spellings()
            .into_iter()
            .map(|(p, q, pos)| (p.to_string(), q.to_string(), pos)),
    );
    for (p, q, pos) in pairs {
        if filter
            .as_ref()
            .is_none_or(|f| p.contains(f.as_str()) || q.contains(f.as_str()))
        {
            pool.push((p, TrigUnit::Radians, vec![], false, Some((q, pos))));
        }
    }

    let started = std::time::Instant::now();
    let next = AtomicUsize::new(0);
    let found: Mutex<Vec<Finding>> = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, AtOrd::Relaxed);
                    let Some((e, u, c, w, other)) = pool.get(i) else {
                        break;
                    };
                    let mut r = match other {
                        Some((q, pos)) => check_pair(e, q, *pos),
                        None => check(e, *u, c, *w),
                    };
                    if *u != TrigUnit::Radians {
                        for f in &mut r {
                            f.expr = format!("{} [{u:?}]", f.expr);
                        }
                    }
                    found.lock().unwrap().extend(r);
                }
            });
        }
    });
    let found = found.into_inner().unwrap();
    let (notes, fails): (Vec<&Finding>, Vec<&Finding>) = found.iter().partition(|f| f.note);
    let functions = |fs: &[&Finding]| {
        let mut e: Vec<&str> = fs.iter().map(|f| f.expr.as_str()).collect();
        e.sort();
        e.dedup();
        e.len()
    };
    println!(
        "{} functions in {:.1} s; {} contradictions in {} functions ({} notes in {} functions: not contradictions)",
        pool.len(),
        started.elapsed().as_secs_f64(),
        fails.len(),
        functions(&fails),
        notes.len(),
        functions(&notes),
    );
    let all = args.iter().any(|a| a == "--all");
    for (title, list) in [("", &fails), ("note: ", &notes)] {
        let mut by: std::collections::BTreeMap<&str, Vec<&Finding>> = Default::default();
        for f in list.iter() {
            by.entry(f.class).or_default().push(f);
        }
        for (class, fs) in &by {
            println!(
                "\n## {title}{class}: {} in {} functions",
                fs.len(),
                functions(fs)
            );
            let mut shown = 0;
            let mut last = "";
            for f in fs.iter() {
                if f.expr == last {
                    continue;
                }
                last = &f.expr;
                println!("  y={}: {}", f.expr, f.detail);
                shown += 1;
                if shown >= 60 && !all {
                    println!("  … (--all for every one)");
                    break;
                }
            }
        }
    }
    if !fails.is_empty() {
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------- copied from sweep.rs

fn transforms() -> Vec<Aff> {
    let mut t: Vec<Aff> = Vec::new();
    for a in [1.0, 1e3, 1e6, 1e9, 1e12] {
        t.push(Aff { a, ..ID });
    }
    for c in [
        1e-12, -1e-12, 1e-9, -1e-9, 1e-7, -1e-7, 1e-3, -1e-3, 1.0, -1.0, 1e6, -1e6,
    ] {
        t.push(Aff { c, ..ID });
    }
    for k in [1e-12, -1e-12, 1e-6, -1e-6, -1.0, 1e6, 1e12] {
        t.push(Aff { k, ..ID });
    }
    for s in [1e-3, 1e3, 1e6] {
        t.push(Aff { s, ..ID });
    }
    t.push(Aff {
        a: 1000.0,
        s: 1.0,
        k: -2.0,
        c: 1.0,
    });
    t.push(Aff {
        a: 1e9,
        s: 1000.0,
        k: 1.0,
        c: -1.0,
    });
    t.push(Aff {
        a: -1e6,
        s: 1.0,
        k: 1e-6,
        c: 1e6,
    });
    t
}

fn num(v: f64) -> String {
    let s = format!("{v}");
    if v < 0.0 { format!("({s})") } else { s }
}

/// y = k·f((x − a)/s) + c.
#[derive(Clone, Copy)]
struct Aff {
    a: f64,
    s: f64,
    k: f64,
    c: f64,
}

const ID: Aff = Aff {
    a: 0.0,
    s: 1.0,
    k: 1.0,
    c: 0.0,
};

impl Aff {
    fn apply(self, base: &str) -> String {
        let mut e = base.to_string();
        if self.a != 0.0 || self.s != 1.0 {
            let inner = match (self.a != 0.0, self.s != 1.0) {
                (true, false) => format!("x-{}", num(self.a)),
                (false, true) => format!("x/{}", num(self.s)),
                _ => format!("(x-{})/{}", num(self.a), num(self.s)),
            };
            e = subst_x(&e, &inner);
        }
        if self.k != 1.0 {
            e = format!("{}*({e})", num(self.k));
        }
        if self.c != 0.0 {
            e = format!("({e})+{}", num(self.c));
        }
        e
    }
}

/// Substitute `repl` for the variable x (not the x inside `exp`, `max`, ...).
fn subst_x(expr: &str, repl: &str) -> String {
    let chars: Vec<char> = expr.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1).copied();
        if c == 'x'
            && !prev.is_some_and(|p| p.is_ascii_alphabetic())
            && !next.is_some_and(|n| n.is_ascii_alphabetic())
        {
            out.push('(');
            out.push_str(repl);
            out.push(')');
        } else {
            out.push(c);
        }
    }
    out
}

/// Base functions (the right-hand side of `y=`).
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
    // Extras.
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
    // More everyday functions.
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

/// Expressions whose inner operations are undefined at known points while
/// an outer one could make IEEE arithmetic finite again, with those points
/// (which must be outside the claimed domain and never be intercepts). The
/// flag marks the calculator's deliberate 0⁰ = 1, under which the listed
/// points are defined after all: disagreements there are policy.
const UNDEFINED: &[(&str, TrigUnit, &[f64], bool)] = &[
    ("x^-1", TrigUnit::Radians, &[0.0], false),
    ("1/(x^-1)", TrigUnit::Radians, &[0.0], false),
    ("1/(1/x)", TrigUnit::Radians, &[0.0], false),
    ("x*(1/x)", TrigUnit::Radians, &[0.0], false),
    ("1^(1/x)", TrigUnit::Radians, &[0.0], false),
    ("(1/x)^0", TrigUnit::Radians, &[0.0], false),
    ("1^(ln(x))", TrigUnit::Radians, &[0.0, -1.0, -5.0], false),
    ("exp(-x^-2)", TrigUnit::Radians, &[0.0], false),
    ("1/asech(x)", TrigUnit::Radians, &[0.0], false),
    ("1/acoth(x)", TrigUnit::Radians, &[1.0, -1.0], false),
    ("exp(-atanh(x))", TrigUnit::Radians, &[1.0, -1.0], false),
    (
        "atan(tan(x))",
        TrigUnit::Degrees,
        &[90.0, -90.0, 270.0],
        false,
    ),
    (
        "atan(tan(x))",
        TrigUnit::Grads,
        &[100.0, -100.0, 300.0],
        false,
    ),
    ("atan(sec(x))", TrigUnit::Degrees, &[90.0, 270.0], false),
    ("atan(csc(x))", TrigUnit::Degrees, &[0.0, 180.0], false),
    ("atan(cot(x))", TrigUnit::Degrees, &[0.0, 180.0], false),
    ("atan(1/(x-x))", TrigUnit::Radians, &[0.0, 1.0, -3.0], false),
    ("ln(x-x)", TrigUnit::Radians, &[0.0, 1.0], false),
    ("e^(-1/abs(x))", TrigUnit::Radians, &[0.0], false),
    ("atan(1/sin(x))", TrigUnit::Radians, &[0.0], false),
    ("1/e^(1/x)", TrigUnit::Radians, &[0.0], false),
    ("(x-x)^0", TrigUnit::Radians, &[0.0, 1.0], true),
    ("atan(x^-2)", TrigUnit::Radians, &[0.0], false),
    ("atan(log(x))", TrigUnit::Radians, &[0.0], false),
    ("atan(ln(abs(x)))", TrigUnit::Radians, &[0.0], false),
    ("0^(-x)", TrigUnit::Radians, &[0.0, 1.0, 2.5], true),
    ("root(x,-2)", TrigUnit::Radians, &[0.0], false),
    ("1/root(x,-2)", TrigUnit::Radians, &[0.0], false),
    ("x^(-1/3)", TrigUnit::Radians, &[0.0], false),
    ("1/x^(-1/3)", TrigUnit::Radians, &[0.0], false),
    ("e^(1/(x-1))", TrigUnit::Radians, &[1.0], false),
    ("atan(1/(x^2-1))", TrigUnit::Radians, &[1.0, -1.0], false),
];

/// A hand-written case: the expression and where to sample densely (its
/// features sit near these centres). `wide` adds a long fine grid, for
/// bounded aperiodic sums whose extremes lie far out.
struct Special {
    expr: String,
    centres: Vec<f64>,
    wide: bool,
}

fn special(expr: impl Into<String>, centres: &[f64]) -> Special {
    Special {
        expr: expr.into(),
        centres: centres.to_vec(),
        wide: false,
    }
}

/// The hand-written families.
fn specials() -> Vec<Special> {
    let mut v = Vec::new();
    // Several distinct close centres, as products, nested products (the
    // same function: see `EQUIVALENT`), quotients and triples.
    for a in [1e6, 1e9, 1e12] {
        for d in [0.0625, 1.0, 1000.0] {
            let (b, c) = (a + d, a + 2.0 * d);
            let cs = [a, b, c];
            v.push(special(format!("(x-{a})*(x-{b})"), &cs));
            v.push(special(format!("(x-{a})*((x-{a})-{d})"), &cs));
            v.push(special(format!("(x-{a})/(x-{b})"), &cs));
            v.push(special(format!("(x-{b})/(x-{a})^2"), &cs));
            v.push(special(format!("(x-{a})*(x-{b})*(x-{c})"), &cs));
            v.push(special(format!("(x-{a})^2*(x-{b})"), &cs));
        }
    }
    // Two frames at once: a far centre and a slow quadratic about 0.
    for (a, s) in [(1e6, 1e9), (1e12, 1e15), (1e6, 1e3), (1e9, 1e9)] {
        for c in ["0.0000001", "1"] {
            let m = a * s * s / (s * s + 1.0);
            v.push(special(format!("(x-{a})^2+{c}+(x/{s})^2"), &[a, m, 0.0]));
        }
    }
    v.push(special("(x-1000000)^2+(x-1000001)^2", &[1e6, 1e6 + 1.0]));
    v.push(special("(x-1000000000)^2*(x+1000000000)^2", &[1e9, -1e9]));
    v.push(special(
        "(x-1000000000000)^2+0.0000001+((x-1000)/1000)^2",
        &[1e12, 1000.0],
    ));
    // Discontinuous functions of trig, with offsets and shifts.
    for e in [
        "floor(sin(x))",
        "ceil(sin(x))",
        "floor(cos(x))",
        "ceil(cos(x))",
        "round(sin(x))",
        "sign(sin(x))",
        "floor(2*sin(x))",
        "floor(sin(x))+0.5",
        "atan(tan(x))",
        "atan(tan(x))+1",
        "atan(tan(x))+10",
        "atan(tan(x))-1000000",
        "atan(tan(x-1000))",
        "floor(cos(x-1000000))",
        "x-floor(x)",
        "floor(x)",
        "floor(x)-0.5",
        "mod(x,1)",
        "1/(1+cos(x))",
    ] {
        v.push(special(e, &[0.0]));
    }
    // Bounded, not periodic.
    for e in [
        "sin(x)+sin(sqrt(2)*x)",
        "sin(x)*sin(sqrt(3)*x)",
        "cos(x)+cos(pi*x)",
    ] {
        v.push(Special {
            expr: e.into(),
            centres: vec![0.0],
            wide: true,
        });
    }
    // Exactly zero, everywhere or where defined.
    for e in [
        "0*x",
        "x-x",
        "sin(x)-sin(x)",
        "0*e^x",
        "(x-x)*x^2",
        "0/x",
        "0/x^2",
        "x^2/x^2",
    ] {
        v.push(special(e, &[0.0]));
    }
    // Small variation on a large value, and a tiny periodic difference.
    for e in [
        "sin(x)+10000000000000",
        "cos(x)+10000000000000",
        "sin(x)+1000000000000",
        "sin(x)-sin(x+0.000000001)",
        "tan(x)+1000000",
        "sin(x)/1000000000000+1",
    ] {
        v.push(special(e, &[0.0]));
    }
    // Underflow and overflow of intermediates (review 9, R9-M-04) and
    // everyday compositions that should be easy.
    for e in [
        "exp(x)^-1",
        "1/exp(x)",
        "atan(exp(x)^-1)",
        "atan(1/exp(x))",
        "0/exp(1/x)",
        "0/exp(x)",
        "exp(x)/exp(x)",
        "atan(1/exp(-1000))",
        "atan(1/exp(-1000))+x",
        "ln(exp(x))",
        "ln(1+exp(x))",
        "exp(x)^2/exp(x)",
        "1/(1+exp(-x))",
        "x^(1/10)+1",
    ] {
        v.push(special(e, &[0.0]));
    }
    v
}

/// Pairs of expressions for the same function: every definite result must
/// agree.
fn equivalent() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = Vec::new();
    for a in [1e6, 1e9, 1e12] {
        for d in [0.0625, 1.0, 1000.0] {
            let b = a + d;
            v.push((format!("(x-{a})*(x-{b})"), format!("(x-{a})*((x-{a})-{d})")));
        }
    }
    for (p, q) in [
        ("exp(x)^-1", "1/exp(x)"),
        ("atan(exp(x)^-1)", "atan(1/exp(x))"),
        ("ln(exp(x))", "x"),
        ("exp(x)/exp(x)", "1"),
        ("0/exp(x)", "0"),
        ("0*x", "0"),
        ("x-x", "0"),
        ("sin(x)-sin(x)", "0"),
        ("0*e^x", "0"),
        ("x^2/x^2", "x/x"),
        ("0/x^2", "0/x"),
        ("exp(x)^2/exp(x)", "exp(x)"),
        ("1/(1+exp(-x))", "exp(x)/(exp(x)+1)"),
    ] {
        v.push((p.into(), q.into()));
    }
    v
}
