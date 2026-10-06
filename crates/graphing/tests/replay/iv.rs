//! The replay's own interval arithmetic: MPFR endpoints rounded outward
//! with directed rounding (`Round::Down` for lower ends, `Round::Up` for
//! upper ends), and a three-way decoration in the spirit of IEEE 1788:
//! defined nowhere on the box (`empty`), defined everywhere (`def`),
//! defined and continuous everywhere (`cont`). Written from the graphing
//! language's specification (`docs/ti-conventions.md`, the function
//! conventions in `crate::functions`' doc comments), sharing no code with
//! `graphing::interval`.
//!
//! Every value in one evaluation has the thread's current precision
//! ([`prec`]), so copying a value never rounds it.

use rug::Float;
use rug::float::{Constant, Round};
use rug::ops::{AssignRound, PowAssignRound};
use std::cell::Cell;
use std::cmp::Ordering;

thread_local! {
    static PREC: Cell<u32> = const { Cell::new(160) };
}

/// The working precision in bits.
pub fn prec() -> u32 {
    PREC.get()
}

pub fn set_prec(p: u32) {
    PREC.set(p);
}

/// MPFR's widest exponent range (per thread): e^−10¹⁵ stays a tiny
/// positive number rather than an underflowed 0.
pub fn init() {
    use gmp_mpfr_sys::mpfr;
    unsafe {
        mpfr::set_emin(mpfr::get_emin_min());
        mpfr::set_emax(mpfr::get_emax_max());
    }
}

pub fn fl(v: f64) -> Float {
    Float::with_val(prec(), v)
}

pub fn inf(positive: bool) -> Float {
    fl(if positive {
        f64::INFINITY
    } else {
        f64::NEG_INFINITY
    })
}

fn cp(x: &Float) -> Float {
    Float::with_val(prec(), x)
}

/// `op` applied to a copy of `x`, rounded in direction `r`; with whether
/// the result is exact.
fn ap(x: &Float, r: Round, op: impl Fn(&mut Float, Round) -> Ordering) -> (Float, bool) {
    let mut v = cp(x);
    let o = op(&mut v, r);
    (v, o == Ordering::Equal)
}

fn dn(x: &Float, op: impl Fn(&mut Float, Round) -> Ordering) -> Float {
    nan_to(ap(x, Round::Down, op).0, false)
}

fn up(x: &Float, op: impl Fn(&mut Float, Round) -> Ordering) -> Float {
    nan_to(ap(x, Round::Up, op).0, true)
}

/// A NaN bound (from ∞ − ∞, 0·∞ and the like) as the widest bound.
fn nan_to(v: Float, upper: bool) -> Float {
    if v.is_nan() { inf(upper) } else { v }
}

fn min_f(a: Float, b: Float) -> Float {
    if a <= b { a } else { b }
}

fn max_f(a: Float, b: Float) -> Float {
    if a >= b { a } else { b }
}

/// A decorated interval with strict-sign facts.
///
/// The box a value is enclosed over holds reals only: an infinite end of
/// it is never reached, so 1/x over `[M, +∞]` lies in `(0, 1/M]` — its
/// low end 0 is a limit, not a value. `pos` (`neg`) records that every
/// value is strictly positive (negative) even where `lo` (`hi`) is 0.
#[derive(Clone, Debug)]
pub struct Iv {
    pub lo: Float,
    pub hi: Float,
    /// Defined nowhere on the box.
    pub empty: bool,
    /// Defined everywhere on the box.
    pub def: bool,
    /// Defined and continuous everywhere on the box.
    pub cont: bool,
    /// Every value is > 0.
    pub pos: bool,
    /// Every value is < 0.
    pub neg: bool,
}

impl Iv {
    pub fn new(lo: Float, hi: Float) -> Iv {
        let (pos, neg) = (lo > 0, hi < 0);
        Iv {
            lo,
            hi,
            empty: false,
            def: true,
            cont: true,
            pos,
            neg,
        }
    }

    pub fn point(v: Float) -> Iv {
        Iv::new(v.clone(), v)
    }

    pub fn of(v: f64) -> Iv {
        Iv::point(fl(v))
    }

    pub fn of2(lo: f64, hi: f64) -> Iv {
        Iv::new(fl(lo), fl(hi))
    }

    pub fn empty() -> Iv {
        Iv {
            lo: inf(true),
            hi: inf(false),
            empty: true,
            def: false,
            cont: false,
            pos: false,
            neg: false,
        }
    }

    /// Any real, possibly undefined somewhere.
    pub fn unknown() -> Iv {
        Iv {
            lo: inf(false),
            hi: inf(true),
            empty: false,
            def: false,
            cont: false,
            pos: false,
            neg: false,
        }
    }

    pub fn entire() -> Iv {
        Iv::new(inf(false), inf(true))
    }

    /// With strict-sign facts known of the result added.
    pub fn strict(mut self, pos: bool, neg: bool) -> Iv {
        if !self.empty {
            self.pos |= pos || self.lo > 0;
            self.neg |= neg || self.hi < 0;
        }
        self
    }

    pub fn is_point(&self) -> bool {
        !self.empty && self.lo == self.hi
    }

    pub fn is_exactly(&self, v: f64) -> bool {
        self.is_point() && self.lo == v
    }

    pub fn gt(&self, c: f64) -> bool {
        !self.empty && (self.lo > c || (c == 0.0 && self.pos))
    }

    pub fn lt(&self, c: f64) -> bool {
        !self.empty && (self.hi < c || (c == 0.0 && self.neg))
    }

    pub fn ge(&self, c: f64) -> bool {
        !self.empty && self.lo >= c
    }

    pub fn le(&self, c: f64) -> bool {
        !self.empty && self.hi <= c
    }

    /// 0 may be a value.
    pub fn has0(&self) -> bool {
        !self.empty && self.lo <= 0 && self.hi >= 0 && !self.pos && !self.neg
    }

    pub fn ne0(&self) -> bool {
        self.gt(0.0) || self.lt(0.0)
    }

    pub fn bounded(&self) -> bool {
        !self.empty && self.lo.is_finite() && self.hi.is_finite()
    }

    /// Strictly on side `above` of `c`.
    pub fn beyond(&self, c: f64, above: bool) -> bool {
        if above { self.gt(c) } else { self.lt(c) }
    }

    /// The decoration of `self`, joined with the arguments' (a result is
    /// only as defined and continuous as what it was computed from).
    fn deco(mut self, args: &[&Iv], def: bool, cont: bool) -> Iv {
        if self.empty || args.iter().any(|a| a.empty) {
            return Iv::empty();
        }
        self.def = def && args.iter().all(|a| a.def);
        self.cont = self.def && cont && args.iter().all(|a| a.cont);
        if self.lo.is_nan() || self.hi.is_nan() {
            self.lo = inf(false);
            self.hi = inf(true);
            self.pos = false;
            self.neg = false;
        }
        self
    }

    /// The tighter of two enclosures of the same thing.
    pub fn meet(&self, o: &Iv) -> Iv {
        if self.empty || o.empty {
            return Iv::empty();
        }
        let lo = max_f(self.lo.clone(), o.lo.clone());
        let hi = min_f(self.hi.clone(), o.hi.clone());
        if lo > hi {
            // Disjoint enclosures of one quantity: only possible where it
            // is undefined everywhere; keep the first.
            return self.clone();
        }
        Iv {
            lo,
            hi,
            empty: false,
            def: self.def || o.def,
            cont: self.cont || o.cont,
            pos: self.pos || o.pos,
            neg: self.neg || o.neg,
        }
        .strict(false, false)
    }

    pub fn hull(&self, o: &Iv) -> Iv {
        if self.empty {
            return o.clone();
        }
        if o.empty {
            return self.clone();
        }
        Iv {
            lo: min_f(self.lo.clone(), o.lo.clone()),
            hi: max_f(self.hi.clone(), o.hi.clone()),
            empty: false,
            def: self.def && o.def,
            cont: self.cont && o.cont,
            pos: self.pos && o.pos,
            neg: self.neg && o.neg,
        }
    }

    pub fn with(mut self, def: bool, cont: bool) -> Iv {
        self.def = self.def && def;
        self.cont = self.cont && cont && self.def;
        self
    }

    pub fn mid(&self) -> Float {
        if self.lo.is_infinite() || self.hi.is_infinite() {
            if self.lo.is_finite() {
                return cp(&self.lo);
            }
            if self.hi.is_finite() {
                return cp(&self.hi);
            }
            return fl(0.0);
        }
        Float::with_val(prec(), &self.lo + &self.hi) / 2u32
    }
}

// ------------------------------------------------------------- constants

pub fn pi() -> Iv {
    Iv::new(
        Float::with_val_round(prec(), Constant::Pi, Round::Down).0,
        Float::with_val_round(prec(), Constant::Pi, Round::Up).0,
    )
}

pub fn e() -> Iv {
    let one = fl(1.0);
    Iv::new(
        dn(&one, |v, r| v.exp_round(r)),
        up(&one, |v, r| v.exp_round(r)),
    )
}

/// A decimal written in digits and at most one `.`, enclosed (a point when
/// the double… or binary value at this precision is exact).
pub fn decimal(text: &str) -> Iv {
    let p = prec();
    let mut t = text.to_string();
    if t.starts_with('.') {
        t.insert(0, '0');
    }
    if t.ends_with('.') {
        t.push('0');
    }
    let parse = |r: Round| -> Float {
        let mut v = Float::new(p);
        let src = Float::parse(&t).expect("a decimal");
        v.assign_round(src, r);
        v
    };
    Iv::new(parse(Round::Down), parse(Round::Up))
}

// ------------------------------------------------------------ arithmetic

pub fn neg(a: &Iv) -> Iv {
    if a.empty {
        return Iv::empty();
    }
    Iv {
        lo: Float::with_val(prec(), -&a.hi),
        hi: Float::with_val(prec(), -&a.lo),
        pos: a.neg,
        neg: a.pos,
        ..a.clone()
    }
}

/// Strictly positive.
fn sp(a: &Iv) -> bool {
    a.gt(0.0)
}

/// Strictly negative.
fn sn(a: &Iv) -> bool {
    a.lt(0.0)
}

pub fn add(a: &Iv, b: &Iv) -> Iv {
    if a.empty || b.empty {
        return Iv::empty();
    }
    let p = prec();
    let lo = nan_to(
        Float::with_val_round(p, &a.lo + &b.lo, Round::Down).0,
        false,
    );
    let hi = nan_to(Float::with_val_round(p, &a.hi + &b.hi, Round::Up).0, true);
    // A strictly positive term and a nonnegative one: strictly positive.
    let pos = (sp(a) && b.ge(0.0)) || (sp(b) && a.ge(0.0));
    let neg = (sn(a) && b.le(0.0)) || (sn(b) && a.le(0.0));
    Iv::new(lo, hi).deco(&[a, b], true, true).strict(pos, neg)
}

pub fn sub(a: &Iv, b: &Iv) -> Iv {
    add(a, &neg(b))
}

/// x·y rounded in direction r, with 0·∞ = 0 (the endpoints stand for
/// reals: an exact 0 times any real is 0).
fn mul_r(x: &Float, y: &Float, r: Round) -> Float {
    if x.is_zero() || y.is_zero() {
        return fl(0.0);
    }
    Float::with_val_round(prec(), x * y, r).0
}

/// The strict signs of a product or quotient of strictly signed reals.
fn sign_of_product(a: &Iv, b: &Iv) -> (bool, bool) {
    (
        (sp(a) && sp(b)) || (sn(a) && sn(b)),
        (sp(a) && sn(b)) || (sn(a) && sp(b)),
    )
}

pub fn mul(a: &Iv, b: &Iv) -> Iv {
    if a.empty || b.empty {
        return Iv::empty();
    }
    let (pos, neg) = sign_of_product(a, b);
    let corners = [
        (&a.lo, &b.lo),
        (&a.lo, &b.hi),
        (&a.hi, &b.lo),
        (&a.hi, &b.hi),
    ];
    let mut lo = inf(true);
    let mut hi = inf(false);
    for (x, y) in corners {
        let l = mul_r(x, y, Round::Down);
        let h = mul_r(x, y, Round::Up);
        if l.is_nan() || h.is_nan() {
            // (Unreachable: mul_r makes 0·∞ = 0.) No sign is known of
            // "anything".
            return Iv::entire().deco(&[a, b], true, true);
        }
        lo = min_f(lo, l);
        hi = max_f(hi, h);
    }
    Iv::new(lo, hi).deco(&[a, b], true, true).strict(pos, neg)
}

/// Multiplication by an exact small integer or a constant interval.
pub fn scale(a: &Iv, k: f64) -> Iv {
    mul(a, &Iv::of(k))
}

pub fn div(a: &Iv, b: &Iv) -> Iv {
    if a.empty || b.empty {
        return Iv::empty();
    }
    if b.is_exactly(0.0) {
        return Iv::empty();
    }
    if b.has0() {
        // Undefined where b = 0. With 0 an end of b, where defined b is
        // strictly of one sign: the quotient as for that open interval,
        // not defined throughout.
        if b.lo == 0 && b.hi > 0 {
            return div(a, &b.clone().strict(true, false)).deco(&[a, b], false, false);
        }
        if b.hi == 0 && b.lo < 0 {
            return div(a, &b.clone().strict(false, true)).deco(&[a, b], false, false);
        }
        return Iv::unknown().deco(&[a, b], false, false);
    }
    let (pos, neg) = sign_of_product(a, b);
    // b is strictly of one sign: a 0 end of it is a limit, approached
    // from b's side (x/0⁺ = +∞ for x > 0).
    let mut blo = cp(&b.lo);
    let mut bhi = cp(&b.hi);
    if blo.is_zero() {
        blo = fl(0.0);
    }
    if bhi.is_zero() {
        bhi = -fl(0.0);
    }
    let mut lo = inf(true);
    let mut hi = inf(false);
    for x in [&a.lo, &a.hi] {
        for y in [&blo, &bhi] {
            let q = |r: Round| -> Float {
                if x.is_zero() {
                    return fl(0.0);
                }
                if y.is_infinite() && x.is_finite() {
                    return fl(0.0);
                }
                Float::with_val_round(prec(), x / y, r).0
            };
            let (l, h) = (q(Round::Down), q(Round::Up));
            if l.is_nan() || h.is_nan() {
                // ∞/∞: two unbounded reals; their quotient is anything of
                // the sign of x·y, from 0 out to ∞.
                let s = if (x.is_sign_negative()) == (y.is_sign_negative()) {
                    inf(true)
                } else {
                    inf(false)
                };
                lo = min_f(min_f(lo, fl(0.0)), cp(&s));
                hi = max_f(max_f(hi, fl(0.0)), s);
                continue;
            }
            lo = min_f(lo, l);
            hi = max_f(hi, h);
        }
    }
    Iv::new(lo, hi).deco(&[a, b], true, true).strict(pos, neg)
}

pub fn recip(b: &Iv) -> Iv {
    div(&Iv::of(1.0), b)
}

/// The part of `a` inside `[lo, hi]` (bounds included as asked), and
/// whether `a` stayed inside.
fn restrict(a: &Iv, lo: Option<(f64, bool)>, hi: Option<(f64, bool)>) -> (Option<Iv>, bool) {
    let mut r = a.clone();
    let mut inside = true;
    if let Some((l, closed)) = lo {
        let ok = if closed {
            r.lo >= l
        } else {
            r.lo > l || (l == 0.0 && a.pos)
        };
        if !ok {
            inside = false;
            // Wholly below (a strictly negative a below a closed 0 too).
            if (closed && (r.hi < l || (l == 0.0 && a.neg))) || (!closed && r.hi <= l) {
                return (None, false);
            }
            r.lo = fl(l);
            // Only the points inside the domain count: above an open 0.
            r.pos = !closed && l == 0.0;
        }
    }
    if let Some((h, closed)) = hi {
        let ok = if closed {
            r.hi <= h
        } else {
            r.hi < h || (h == 0.0 && a.neg)
        };
        if !ok {
            inside = false;
            if (closed && (r.lo > h || (h == 0.0 && a.pos))) || (!closed && r.lo >= h) {
                return (None, false);
            }
            r.hi = fl(h);
            r.neg = !closed && h == 0.0;
        }
    }
    (Some(r), inside)
}

fn increasing(a: &Iv, op: impl Fn(&mut Float, Round) -> Ordering + Copy) -> Iv {
    Iv::new(dn(&a.lo, op), up(&a.hi, op))
}

fn decreasing(a: &Iv, op: impl Fn(&mut Float, Round) -> Ordering + Copy) -> Iv {
    Iv::new(dn(&a.hi, op), up(&a.lo, op))
}

/// What a function's strict sign is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Signs {
    /// Not known from the argument's.
    No,
    /// The argument's (an odd increasing function, 0 only at 0).
    Keep,
    /// Always positive.
    Pos,
}

/// A monotone function on a domain `[lo, hi]` (ends open or closed).
fn on_domain(
    a: &Iv,
    lo: Option<(f64, bool)>,
    hi: Option<(f64, bool)>,
    inc: bool,
    op: impl Fn(&mut Float, Round) -> Ordering + Copy,
) -> Iv {
    on_domain_s(a, lo, hi, inc, Signs::No, op)
}

fn on_domain_s(
    a: &Iv,
    lo: Option<(f64, bool)>,
    hi: Option<(f64, bool)>,
    inc: bool,
    signs: Signs,
    op: impl Fn(&mut Float, Round) -> Ordering + Copy,
) -> Iv {
    if a.empty {
        return Iv::empty();
    }
    let (Some(r), inside) = restrict(a, lo, hi) else {
        return Iv::empty();
    };
    let v = if inc {
        increasing(&r, op)
    } else {
        decreasing(&r, op)
    };
    let v = v.deco(&[a], inside, inside);
    match signs {
        Signs::No => v,
        Signs::Keep => v.strict(sp(&r), sn(&r)),
        Signs::Pos => v.strict(true, false),
    }
}

pub fn exp(a: &Iv) -> Iv {
    on_domain_s(a, None, None, true, Signs::Pos, |v, r| v.exp_round(r))
}

pub fn ln(a: &Iv) -> Iv {
    on_domain(a, Some((0.0, false)), None, true, |v, r| v.ln_round(r))
}

pub fn log10(a: &Iv) -> Iv {
    on_domain(a, Some((0.0, false)), None, true, |v, r| v.log10_round(r))
}

pub fn sqrt(a: &Iv) -> Iv {
    on_domain_s(a, Some((0.0, true)), None, true, Signs::Keep, |v, r| {
        v.sqrt_round(r)
    })
}

pub fn cbrt(a: &Iv) -> Iv {
    on_domain_s(a, None, None, true, Signs::Keep, |v, r| v.cbrt_round(r))
}

pub fn atan(a: &Iv) -> Iv {
    on_domain_s(a, None, None, true, Signs::Keep, |v, r| v.atan_round(r))
}

pub fn asin(a: &Iv) -> Iv {
    on_domain_s(
        a,
        Some((-1.0, true)),
        Some((1.0, true)),
        true,
        Signs::Keep,
        |v, r| v.asin_round(r),
    )
}

pub fn acos(a: &Iv) -> Iv {
    on_domain(a, Some((-1.0, true)), Some((1.0, true)), false, |v, r| {
        v.acos_round(r)
    })
}

pub fn sinh(a: &Iv) -> Iv {
    on_domain_s(a, None, None, true, Signs::Keep, |v, r| v.sinh_round(r))
}

pub fn tanh(a: &Iv) -> Iv {
    on_domain_s(a, None, None, true, Signs::Keep, |v, r| v.tanh_round(r))
}

pub fn asinh(a: &Iv) -> Iv {
    on_domain_s(a, None, None, true, Signs::Keep, |v, r| v.asinh_round(r))
}

pub fn acosh(a: &Iv) -> Iv {
    on_domain(a, Some((1.0, true)), None, true, |v, r| v.acosh_round(r))
}

pub fn atanh(a: &Iv) -> Iv {
    on_domain_s(
        a,
        Some((-1.0, false)),
        Some((1.0, false)),
        true,
        Signs::Keep,
        |v, r| v.atanh_round(r),
    )
}

pub fn cosh(a: &Iv) -> Iv {
    if a.empty {
        return Iv::empty();
    }
    let op = |v: &mut Float, r: Round| v.cosh_round(r);
    let v = if a.lo >= 0 {
        increasing(a, op)
    } else if a.hi <= 0 {
        decreasing(a, op)
    } else {
        let m = max_f(up(&a.lo, op), up(&a.hi, op));
        Iv::new(fl(1.0), m)
    };
    v.deco(&[a], true, true).strict(true, false)
}

/// |a|.
pub fn abs(a: &Iv) -> Iv {
    if a.empty {
        return Iv::empty();
    }
    let v = if a.lo >= 0 {
        a.clone()
    } else if a.hi <= 0 {
        neg(a)
    } else {
        let m = max_f(Float::with_val(prec(), -&a.lo), cp(&a.hi));
        Iv::new(fl(0.0), m)
    };
    v.deco(&[a], true, true).strict(sp(a) || sn(a), false)
}

/// Integer power aⁿ, 0 to a power ≤ 0 undefined.
pub fn powi(a: &Iv, n: i64) -> Iv {
    powi_big(a, &rug::Integer::from(n))
}

/// [`powi`] for an n of any size (MPFR's power to an integer, exactly
/// rounded: an n past `i32` was clipped to `i32::MAX`, an odd number).
pub fn powi_big(a: &Iv, n: &rug::Integer) -> Iv {
    if a.empty {
        return Iv::empty();
    }
    if *n == 0 {
        if a.is_exactly(0.0) {
            return Iv::empty();
        }
        let ok = !a.has0();
        return Iv::of(1.0).deco(&[a], ok, ok);
    }
    if *n < 0 {
        return recip(&powi_big(a, &rug::Integer::from(-n)));
    }
    if *n == 1 {
        return a.clone();
    }
    let pw = |x: &Float, r: Round| -> Float {
        let mut v = cp(x);
        v.pow_assign_round(n, r);
        nan_to(v, r == Round::Up)
    };
    let odd = n.is_odd();
    let v = if odd || a.lo >= 0 {
        Iv::new(pw(&a.lo, Round::Down), pw(&a.hi, Round::Up))
    } else if a.hi <= 0 {
        Iv::new(pw(&a.hi, Round::Down), pw(&a.lo, Round::Up))
    } else {
        let m = max_f(pw(&a.lo, Round::Up), pw(&a.hi, Round::Up));
        Iv::new(fl(0.0), m)
    };
    let (pos, neg) = if !odd {
        (sp(a) || sn(a), false)
    } else {
        (sp(a), sn(a))
    };
    v.deco(&[a], true, true).strict(pos, neg)
}

/// x^y for x ≥ 0 (a point exponent `y` interval), increasing or
/// decreasing in x by the sign of y. Defined where x > 0, and at x = 0
/// when y > 0 (value 0).
fn pow_pos(a: &Iv, y: &Iv) -> Iv {
    if a.empty || y.empty {
        return Iv::empty();
    }
    let zero_ok = y.gt(0.0);
    let lo_bound = if zero_ok { (0.0, true) } else { (0.0, false) };
    let (Some(r), inside) = restrict(a, Some(lo_bound), None) else {
        return Iv::empty();
    };
    // Corners of x^y over x ∈ r, y ∈ y: x^y is monotone in each.
    let pw = |x: &Float, e: &Float, rd: Round| -> Float {
        if x.is_zero() {
            return if *e > 0 {
                fl(0.0)
            } else if *e == 0 {
                fl(1.0)
            } else {
                inf(true)
            };
        }
        let mut v = cp(x);
        v.pow_assign_round(e, rd);
        nan_to(v, rd == Round::Up)
    };
    let mut lo = inf(true);
    let mut hi = inf(false);
    for x in [&r.lo, &r.hi] {
        for e in [&y.lo, &y.hi] {
            lo = min_f(lo, pw(x, e, Round::Down));
            hi = max_f(hi, pw(x, e, Round::Up));
        }
    }
    // x^y with y straddling 0 over x straddling 1 is still monotone in
    // each argument, so the corners bound it; x > 0 gives x^y > 0.
    Iv::new(lo, hi)
        .deco(&[a, y], inside, inside)
        .strict(sp(&r), false)
}

/// The real q-th root (q ≥ 2): x ≥ 0 for even q; odd q accepts negative
/// x (MPFR's rootn is exact where the root is).
pub fn rootn(a: &Iv, q: u32) -> Iv {
    let lo = if q.is_multiple_of(2) {
        Some((0.0, true))
    } else {
        None
    };
    on_domain_s(a, lo, None, true, Signs::Keep, move |v, r| {
        v.root_round(q, r)
    })
}

/// ᵠ√a for a degree past `u32` (a typed 9007199254740993, review 13
/// R13-M-04: MPFR's root takes a u32), as sign(a)·exp(ln|a|/q), each step
/// rounded the way of the bound it gives (the magnitude's the other way
/// for a negative end): a negative base only for odd q. Any q ≥ 2,
/// however long (a typed 10³⁰¹ + 1, review 14, R14-M-04): divided by
/// exactly, at as many bits as it has.
pub fn rootn_big(a: &Iv, q: &rug::Integer) -> Iv {
    let lo = if q.is_even() { Some((0.0, true)) } else { None };
    let qf = Float::with_val(prec().max(q.significant_bits()), q);
    let qf = &qf;
    on_domain_s(a, lo, None, true, Signs::Keep, move |v, r| {
        use rug::ops::{DivAssignRound, NegAssign};
        if v.is_zero() {
            return Ordering::Equal;
        }
        let neg = v.is_sign_negative();
        let rm = match (neg, r) {
            (true, Round::Down) => Round::Up,
            (true, _) => Round::Down,
            (false, r) => r,
        };
        v.abs_mut();
        v.ln_round(rm);
        v.div_assign_round(qf, rm);
        v.exp_round(rm);
        if neg {
            v.neg_assign();
        }
        if r == Round::Down {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    })
}

/// a^(p/q) in lowest terms, q > 1, real-root semantics: (ᵠ√a)^p, so a
/// negative base only for odd q, and 0 only to a positive power. p and q
/// of any size (a typed 1000001/3, review 15, R15-M-01; a root's degree
/// of hundreds of digits, review 14, R14-M-04).
pub fn pow_ratio(a: &Iv, p: &rug::Integer, q: &rug::Integer) -> Iv {
    if a.empty {
        return Iv::empty();
    }
    let r = match q.to_u32() {
        Some(q) => rootn(a, q),
        None => rootn_big(a, q),
    };
    if r.empty {
        return Iv::empty();
    }
    powi_big(&r, p)
}

/// The rational p/q enclosed by the working precision, rounded outward
/// (p and q however long).
pub fn ratio(p: &rug::Integer, q: &rug::Integer) -> Iv {
    let r = rug::Rational::from((p.clone(), q.clone()));
    let (lo, _) = Float::with_val_round(prec(), &r, Round::Down);
    let (hi, _) = Float::with_val_round(prec(), &r, Round::Up);
    Iv::new(lo, hi)
}

/// b^e for an exponent that varies with x: a positive base, or 0 to a
/// positive power.
pub fn pow_var(b: &Iv, e: &Iv) -> Iv {
    pow_pos(b, e)
}

/// b^e for a constant exponent that isn't written as a ratio of integers:
/// a negative base only to an integer power, 0 only to a positive power.
pub fn pow_const(b: &Iv, e: &Iv) -> Iv {
    if b.empty || e.empty {
        return Iv::empty();
    }
    // An exponent enclosed by an integer point: that integer power,
    // defined only where the exponent is too (review 17, R17-M-02: a point
    // may still be possibly undefined).
    if e.is_point() && e.lo.is_integer() {
        let n = e.lo.to_f64();
        if n.abs() < 1e9 {
            return powi(b, n as i64).with(e.def, e.cont);
        }
    }
    // An exponent only enclosed, around an integer: whether a negative
    // base is allowed is not decided.
    if !b.empty && b.lo < 0 && !e.empty && floor_f(&e.hi) >= e.lo {
        return Iv::unknown().deco(&[b, e], false, false);
    }
    pow_pos(b, e)
}

// ------------------------------------------------------------ steps

fn floor_f(x: &Float) -> Float {
    let mut v = cp(x);
    v.floor_mut();
    v
}

fn ceil_f(x: &Float) -> Float {
    let mut v = cp(x);
    v.ceil_mut();
    v
}

/// Round half away from zero.
fn round_f(x: &Float) -> Float {
    let mut v = cp(x);
    v.round_mut();
    v
}

/// A step function g: continuous (constant) on the box when g(lo) = g(hi),
/// else a jump inside.
fn step(a: &Iv, g: fn(&Float) -> Float) -> Iv {
    if a.empty {
        return Iv::empty();
    }
    let (l, h) = (g(&a.lo), g(&a.hi));
    let same = l == h && a.lo.is_finite() && a.hi.is_finite();
    Iv::new(l, h).deco(&[a], true, same)
}

/// Whether the box holds no k + `offset` (k ∈ ℤ): a step function jumping
/// there is constant, and differentiable, across it.
pub fn no_jump(a: &Iv, offset: f64) -> bool {
    if a.empty || !a.bounded() {
        return false;
    }
    let s = sub(a, &Iv::of(offset));
    floor_f(&s.hi) < s.lo
}

pub fn floor(a: &Iv) -> Iv {
    step(a, floor_f)
}

pub fn ceil(a: &Iv) -> Iv {
    step(a, ceil_f)
}

pub fn round(a: &Iv) -> Iv {
    step(a, round_f)
}

pub fn sign(a: &Iv) -> Iv {
    if a.empty {
        return Iv::empty();
    }
    let s = |x: &Float| -> f64 {
        if *x > 0 {
            1.0
        } else if *x < 0 {
            -1.0
        } else {
            0.0
        }
    };
    if sp(a) {
        return Iv::of(1.0).deco(&[a], true, true);
    }
    if sn(a) {
        return Iv::of(-1.0).deco(&[a], true, true);
    }
    let (l, h) = (s(&a.lo), s(&a.hi));
    let same = l == h && (l != 0.0 || a.is_point());
    Iv::of2(l, h).deco(&[a], true, same)
}

/// Floored modulo a − b·⌊a/b⌋ (the sign of b), b ≠ 0.
pub fn modulo(a: &Iv, b: &Iv) -> Iv {
    if a.empty || b.empty {
        return Iv::empty();
    }
    let q = div(a, b);
    if q.empty || !q.def {
        return Iv::unknown().deco(&[a, b], false, false);
    }
    let fq = floor(&q);
    if fq.cont && fq.is_point() {
        let v = sub(a, &mul(b, &fq));
        return v.deco(&[a, b], true, true);
    }
    // A jump inside: anywhere between 0 and b.
    let lo = min_f(fl(0.0), cp(&b.lo));
    let hi = max_f(fl(0.0), cp(&b.hi));
    Iv::new(lo, hi).deco(&[a, b], true, false)
}

pub fn min2(a: &Iv, b: &Iv) -> Iv {
    if a.empty || b.empty {
        return Iv::empty();
    }
    Iv::new(min_f(cp(&a.lo), cp(&b.lo)), min_f(cp(&a.hi), cp(&b.hi)))
        .deco(&[a, b], true, true)
        .strict(sp(a) && sp(b), sn(a) || sn(b))
}

pub fn max2(a: &Iv, b: &Iv) -> Iv {
    if a.empty || b.empty {
        return Iv::empty();
    }
    Iv::new(max_f(cp(&a.lo), cp(&b.lo)), max_f(cp(&a.hi), cp(&b.hi)))
        .deco(&[a, b], true, true)
        .strict(sp(a) || sp(b), sn(a) && sn(b))
}

// ------------------------------------------------------------ trig

/// The angle unit: a full turn in the unit (2π, 360, 400).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Radians,
    Degrees,
    Grads,
}

impl Unit {
    pub fn parse(s: &str) -> Option<Unit> {
        match s {
            "radians" => Some(Unit::Radians),
            "degrees" => Some(Unit::Degrees),
            "grads" => Some(Unit::Grads),
            _ => None,
        }
    }

    /// A full turn as an MPFR "u" (0 for radians).
    fn u(self) -> u32 {
        match self {
            Unit::Radians => 0,
            Unit::Degrees => 360,
            Unit::Grads => 400,
        }
    }

    /// One unit in radians (an enclosure).
    pub fn to_rad(self) -> Iv {
        match self {
            Unit::Radians => Iv::of(1.0),
            Unit::Degrees => div(&pi(), &Iv::of(180.0)),
            Unit::Grads => div(&pi(), &Iv::of(200.0)),
        }
    }

    /// One radian in the unit.
    pub fn per_rad(self) -> Iv {
        match self {
            Unit::Radians => Iv::of(1.0),
            Unit::Degrees => div(&Iv::of(180.0), &pi()),
            Unit::Grads => div(&Iv::of(200.0), &pi()),
        }
    }

    /// A quarter turn in the unit (exact in degrees and grads).
    fn quarter(self) -> Option<f64> {
        match self {
            Unit::Radians => None,
            Unit::Degrees => Some(90.0),
            Unit::Grads => Some(100.0),
        }
    }
}

/// sin or cos (`cos`) of a point, in the unit, rounded in direction r;
/// exact where the result is (sin 180° = 0).
fn sc_point(x: &Float, unit: Unit, cos: bool, r: Round) -> Float {
    let mut v = cp(x);
    match (unit, cos) {
        (Unit::Radians, false) => v.sin_round(r),
        (Unit::Radians, true) => v.cos_round(r),
        (u, false) => v.sin_u_round(u.u(), r),
        (u, true) => v.cos_u_round(u.u(), r),
    };
    nan_to(v, r == Round::Up)
}

/// Whether the box `a` (in the unit) may contain a point `phase + k·step`
/// (both in quarter turns), k ∈ ℤ.
fn may_contain(a: &Iv, unit: Unit, phase: f64, step: f64) -> bool {
    // In quarter turns: t = a / quarter.
    let quarter = match unit.quarter() {
        Some(q) => Iv::of(q),
        None => div(&pi(), &Iv::of(2.0)),
    };
    let t = div(a, &quarter);
    if t.empty || !t.lo.is_finite() || !t.hi.is_finite() {
        return true;
    }
    // k from (t − phase)/step.
    let s = div(&sub(&t, &Iv::of(phase)), &Iv::of(step));
    let k_lo = ceil_f(&s.lo);
    let k_hi = floor_f(&s.hi);
    k_lo <= k_hi
}

/// An argument too large to reduce by a period in reasonable time and
/// memory (beyond 2⁶⁵⁵³⁶: e^x at x = 10¹⁵ is 2^(1.4·10¹⁵)); only an
/// exponential far out reaches one. Its sine and cosine are taken as
/// [−1, 1], its tangent unknown.
fn unreducible(a: &Iv) -> bool {
    let bound = Float::with_val(64, 1u32) << 65_536u32;
    let big = |v: &Float| v.is_finite() && *v.as_abs() > bound;
    big(&a.lo) || big(&a.hi)
}

/// sin (`cos` false) or cos over a box, in the unit.
pub fn sin_cos(a: &Iv, unit: Unit, cos: bool) -> Iv {
    if a.empty {
        return Iv::empty();
    }
    if !a.lo.is_finite() || !a.hi.is_finite() || unreducible(a) {
        return Iv::of2(-1.0, 1.0).deco(&[a], true, true);
    }
    // Maxima of sin at 1 + 4k quarter turns, minima at 3 + 4k; cos: 0 + 4k
    // and 2 + 4k.
    let (pmax, pmin) = if cos { (0.0, 2.0) } else { (1.0, 3.0) };
    let mut lo = min_f(
        sc_point(&a.lo, unit, cos, Round::Down),
        sc_point(&a.hi, unit, cos, Round::Down),
    );
    let mut hi = max_f(
        sc_point(&a.lo, unit, cos, Round::Up),
        sc_point(&a.hi, unit, cos, Round::Up),
    );
    if may_contain(a, unit, pmax, 4.0) {
        hi = fl(1.0);
    }
    if may_contain(a, unit, pmin, 4.0) {
        lo = fl(-1.0);
    }
    Iv::new(lo, hi).deco(&[a], true, true)
}

/// tan over a box in the unit: undefined at odd quarter turns.
pub fn tan(a: &Iv, unit: Unit) -> Iv {
    if a.empty {
        return Iv::empty();
    }
    if a.is_point()
        && let Some(q) = unit.quarter()
    {
        let t = Float::with_val(prec(), &a.lo / q);
        if t.is_integer() {
            let k = t.to_f64() as i64;
            // (0 only where the argument is defined: its decoration kept,
            // since a meet with this enclosure takes the better one; review
            // 17. An odd quarter turn is undefined either way.)
            return if k.rem_euclid(2) == 1 {
                Iv::empty()
            } else {
                Iv::of(0.0).deco(&[a], true, true)
            };
        }
    }
    if !a.lo.is_finite() || !a.hi.is_finite() || unreducible(a) || may_contain(a, unit, 1.0, 2.0) {
        return Iv::unknown().deco(&[a], false, false);
    }
    let t = |x: &Float, r: Round| -> Float {
        let mut v = cp(x);
        match unit {
            Unit::Radians => v.tan_round(r),
            u => v.tan_u_round(u.u(), r),
        };
        nan_to(v, r == Round::Up)
    };
    Iv::new(t(&a.lo, Round::Down), t(&a.hi, Round::Up)).deco(&[a], true, true)
}

/// arcsin, arccos or arctan (`which`: 's', 'c', 't') of a box, as an angle
/// in the unit, rounded directly in it (arctan(−∞) is −90° exactly, not
/// −π/2 times 180/π rounded outward).
pub fn ainv_unit(a: &Iv, unit: Unit, which: char) -> Iv {
    let u = unit.u();
    match which {
        's' => on_domain_s(
            a,
            Some((-1.0, true)),
            Some((1.0, true)),
            true,
            Signs::Keep,
            move |v, r| v.asin_u_round(u, r),
        ),
        'c' => on_domain(
            a,
            Some((-1.0, true)),
            Some((1.0, true)),
            false,
            move |v, r| v.acos_u_round(u, r),
        ),
        _ => on_domain_s(a, None, None, true, Signs::Keep, move |v, r| {
            v.atan_u_round(u, r)
        }),
    }
}

/// An angle in radians as the unit.
pub fn angle_out(rad: &Iv, unit: Unit) -> Iv {
    if unit == Unit::Radians {
        rad.clone()
    } else {
        mul(rad, &unit.per_rad())
    }
}

/// arccot with range (0, π): π/2 − arctan x, in radians.
pub fn acot(a: &Iv) -> Iv {
    let half_pi = div(&pi(), &Iv::of(2.0));
    sub(&half_pi, &atan(a))
}
