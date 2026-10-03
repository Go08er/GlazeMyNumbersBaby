//! An independent consistency check of what function analysis shows (not
//! run in CI).
//!
//! `cargo run --release -p graphing --example oracle [-- FILTER] [--dump EXPR]`
//!
//! The sweep (`examples/sweep.rs`) runs the runtime gate's own checks, so a
//! clean sweep can't vouch for those checks. This one shares none of them:
//! it reads only the public analysis result (what the panel shows, and the
//! numbers behind it), the compiled program, and the reference evaluator
//! `analysis::truth::reval` for its *values* (a double with an unbounded
//! exponent where an intermediate leaves the doubles). Nothing from
//! `analysis::verify` is used: no noise estimate, rounding bound, sample
//! set, rule or tolerance of the gate's.
//!
//! The pool is the sweep's (its bases under the same transforms, its
//! hand-written families, undefined points and equivalence partners), the
//! cases of review 10, and more of their shapes. Each *definite* answer is
//! checked against f sampled densely near 0, log-spaced out to 10¹⁵,
//! around the value of every x-free subtree of the expression (and its
//! negative), and around every reported feature. A contradiction is
//! reported by class; the example exits with status 1 if there is one.
//!
//! Tolerances, all of them:
//!
//! * `tol(x)`: 4 ulp of f(x) plus the largest change of f over the four
//!   floats either side of x. Values closer than that are the same.
//! * A point within 8 ulp of a reported end, excluded point or pole is the
//!   precision of that point, not a claim about it.
//! * Values (range bounds, extremum values, the y-intercept) are only
//!   different if the panel's own formatter (`format_nonzero`) also writes
//!   them differently. Shapes (domain, zeros, parity, period,
//!   monotonicity, asymptotes) get no such allowance.
//! * A reported zero or zero value holds if f is exactly 0 there, changes
//!   sign within four floats, or is no bigger than four times its own change
//!   over those floats (a zero between floats). Beyond the doubles this is
//!   judged on the reference's exact sign and magnitude.
//! * A found crossing, turn or pole matches a reported one within 10⁻⁶
//!   relative (the panel's 6 digits), or anywhere between the samples that
//!   bracket it.
//! * A compiled value and the reference's agree within 8 ulp; where they
//!   don't (or one is undefined and the other not) that is reported as
//!   `evaluator-disagrees`, and the point is used for nothing else.

use std::cmp::Ordering;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering as AtOrd};

use graphing::analysis::format::format_nonzero;
use graphing::analysis::truth::{R, Xf, reval};
use graphing::analysis::{
    AnalysisError, Family, Interval, KeyGraphFeatures, Monotonicity, Parity, Periodicity, analyze,
    flags,
};
use graphing::ast::Expr;
use graphing::compile::{CompileOptions, Program};
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
            "{}(below 2^-2^40)",
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
    /// The reference's magnitude saturated (e^t for t below about −10¹⁸
    /// is one sentinel): its sign holds, its size can't be compared.
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
    prog: Program,
    unit: TrigUnit,
    /// `tol` and `res` by x (each takes up to a hundred evaluations).
    memo: std::cell::RefCell<std::collections::HashMap<(u64, bool), f64>>,
}

impl F {
    fn at(&self, x: f64) -> Val {
        let c = self.prog.eval(x, 0.0);
        let r = reval(&self.ast, x, self.unit);
        let mk = |k, v: f64, xf: Xf| Val {
            k,
            v,
            xf,
            c,
            sat: xf.e.unsigned_abs() > 1 << 40,
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
                } else if p == c
                    || (c.is_finite() && p.is_finite() && (c - p).abs() <= 8.0 * ulp(m) + 1e-9 * m)
                    // Below the normal doubles either way: the reference's
                    // exact value is what the comparisons use.
                    || (c.abs() < f64::MIN_POSITIVE && p.abs() < f64::MIN_POSITIVE)
                    || (c.is_infinite() && (p == c || xf.huge() && xf.sign() == c.signum()))
                {
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
            R::Unknown => {
                if c.is_finite() {
                    mk(K::Def, c, Xf::of(c))
                } else {
                    mk(K::Unknown, c, Xf::ZERO)
                }
            }
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
        let v = match reval(e, x, self.unit) {
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
                && fmt(c) == fmt(z)
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

fn definite(k: &KeyGraphFeatures, flag: u32) -> bool {
    k.too_complex_features & flag == 0
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
    let w = 1e-6 * x.abs().max(1.0);
    fams.iter()
        .any(|f| family_in(f, lo.min(x - w), hi.max(x + w)))
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

struct Finding {
    class: &'static str,
    expr: String,
    detail: String,
    rank: f64,
}

struct Ctx<'a> {
    expr: &'a str,
    out: Vec<Finding>,
}

impl Ctx<'_> {
    fn fail(&mut self, class: &'static str, detail: String) {
        self.fail_at(class, 0.0, detail);
    }

    /// Keeps the three witnesses per class and function nearest the origin.
    fn fail_at(&mut self, class: &'static str, x: f64, detail: String) {
        self.out.push(Finding {
            class,
            expr: self.expr.to_string(),
            detail,
            rank: x.abs(),
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
    let Ok(prog) = Program::compile(ast, &opts) else {
        return cx.out;
    };
    let f = F {
        ast: ast.clone(),
        prog,
        unit,
        memo: Default::default(),
    };
    let k = analyze(&eq, &opts);
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
                    reval(&f.ast, *x, unit)
                ),
            );
        }
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
                    (y - v0.v).abs() > f.tol(0.0) && fmt(y) != fmt(v0.v)
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
            if (v.v - b).abs() > f.tol(*x) && fmt(v.v) != fmt(b) {
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
            let s = 1e-15 * x.abs().max(1.0);
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
                    (y - v.v).abs() > f.tol(x) && fmt(*y) != fmt(v.v)
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
        // Within the noise of a reported zero: f stays within its tolerance
        // of 0 all the way to it (an expanded (x − 1)⁴ is ±10⁻¹⁶ for a while).
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
        if l == r {
            // An exact zero.
            if zeros_definite && !near(&d.zeros, l, l, l) && !in_noise_of_reported(l) {
                cx.fail_at(
                    "zero-missing",
                    l,
                    format!("f({l:e}) = 0; claimed: {}", k.x_intercept),
                );
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
            if zeros_definite && !near(&d.zeros, l, xs[i - 1], xs[i]) && !in_noise_of_reported(l) {
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
    // Turns: a sample with no higher (lower) one between it and samples
    // beyond f's tolerance below (above) it on either side.
    for (flag, list, sign, name) in [
        (flags::MAXIMA, &d.maxima, 1.0, "maximum"),
        (flags::MINIMA, &d.minima, -1.0, "minimum"),
    ] {
        if !definite(&k, flag) {
            continue;
        }
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
        let mut refined = 0;
        let mut i = 0;
        while i < xs.len() {
            let c = i;
            i += 1;
            if !vals[c].cmp_ok() || !vals[c].v.is_finite() || refined > 300 {
                continue;
            }
            // Skip the rest of a run of equal values.
            while i < xs.len()
                && vals[i].cmp_ok()
                && cmp_xf(vals[i].xf, vals[c].xf) == Ordering::Equal
            {
                i += 1;
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
            if near(&fams, xs[c], xl, xr) || near(&poles, xs[c], xl, xr) {
                continue;
            }
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
            let xm = a + (b - a) / 2.0;
            let vm = f.at(xm);
            if !vm.cmp_ok() || higher(&cv, &vm) || xm.abs() > 1e7 {
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
            if slope {
                continue;
            }
            if near(&fams, xm, xm, xm) || near(&poles, xm, xm, xm) {
                continue;
            }
            cx.fail_at(
                "extremum-missing",
                xm,
                format!(
                    "{name} near {xm:e} (f = {}; {} and {} at {xl:e} and {xr:e}); claimed: {:?}",
                    show(&vm),
                    show(&vals[l]),
                    show(&vals[r]),
                    list.iter().map(|m| (m.0.x, m.1)).collect::<Vec<_>>()
                ),
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
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for c in [100.0, 1000.0, 1234.0, 123456.0] {
        v.push(format!("exp(-(x-{c})^2)+exp(-x^2)"));
        v.push(format!("exp(-(x-{c})^2)+exp(-(x+{c})^2)"));
    }
    v
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--dump") && args.len() >= 2 {
        let opts = CompileOptions::default();
        let eq = Equation::parse(&format!("y={}", args[1])).expect("parses");
        let k = analyze(&eq, &opts);
        println!("{k:#?}");
        for f in check(&args[1], TrigUnit::Radians, &[], false) {
            println!("{}: {}", f.class, f.detail);
        }
        return;
    }
    let filter = args.iter().find(|a| !a.starts_with("--")).cloned();
    let mut pool: Vec<(String, TrigUnit, Vec<f64>, bool)> = Vec::new();
    let mut push = |e: String, u: TrigUnit, c: Vec<f64>, w: bool| {
        if filter.as_ref().is_none_or(|f| e.contains(f.as_str())) {
            pool.push((e, u, c, w));
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
    pool.sort_by(|a, b| a.0.cmp(&b.0).then((a.1 as u8).cmp(&(b.1 as u8))));
    pool.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);

    let started = std::time::Instant::now();
    let next = AtomicUsize::new(0);
    let found: Mutex<Vec<Finding>> = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, AtOrd::Relaxed);
                    let Some((e, u, c, w)) = pool.get(i) else {
                        break;
                    };
                    let mut r = check(e, *u, c, *w);
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
    let mut by: std::collections::BTreeMap<&str, Vec<&Finding>> = Default::default();
    for f in &found {
        by.entry(f.class).or_default().push(f);
    }
    println!(
        "{} functions in {:.1} s; {} contradictions in {} functions",
        pool.len(),
        started.elapsed().as_secs_f64(),
        found.len(),
        {
            let mut e: Vec<&str> = found.iter().map(|f| f.expr.as_str()).collect();
            e.sort();
            e.dedup();
            e.len()
        }
    );
    for (class, fs) in &by {
        let mut exprs: Vec<&str> = fs.iter().map(|f| f.expr.as_str()).collect();
        exprs.sort();
        exprs.dedup();
        println!("\n## {class}: {} in {} functions", fs.len(), exprs.len());
        let mut shown = 0;
        let mut last = "";
        for f in fs.iter() {
            if f.expr == last {
                continue;
            }
            last = &f.expr;
            println!("  y={}: {}", f.expr, f.detail);
            shown += 1;
            if shown >= 60 && !args.iter().any(|a| a == "--all") {
                println!("  … (--all for every one)");
                break;
            }
        }
    }
    if !found.is_empty() {
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
