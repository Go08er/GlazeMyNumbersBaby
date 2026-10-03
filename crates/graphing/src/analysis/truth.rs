//! The truth about a function, for checking what analysis says about it
//! (the runtime gate in [`super::verify`] and the adversarial sweep,
//! `examples/sweep.rs`).
//!
//! * A reference evaluator over the parsed tree ([`reval`]): the compiled
//!   program's operations, rounding and all, while every operand is a
//!   double; an intermediate that underflows or overflows a double stays a
//!   nonzero, finite number ([`Xf`], a double with an unbounded exponent),
//!   so e^(−800) is not 0 and 1/e^(−800) is defined.
//! * A first-order bound on the rounding of each operation at a point,
//!   propagated through the tree ([`eb`]).
//!
//! Nothing here depends on what the analysis engine estimates (its scales,
//! noise levels or tolerances): a claim is judged by the function alone.

#![allow(missing_docs)]

use std::f64::consts::{LN_2, LN_10};

use crate::ast::{BinOp, Expr, Func};
use crate::compile::{CompileOptions, DEFAULT_VARIABLE_VALUE};
use crate::functions::{self as fns, TrigUnit};

/// `e` with every slider variable replaced by its value in `opts`.
pub fn bind_variables(e: &Expr, opts: &CompileOptions<'_>) -> Expr {
    let b = |a: &Expr| Box::new(bind_variables(a, opts));
    match e {
        Expr::Var(n) => Expr::Num(opts.variables.value(n).unwrap_or(DEFAULT_VARIABLE_VALUE)),
        Expr::Neg(a) => Expr::Neg(b(a)),
        Expr::Degrees(a) => Expr::Degrees(b(a)),
        Expr::Bin(op, x, y) => Expr::Bin(*op, b(x), b(y)),
        Expr::Call(f, args) => {
            Expr::Call(*f, args.iter().map(|a| bind_variables(a, opts)).collect())
        }
        other => other.clone(),
    }
}

// Floats.

/// Floats in order as integers (−0 and +0 coincide).
pub fn to_ord(x: f64) -> i64 {
    let b = x.to_bits() as i64;
    if b < 0 { -(b & i64::MAX) } else { b }
}

pub fn from_ord(o: i64) -> f64 {
    if o < 0 {
        f64::from_bits(o.unsigned_abs() | (1 << 63))
    } else {
        f64::from_bits(o as u64)
    }
}

/// The float `k` representable steps from x.
pub fn nudge(x: f64, k: i64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let y = from_ord(to_ord(x).saturating_add(k));
    if y.is_finite() { y } else { x }
}

/// The spacing of the floats at x.
pub fn ulp(x: f64) -> f64 {
    let a = x.abs();
    if !a.is_finite() {
        return f64::INFINITY;
    }
    let n = f64::from_bits(a.to_bits() + 1);
    if n.is_finite() {
        n - a
    } else {
        a - f64::from_bits(a.to_bits() - 1)
    }
}

/// Floats strictly between a and b, roughly (saturating).
pub fn floats_between(a: f64, b: f64) -> i64 {
    (to_ord(b) as i128 - to_ord(a) as i128)
        .unsigned_abs()
        .min(i64::MAX as u128) as i64
}

// ---------------------------------------------------------------------------
// The reference evaluator.

/// m · 2ᵉ with ½ ≤ |m| < 1 (or m = 0): a double with an unbounded exponent.
#[derive(Clone, Copy, Debug)]
pub struct Xf {
    pub m: f64,
    pub e: i64,
}

pub fn frexp(v: f64) -> (f64, i64) {
    if v == 0.0 || !v.is_finite() {
        return (v, 0);
    }
    let bits = v.to_bits();
    let ex = ((bits >> 52) & 0x7ff) as i64;
    if ex == 0 {
        let (m, e) = frexp(v * 18446744073709551616.0);
        return (m, e - 64);
    }
    (
        f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52)),
        ex - 1022,
    )
}

pub fn ldexp(m: f64, e: i64) -> f64 {
    if m == 0.0 {
        return m;
    }
    if e > 2200 {
        return m.signum() * f64::INFINITY;
    }
    if e < -2200 {
        return m.signum() * 0.0;
    }
    let e = e as i32;
    let h = e / 2;
    m * 2f64.powi(h) * 2f64.powi(e - h)
}

impl Xf {
    pub const ZERO: Xf = Xf { m: 0.0, e: 0 };

    pub fn of(v: f64) -> Xf {
        let (m, e) = frexp(v);
        Xf { m, e }
    }
    pub fn norm(m: f64, e: i64) -> Xf {
        let (mm, ee) = frexp(m);
        Xf {
            m: mm,
            e: if mm == 0.0 { 0 } else { e + ee },
        }
    }
    pub fn f(self) -> f64 {
        ldexp(self.m, self.e)
    }
    pub fn is_zero(self) -> bool {
        self.m == 0.0
    }
    pub fn sign(self) -> f64 {
        if self.m == 0.0 { 0.0 } else { self.m.signum() }
    }
    /// Exactly a double (subnormals included): the compiled program's
    /// arithmetic applies, rounding and all.
    pub fn normal(self) -> bool {
        if self.m == 0.0 {
            return true;
        }
        let f = self.f();
        f.is_finite() && f != 0.0 && {
            let b = Xf::of(f);
            b.m == self.m && b.e == self.e
        }
    }
    /// Nonzero but below the doubles (or between subnormals).
    pub fn tiny(self) -> bool {
        self.m != 0.0 && self.e < 0 && !self.normal()
    }
    /// Beyond the largest double.
    pub fn huge(self) -> bool {
        self.e > 1024
    }
    pub fn neg(self) -> Xf {
        Xf {
            m: -self.m,
            e: self.e,
        }
    }
    pub fn abs(self) -> Xf {
        Xf {
            m: self.m.abs(),
            e: self.e,
        }
    }
    pub fn mul(self, o: Xf) -> Xf {
        Xf::norm(self.m * o.m, self.e + o.e)
    }
    pub fn div(self, o: Xf) -> Xf {
        Xf::norm(self.m / o.m, self.e - o.e)
    }
    pub fn add(self, o: Xf) -> Xf {
        if self.m == 0.0 {
            return o;
        }
        if o.m == 0.0 {
            return self;
        }
        let (a, b) = if self.e >= o.e { (self, o) } else { (o, self) };
        let d = a.e - b.e;
        if d > 1100 {
            return a;
        }
        Xf::norm(a.m + ldexp(b.m, -d), a.e)
    }
    /// ln of a positive number.
    pub fn ln(self) -> f64 {
        self.m.ln() + self.e as f64 * LN_2
    }
    pub fn exp(t: f64) -> R {
        if t.is_nan() {
            return R::Undef;
        }
        if (-700.0..=700.0).contains(&t) {
            return R::V(Xf::of(t.exp()));
        }
        let k = (t / LN_2).floor();
        // (Below even this exponent range, e^t is still a positive number:
        // smaller than anything it meets.)
        if k < -4e18 {
            return R::V(Xf {
                m: 0.5,
                e: -(1 << 61),
            });
        }
        if !k.is_finite() || k > 4e18 {
            return R::Unknown;
        }
        R::V(Xf::norm((t - k * LN_2).exp(), k as i64))
    }
    /// A double standing in for it: itself, or the smallest or largest
    /// double of its sign when it is beyond them.
    pub fn proxy(self) -> f64 {
        if self.normal() {
            return self.f();
        }
        if self.e < 0 {
            let f = self.f();
            return if f == 0.0 { self.sign() * 5e-324 } else { f };
        }
        self.sign() * f64::MAX
    }
}

/// The value of an expression at a point: a number (possibly beyond the
/// doubles), undefined there, or not something the reference can tell.
#[derive(Clone, Copy, Debug)]
pub enum R {
    V(Xf),
    Undef,
    Unknown,
}

/// An exponent written as an integer or a ratio of integers, as the
/// compiler reads it (`syntactic_rational`): real-root semantics apply.
pub fn rational(e: &Expr) -> Option<(i32, i32)> {
    pub fn int(e: &Expr) -> Option<i64> {
        match e {
            Expr::Num(v) if *v == v.trunc() && v.abs() < 1e6 => Some(*v as i64),
            Expr::Neg(a) => int(a).map(|v| -v),
            _ => None,
        }
    }
    let (p, q) = match e {
        Expr::Neg(a) => {
            let (p, q) = rational(a)?;
            return Some((-p, q));
        }
        Expr::Bin(BinOp::Div, a, b) => (int(a)?, int(b)?),
        _ => (int(e)?, 1),
    };
    if q == 0 {
        return None;
    }
    let mut g = (p.unsigned_abs(), q.unsigned_abs());
    while g.1 != 0 {
        g = (g.1, g.0 % g.1);
    }
    let g = g.0.max(1) as i64;
    let (mut p, mut q) = (p / g, q / g);
    if q < 0 {
        p = -p;
        q = -q;
    }
    Some((i32::try_from(p).ok()?, i32::try_from(q).ok()?))
}

pub fn one(r: f64) -> R {
    if r.is_nan() {
        R::Undef
    } else if r.is_infinite() {
        R::Unknown
    } else {
        R::V(Xf::of(r))
    }
}

pub fn reval(e: &Expr, x: f64, u: TrigUnit) -> R {
    match e {
        Expr::Num(v) => R::V(Xf::of(*v)),
        Expr::Const(c) => R::V(Xf::of(c.value())),
        Expr::X => R::V(Xf::of(x)),
        Expr::Y => R::Unknown,
        Expr::Var(_) => R::V(Xf::of(DEFAULT_VARIABLE_VALUE)),
        Expr::Degrees(a) => reval(a, x, u),
        Expr::Neg(a) => match reval(a, x, u) {
            R::V(v) => R::V(v.neg()),
            r => r,
        },
        Expr::Bin(BinOp::Pow, a, b) if let Some((p, q)) = rational(b) => {
            pow_rat(reval(a, x, u), p, q)
        }
        Expr::Bin(op, a, b) => match (reval(a, x, u), reval(b, x, u)) {
            (R::Undef, _) | (_, R::Undef) => R::Undef,
            (R::V(va), R::V(vb)) => bin(*op, va, vb),
            _ => R::Unknown,
        },
        Expr::Call(f, args) => {
            let mut vs = Vec::with_capacity(args.len());
            let mut unknown = false;
            for a in args {
                match reval(a, x, u) {
                    R::Undef => return R::Undef,
                    R::Unknown => unknown = true,
                    R::V(v) => vs.push(v),
                }
            }
            if unknown {
                R::Unknown
            } else {
                call(*f, &vs, u)
            }
        }
    }
}

pub fn bin(op: BinOp, a: Xf, b: Xf) -> R {
    match op {
        BinOp::Pow => return pow_real(a, b),
        BinOp::Div if b.is_zero() => return R::Undef,
        _ => {}
    }
    if a.normal() && b.normal() {
        let (x, y) = (a.f(), b.f());
        let r = match op {
            BinOp::Add => x + y,
            BinOp::Sub => x - y,
            BinOp::Mul => x * y,
            _ => x / y,
        };
        let exact0 = r == 0.0 && (matches!(op, BinOp::Add | BinOp::Sub) || x == 0.0 || y == 0.0);
        if r.is_finite() && (r != 0.0 || exact0) {
            return R::V(Xf::of(r));
        }
    }
    R::V(match op {
        BinOp::Add => a.add(b),
        BinOp::Sub => a.add(b.neg()),
        BinOp::Mul => a.mul(b),
        _ => a.div(b),
    })
}

/// b^(p/q), with the compiler's integer and real-root powers.
pub fn pow_rat(b: R, p: i32, q: i32) -> R {
    let b = match b {
        R::V(b) => b,
        r => return r,
    };
    if b.is_zero() {
        return match p {
            ..0 => R::Undef,
            0 => R::V(Xf::of(1.0)),
            _ => R::V(Xf::ZERO),
        };
    }
    if b.normal() {
        let bf = b.f();
        let r = if q == 1 {
            fns::pow_int(bf, p)
        } else {
            fns::pow_rational(bf, p, q)
        };
        if r.is_nan() {
            return R::Undef;
        }
        if r.is_finite() && r != 0.0 {
            return R::V(Xf::of(r));
        }
    }
    if b.sign() < 0.0 && q % 2 == 0 {
        return R::Undef;
    }
    let t = (p as f64 / q as f64) * b.abs().ln();
    match Xf::exp(t) {
        R::V(m) if b.sign() < 0.0 && p % 2 != 0 => R::V(m.neg()),
        r => r,
    }
}

/// a^b for a computed exponent (`fns::pow`, so 0⁰ = 1).
pub fn pow_real(a: Xf, b: Xf) -> R {
    if a.normal() && b.normal() {
        let (x, y) = (a.f(), b.f());
        let r = fns::pow(x, y);
        if r.is_nan() {
            return R::Undef;
        }
        if r.is_finite() && (r != 0.0 || x == 0.0) {
            return R::V(Xf::of(r));
        }
    }
    if a.is_zero() {
        return if b.sign() < 0.0 {
            R::Undef
        } else if b.is_zero() {
            R::V(Xf::of(1.0))
        } else {
            R::V(Xf::ZERO)
        };
    }
    let y = b.f();
    if !y.is_finite() {
        return R::Unknown;
    }
    if a.sign() < 0.0 {
        if y != y.trunc() {
            return R::Undef;
        }
        let odd = y.abs() < 9007199254740992.0 && y % 2.0 != 0.0;
        return match Xf::exp(y * a.abs().ln()) {
            R::V(m) if odd => R::V(m.neg()),
            r => r,
        };
    }
    Xf::exp(y * a.ln())
}

pub fn root_x(x: Xf, n: Xf) -> R {
    if x.normal() && n.normal() {
        let r = fns::root(x.f(), n.f());
        if r.is_nan() {
            return R::Undef;
        }
        if r.is_finite() && (r != 0.0 || x.is_zero()) {
            return R::V(Xf::of(r));
        }
    }
    let nf = n.f();
    if nf == 0.0 || !nf.is_finite() {
        return R::Undef;
    }
    if x.is_zero() {
        return if nf < 0.0 { R::Undef } else { R::V(Xf::ZERO) };
    }
    let odd = nf.fract() == 0.0 && nf.abs() < 9e15 && (nf % 2.0).abs() == 1.0;
    if x.sign() < 0.0 && !odd {
        return R::Undef;
    }
    match Xf::exp(x.abs().ln() / nf) {
        R::V(m) if x.sign() < 0.0 => R::V(m.neg()),
        r => r,
    }
}

pub fn trig_factor(u: TrigUnit) -> f64 {
    u.to_radians_factor()
}

pub fn call(f: Func, v: &[Xf], u: TrigUnit) -> R {
    use Func::*;
    let a = v[0];
    let c = trig_factor(u);
    match f {
        Exp => {
            let t = a.f();
            if t.is_finite() {
                Xf::exp(t)
            } else {
                R::Unknown
            }
        }
        Ln | Log => {
            if a.sign() <= 0.0 {
                return R::Undef;
            }
            if a.normal() {
                return one(if f == Ln {
                    fns::ln(a.f())
                } else {
                    fns::log10(a.f())
                });
            }
            let l = a.ln();
            R::V(Xf::of(if f == Ln { l } else { l / LN_10 }))
        }
        Sqrt => {
            if a.sign() < 0.0 {
                R::Undef
            } else if a.normal() {
                one(a.f().sqrt())
            } else {
                let (m, e) = if a.e % 2 != 0 {
                    (a.m * 2.0, a.e - 1)
                } else {
                    (a.m, a.e)
                };
                R::V(Xf::norm(m.sqrt(), e / 2))
            }
        }
        Cbrt => {
            if a.normal() {
                one(a.f().cbrt())
            } else {
                let r = a.e.rem_euclid(3);
                R::V(Xf::norm((a.m * 2f64.powi(r as i32)).cbrt(), (a.e - r) / 3))
            }
        }
        Abs => R::V(a.abs()),
        Sign => R::V(Xf::of(a.sign())),
        Floor | Ceil | Round => {
            if a.normal() {
                let t = a.f();
                return one(match f {
                    Floor => t.floor(),
                    Ceil => t.ceil(),
                    _ => fns::round(t),
                });
            }
            if a.huge() {
                return R::V(a);
            }
            R::V(Xf::of(match f {
                Floor if a.sign() < 0.0 => -1.0,
                Ceil if a.sign() > 0.0 => 1.0,
                _ => 0.0,
            }))
        }
        Sin | Tan | Cos | Sec | Csc | Cot => {
            if a.huge() {
                return R::Unknown;
            }
            if a.tiny() {
                let t = a.mul(Xf::of(c));
                return R::V(match f {
                    Sin | Tan => t,
                    Cos | Sec => Xf::of(1.0),
                    _ => Xf::of(1.0).div(t),
                });
            }
            let t = a.f();
            one(match f {
                Sin => fns::sin_u(t, u),
                Cos => fns::cos_u(t, u),
                Tan => fns::tan_u(t, u),
                Sec => fns::sec_u(t, u),
                Csc => fns::csc_u(t, u),
                _ => fns::cot_u(t, u),
            })
        }
        Sinh | Cosh | Tanh => {
            if a.tiny() {
                return R::V(if f == Cosh { Xf::of(1.0) } else { a });
            }
            let t = a.f();
            if t.abs() > 700.0 {
                if f == Tanh {
                    return R::V(Xf::of(a.sign()));
                }
                if !t.is_finite() {
                    return R::Unknown;
                }
                return match Xf::exp(t.abs()) {
                    R::V(m) => {
                        let h = Xf { m: m.m, e: m.e - 1 };
                        R::V(if f == Sinh && t < 0.0 { h.neg() } else { h })
                    }
                    r => r,
                };
            }
            one(match f {
                Sinh => t.sinh(),
                Cosh => t.cosh(),
                _ => t.tanh(),
            })
        }
        Sech | Csch => {
            if f == Csch && a.is_zero() {
                return R::Undef;
            }
            if a.tiny() {
                return R::V(if f == Sech {
                    Xf::of(1.0)
                } else {
                    Xf::of(1.0).div(a)
                });
            }
            let t = a.f();
            if t.abs() > 700.0 {
                if !t.is_finite() {
                    return R::Unknown;
                }
                return match Xf::exp(-t.abs()) {
                    R::V(m) => {
                        let d = Xf { m: m.m, e: m.e + 1 };
                        R::V(if f == Csch && t < 0.0 { d.neg() } else { d })
                    }
                    r => r,
                };
            }
            one(if f == Sech {
                fns::sech(t)
            } else {
                fns::csch(t)
            })
        }
        Coth => {
            if a.is_zero() {
                R::Undef
            } else if a.tiny() {
                R::V(Xf::of(1.0).div(a))
            } else {
                one(fns::coth(a.proxy()))
            }
        }
        Asinh | Acosh if a.huge() => {
            if f == Acosh && a.sign() < 0.0 {
                return R::Undef;
            }
            let l = LN_2 + a.abs().ln();
            R::V(Xf::of(if a.sign() < 0.0 { -l } else { l }))
        }
        Asin | Acos | Atan | Asec | Acsc | Acot | Asinh | Acosh | Atanh | Asech | Acsch | Acoth => {
            let p = a.proxy();
            one(match f {
                Asin => fns::asin_u(p, u),
                Acos => fns::acos_u(p, u),
                Atan => fns::atan_u(p, u),
                Asec => fns::asec_u(p, u),
                Acsc => fns::acsc_u(p, u),
                Acot => fns::acot_u(p, u),
                Asinh => p.asinh(),
                Acosh => p.acosh(),
                Atanh => fns::atanh(p),
                Asech => fns::asech(p),
                Acsch => fns::acsch(p),
                _ => fns::acoth(p),
            })
        }
        Factorial | DoubleFactorial => {
            if !a.normal() {
                return R::Unknown;
            }
            one(if f == Factorial {
                fns::factorial(a.f())
            } else {
                fns::double_factorial(a.f())
            })
        }
        Root => root_x(a, v[1]),
        LogBase => {
            let (b, t) = (a, v[1]);
            if b.normal() && t.normal() {
                one(fns::log_base(b.f(), t.f()))
            } else if b.sign() <= 0.0 || t.sign() <= 0.0 || (b.normal() && b.f() == 1.0) {
                R::Undef
            } else {
                R::V(Xf::of(t.ln() / b.ln()))
            }
        }
        Mod | NCr | NPr => {
            if !v.iter().all(|t| t.normal()) {
                return R::Unknown;
            }
            let (p, q) = (a.f(), v[1].f());
            one(match f {
                Mod => fns::modulo(p, q),
                NCr => fns::ncr(p, q),
                _ => fns::npr(p, q),
            })
        }
        Min | Max => {
            let mut best = v[0];
            for &t in &v[1..] {
                let (bp, tp) = (best.proxy(), t.proxy());
                if (f == Min && tp < bp) || (f == Max && tp > bp) {
                    best = t;
                }
            }
            R::V(best)
        }
    }
}

// ---------------------------------------------------------------------------
// Rounding bound.

/// The exact rounding error of the double sum s = a ⊕ b (Knuth's TwoSum).
pub fn two_sum_err(a: f64, b: f64, s: f64) -> f64 {
    let bv = s - a;
    let av = s - bv;
    ((a - av) + (b - bv)).abs()
}

/// g's change when its argument a moves by up to ea. a ± ea can round
/// back to a when ea is below a's own spacing (x − 1000 is off by
/// 5·10⁻¹⁴ but x ± that is x): then step a few floats and scale, which is
/// g′·ea for a smooth g. A jump (floor) is taken whole if within reach.
pub fn change(g: &dyn Fn(f64) -> f64, a: f64, ea: f64, r: f64, jumps: bool) -> f64 {
    if ea == 0.0 {
        return 0.0;
    }
    let h = ea.max(2.0 * ulp(a));
    let mut d = 0.0f64;
    for t in [a - h, a + h] {
        let v = g(t);
        if !v.is_finite() {
            return f64::INFINITY;
        }
        d = d.max((v - r).abs());
    }
    if jumps {
        return d;
    }
    // (An uncertain argument never makes a smooth result certain: the
    // change can underflow, (x − 1) + 1 squared beside 0, but is not 0.)
    let s = d * (ea / h);
    if s == 0.0 && (d > 0.0 || r == 0.0) {
        f64::from_bits(1)
    } else {
        s
    }
}

/// g(a) where a is known to within ea, and how far the double result can
/// be from g at the exact a: g's change across [a − ea, a + ea] plus the
/// rounding of g itself (k units in the last place, none if `exact`).
pub fn unary_err(
    g: &dyn Fn(f64) -> f64,
    a: f64,
    ea: f64,
    k: f64,
    exact: bool,
    jumps: bool,
) -> (f64, f64) {
    let r = g(a);
    if !r.is_finite() || !ea.is_finite() {
        return (r, f64::INFINITY);
    }
    // (Never rounded away: half the least subnormal is 0.)
    let round = if exact {
        0.0
    } else {
        (k * ulp(r)).max(f64::from_bits(1))
    };
    (r, change(g, a, ea, r, jumps) + round)
}

pub fn binary_err(
    g: &dyn Fn(f64, f64) -> f64,
    (a, ea): (f64, f64),
    (b, eb): (f64, f64),
    k: f64,
) -> (f64, f64) {
    let r = g(a, b);
    if !r.is_finite() || !ea.is_finite() || !eb.is_finite() {
        return (r, f64::INFINITY);
    }
    let d = change(&|s| g(s, b), a, ea, r, false) + change(&|t| g(a, t), b, eb, r, false);
    (
        r,
        d + (k * ulp(r)).max(if k > 0.0 { f64::from_bits(1) } else { 0.0 }),
    )
}

/// The compiled program's unary function (`compile::Fn1::apply`).
pub fn fn1(f: Func, u: TrigUnit) -> impl Fn(f64) -> f64 {
    use Func::*;
    move |x: f64| match f {
        Sin => fns::sin_u(x, u),
        Cos => fns::cos_u(x, u),
        Tan => fns::tan_u(x, u),
        Sec => fns::sec_u(x, u),
        Csc => fns::csc_u(x, u),
        Cot => fns::cot_u(x, u),
        Asin => fns::asin_u(x, u),
        Acos => fns::acos_u(x, u),
        Atan => fns::atan_u(x, u),
        Asec => fns::asec_u(x, u),
        Acsc => fns::acsc_u(x, u),
        Acot => fns::acot_u(x, u),
        Sinh => x.sinh(),
        Cosh => x.cosh(),
        Tanh => x.tanh(),
        Sech => fns::sech(x),
        Csch => fns::csch(x),
        Coth => fns::coth(x),
        Asinh => x.asinh(),
        Acosh => x.acosh(),
        Atanh => fns::atanh(x),
        Asech => fns::asech(x),
        Acsch => fns::acsch(x),
        Acoth => fns::acoth(x),
        Sqrt => x.sqrt(),
        Cbrt => x.cbrt(),
        Log => fns::log10(x),
        Ln => fns::ln(x),
        Exp => x.exp(),
        Abs => x.abs(),
        Floor => x.floor(),
        Ceil => x.ceil(),
        Round => fns::round(x),
        Sign => fns::sign(x),
        Factorial => fns::factorial(x),
        DoubleFactorial => fns::double_factorial(x),
        _ => f64::NAN,
    }
}

pub fn fn2(f: Func) -> impl Fn(f64, f64) -> f64 {
    move |a: f64, b: f64| match f {
        Func::Root => fns::root(a, b),
        Func::LogBase => fns::log_base(a, b),
        Func::Mod => fns::modulo(a, b),
        Func::NCr => fns::ncr(a, b),
        Func::NPr => fns::npr(a, b),
        Func::Min if !a.is_nan() && !b.is_nan() => a.min(b),
        Func::Max if !a.is_nan() && !b.is_nan() => a.max(b),
        _ => f64::NAN,
    }
}

/// Is the library result exact here (no rounding at all)? Special values
/// the functions return exactly (sin 0, cos 0, ln 1, quarter turns in
/// degrees and grads) and the functions that never round.
pub fn exact_at(f: Func, a: f64, r: f64, u: TrigUnit) -> bool {
    use Func::*;
    match f {
        Abs | Floor | Ceil | Round | Sign => true,
        Sqrt => r.mul_add(r, -a) == 0.0,
        Cbrt => r * r * r == a,
        Sin | Tan | Cos | Sec | Csc | Cot if u != TrigUnit::Radians => {
            a.rem_euclid(u.full_turn() / 4.0) == 0.0
        }
        Sin | Tan | Asin | Atan | Sinh | Tanh | Asinh | Atanh => a == 0.0 && r == 0.0,
        Cos | Cosh | Exp => a == 0.0 && r == 1.0,
        Ln | Log => a == 1.0 && r == 0.0,
        Acos => a == 1.0 && r == 0.0,
        _ => false,
    }
}

/// f's value at the float x as the compiled program computes it, and a
/// bound on how far that is from the expression's exact value at x: each
/// operation's rounding, propagated to first order through the tree. This
/// is what the rounding of f's terms can do at x (the expanded (x − 1)⁴ is
/// ±4·10⁻¹⁶ beside its zero though neighbouring floats all agree).
pub fn eb(e: &Expr, x: f64, u: TrigUnit) -> (f64, f64) {
    pub const INF: f64 = f64::INFINITY;
    match e {
        Expr::Num(v) => (*v, if v.fract() == 0.0 { 0.0 } else { 0.5 * ulp(*v) }),
        Expr::Const(c) => (c.value(), 0.5 * ulp(c.value())),
        Expr::X => (x, 0.0),
        Expr::Y => (f64::NAN, INF),
        Expr::Var(_) => (DEFAULT_VARIABLE_VALUE, 0.0),
        Expr::Degrees(a) => eb(a, x, u),
        Expr::Neg(a) => {
            let (v, err) = eb(a, x, u);
            (-v, err)
        }
        Expr::Bin(BinOp::Pow, a, b) if let Some((p, q)) = rational(b) => {
            let (va, ea) = eb(a, x, u);
            let g = move |t: f64| {
                if q == 1 {
                    fns::pow_int(t, p)
                } else {
                    fns::pow_rational(t, p, q)
                }
            };
            let r = g(va);
            // (An underflowed square is not exact, whatever fma says.)
            let exact = q == 1
                && (p == 0
                    || p == 1
                    || (p == 2 && (r != 0.0 || va == 0.0) && va.mul_add(va, -r) == 0.0));
            let k = if q == 1 && p.abs() <= 2 {
                0.5
            } else {
                p.unsigned_abs().max(2) as f64
            };
            let (r, err) = unary_err(&g, va, ea, k, exact, false);
            // b^(p/q) for q > 1 raises to the rounded p/q: |r·ln b| times
            // that rounding (5 ulps of x^(2/3) at 3·10¹²).
            let expo = if q == 1 || va == 0.0 {
                0.0
            } else {
                (r * va.abs().ln()).abs() * ulp(p as f64 / q as f64)
            };
            (r, err + expo)
        }
        Expr::Bin(op, a, b) => {
            let ((va, ea), (vb, ebb)) = (eb(a, x, u), eb(b, x, u));
            if *op == BinOp::Pow {
                return binary_err(&|s, t| fns::pow(s, t), (va, ea), (vb, ebb), 2.0);
            }
            let r = match op {
                BinOp::Add => va + vb,
                BinOp::Sub => va - vb,
                BinOp::Mul => va * vb,
                _ => fns::div(va, vb),
            };
            if !r.is_finite() || !ea.is_finite() || !ebb.is_finite() {
                return (r, INF);
            }
            let err = match op {
                BinOp::Add => ea + ebb + two_sum_err(va, vb, r),
                BinOp::Sub => ea + ebb + two_sum_err(va, -vb, r),
                BinOp::Mul => {
                    let under = if r == 0.0 && va != 0.0 && vb != 0.0 {
                        f64::from_bits(1)
                    } else {
                        0.0
                    };
                    let e = va.abs() * ebb
                        + vb.abs() * ea
                        + ea * ebb
                        + va.mul_add(vb, -r).abs()
                        + under;
                    // (Nor does the bound itself: 10⁻⁶ times an underflowed
                    // e⁻¹⁰⁰⁰'s error is no reason to call the product exact.)
                    if e == 0.0 && ((ea > 0.0 && vb != 0.0) || (ebb > 0.0 && va != 0.0)) {
                        f64::from_bits(1)
                    } else {
                        e
                    }
                }
                _ => {
                    if ebb >= vb.abs() {
                        return (r, INF);
                    }
                    let under = if r == 0.0 && va != 0.0 {
                        f64::from_bits(1)
                    } else {
                        0.0
                    };
                    let e = (va.abs() * ebb + vb.abs() * ea) / (vb.abs() * (vb.abs() - ebb))
                        + ((-r).mul_add(vb, va) / vb).abs()
                        + under;
                    if e == 0.0 && (ea > 0.0 || (ebb > 0.0 && va != 0.0)) {
                        f64::from_bits(1)
                    } else {
                        e
                    }
                }
            };
            (r, err)
        }
        Expr::Call(f, args) => {
            let vs: Vec<(f64, f64)> = args.iter().map(|a| eb(a, x, u)).collect();
            match f {
                Func::Root
                | Func::LogBase
                | Func::Mod
                | Func::NCr
                | Func::NPr
                | Func::Min
                | Func::Max => {
                    let k = if matches!(f, Func::Min | Func::Max) {
                        0.0
                    } else if matches!(f, Func::NCr | Func::NPr) {
                        64.0
                    } else {
                        2.0
                    };
                    let g = fn2(*f);
                    let mut acc = vs[0];
                    for &next in &vs[1..] {
                        acc = binary_err(&g, acc, next, k);
                    }
                    acc
                }
                _ => {
                    let (a, ea) = vs[0];
                    let g = fn1(*f, u);
                    let r = g(a);
                    let k = if matches!(f, Func::Factorial | Func::DoubleFactorial) {
                        64.0
                    } else {
                        1.0
                    };
                    let jumps = matches!(f, Func::Floor | Func::Ceil | Func::Round | Func::Sign);
                    unary_err(&g, a, ea, k, exact_at(*f, a, r, u), jumps)
                }
            }
        }
    }
}
