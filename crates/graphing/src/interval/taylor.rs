//! Interval Taylor-mode evaluation over the expression tree.
//!
//! [`taylor`] returns `c[0..=n]` with `c[k]` an enclosure of `f⁽ᵏ⁾(x)/k!`
//! for every x in the input box (n ≤ 3 is what the certifier uses). `c[0]`
//! carries the decoration: whether f is defined and continuous on the box.
//! The higher coefficients mean something only where f is differentiable;
//! [`derivs_valid`] says when to trust them. Every kink (abs or floor at a
//! jump, a root or rational power at 0) makes them unknown.
//!
//! The tree is the one the parser produced, not a simplified or compiled
//! form: numbers are read back to their exact decimals (each by its own
//! [`crate::ast::Lit`]), and the app's semantics are those of `functions.rs`, with
//! the TI rule for a power whose exponent varies (base > 0).

use super::arith::Interval;
use super::dec::{Dec, DecInterval};
use super::elem;
use crate::ast::{BinOp, Constant, Expr, Func};
use crate::compile::{CompileOptions, DEFAULT_VARIABLE_VALUE, syntactic_rational};
use crate::functions::TrigUnit;

/// What an evaluation needs besides the tree and the box.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    /// Angle unit and slider values.
    pub opts: CompileOptions<'a>,
    /// The value(s) of y, for relations in x and y (no y-derivatives).
    pub y: DecInterval,
    /// Probe for poles: n! at a pole of Γ is [`f64::MAX`, +∞] instead of
    /// undefined, so a result that stays bounded there (1/x!) tells a
    /// removable point from a pole (x!) where the values beside it, all
    /// that doubles reach, can't (x! near −1000 underflows to ±0).
    pub pole_probe: bool,
}

impl<'a> Ctx<'a> {
    pub fn new(opts: CompileOptions<'a>) -> Ctx<'a> {
        Ctx {
            opts,
            y: DecInterval::unknown(),
            pole_probe: false,
        }
    }
}

/// A truncated Taylor series with interval coefficients.
pub type Series = Vec<DecInterval>;

/// Taylor coefficients `c[0..=n]` of `e` in x over the box `x`.
pub fn taylor(e: &Expr, x: Interval, n: usize, ctx: &Ctx<'_>) -> Series {
    let mut s = konst(DecInterval::new(x), n);
    if n >= 1 {
        s[1] = DecInterval::point(1.0);
    }
    ev(e, &s, n, ctx)
}

/// An enclosure of `e` over the box `x` (order 0).
pub fn enclose(e: &Expr, x: Interval, ctx: &Ctx<'_>) -> DecInterval {
    taylor(e, x, 0, ctx)[0]
}

/// True when the coefficients up to `k` are usable on the box: f defined
/// and continuous there and the coefficients bounded and from defined
/// operations.
pub fn derivs_valid(s: &Series, k: usize) -> bool {
    s[0].dec >= Dec::Dac
        && s[1..=k.min(s.len() - 1)]
            .iter()
            .all(|c| c.iv.is_bounded() && !c.is_empty() && c.dec >= Dec::Def)
}

// ------------------------------------------------------------- series

fn konst(c: DecInterval, n: usize) -> Series {
    let mut v = vec![c];
    v.extend((0..n).map(|_| DecInterval::point(0.0)));
    v
}

/// Value known, derivatives not.
fn flat(h0: DecInterval, n: usize) -> Series {
    let mut v = vec![h0];
    v.extend((0..n).map(|_| DecInterval::unknown()));
    v
}

fn pt(v: usize) -> DecInterval {
    DecInterval::point(v as f64)
}

fn add(a: &Series, b: &Series) -> Series {
    a.iter().zip(b).map(|(x, y)| elem::add(x, y)).collect()
}

fn sub(a: &Series, b: &Series) -> Series {
    a.iter().zip(b).map(|(x, y)| elem::sub(x, y)).collect()
}

fn neg(a: &Series) -> Series {
    a.iter().map(elem::neg).collect()
}

fn scale(a: &Series, k: &DecInterval) -> Series {
    a.iter().map(|x| elem::mul(x, k)).collect()
}

fn mul(a: &Series, b: &Series) -> Series {
    let n = a.len();
    (0..n)
        .map(|k| {
            let mut s = elem::mul(&a[0], &b[k]);
            for j in 1..=k {
                s = elem::add(&s, &elem::mul(&a[j], &b[k - j]));
            }
            s
        })
        .collect()
}

fn sqr(a: &Series) -> Series {
    let n = a.len();
    let mut h = vec![elem::sqr(&a[0])];
    for k in 1..n {
        let mut s = DecInterval::point(0.0);
        for j in 0..k {
            if j < k - j {
                s = elem::add(&s, &elem::mul(&a[j], &a[k - j]));
            }
        }
        s = elem::mul(&s, &DecInterval::point(2.0));
        if k % 2 == 0 {
            s = elem::add(&s, &elem::sqr(&a[k / 2]));
        }
        h.push(s);
    }
    h
}

fn div(a: &Series, b: &Series) -> Series {
    let n = a.len();
    let mut h: Series = Vec::with_capacity(n);
    h.push(elem::div(&a[0], &b[0]));
    for k in 1..n {
        let mut s = a[k];
        for j in 1..=k {
            s = elem::sub(&s, &elem::mul(&b[j], &h[k - j]));
        }
        h.push(elem::div(&s, &b[0]));
    }
    h
}

fn one(n: usize) -> Series {
    konst(DecInterval::point(1.0), n)
}

fn recip(u: &Series) -> Series {
    div(&one(u.len() - 1), u)
}

/// h with h′ = g·u′: h_k = (1/k) Σ_{j=1..k} j u_j g_{k−j}.
fn integ(h0: DecInterval, u: &Series, g: &Series) -> Series {
    let n = u.len();
    let mut h = vec![h0];
    for k in 1..n {
        let mut s = DecInterval::point(0.0);
        for j in 1..=k {
            s = elem::add(&s, &elem::mul(&elem::mul(&pt(j), &u[j]), &g[k - j]));
        }
        h.push(elem::div(&s, &pt(k)));
    }
    h
}

fn exp(u: &Series) -> Series {
    let n = u.len();
    let mut h = vec![elem::exp(&u[0])];
    for k in 1..n {
        let mut s = DecInterval::point(0.0);
        for j in 1..=k {
            s = elem::add(&s, &elem::mul(&elem::mul(&pt(j), &u[j]), &h[k - j]));
        }
        h.push(elem::div(&s, &pt(k)));
    }
    h
}

fn ln(u: &Series) -> Series {
    let n = u.len();
    let mut h = vec![elem::ln(&u[0])];
    for k in 1..n {
        let mut s = DecInterval::point(0.0);
        for j in 1..k {
            s = elem::add(&s, &elem::mul(&elem::mul(&pt(j), &h[j]), &u[k - j]));
        }
        h.push(elem::div(&elem::sub(&u[k], &elem::div(&s, &pt(k))), &u[0]));
    }
    h
}

fn sqrt(u: &Series) -> Series {
    let n = u.len();
    let mut h = vec![elem::sqrt(&u[0])];
    let two_h0 = elem::mul(&h[0], &DecInterval::point(2.0));
    for k in 1..n {
        let mut s = u[k];
        for j in 1..k {
            s = elem::sub(&s, &elem::mul(&h[j], &h[k - j]));
        }
        h.push(elem::div(&s, &two_h0));
    }
    h
}

/// Radians per unit, enclosed.
fn unit_scale(unit: TrigUnit) -> DecInterval {
    match unit {
        TrigUnit::Radians => DecInterval::point(1.0),
        u => DecInterval::new(elem::pi() / Interval::point(u.full_turn() / 2.0)),
    }
}

/// (sin u, cos u) in `unit` (`hyper`: sinh, cosh).
fn sincos(u: &Series, unit: TrigUnit, hyper: bool) -> (Series, Series) {
    let n = u.len();
    let (mut s, mut c) = if hyper {
        (vec![elem::sinh(&u[0])], vec![elem::cosh(&u[0])])
    } else {
        let (s0, c0) = elem::sin_cos(&u[0], unit);
        (vec![s0], vec![c0])
    };
    let k_rad = if hyper {
        DecInterval::point(1.0)
    } else {
        unit_scale(unit)
    };
    for k in 1..n {
        let mut ss = DecInterval::point(0.0);
        let mut cc = DecInterval::point(0.0);
        for j in 1..=k {
            let ju = elem::mul(&elem::mul(&pt(j), &u[j]), &k_rad);
            ss = elem::add(&ss, &elem::mul(&ju, &c[k - j]));
            cc = elem::add(&cc, &elem::mul(&ju, &s[k - j]));
        }
        s.push(elem::div(&ss, &pt(k)));
        let cc = elem::div(&cc, &pt(k));
        c.push(if hyper { cc } else { elem::neg(&cc) });
    }
    (s, c)
}

/// tan / tanh: h′ = (1 ± h²)·u′ (times the unit's radians for tan).
fn tan(u: &Series, unit: TrigUnit, hyper: bool) -> Series {
    let n = u.len();
    let h0 = if hyper {
        elem::tanh(&u[0])
    } else {
        elem::tan(&u[0], unit)
    };
    let mut h = vec![h0];
    let k_rad = if hyper {
        DecInterval::point(1.0)
    } else {
        unit_scale(unit)
    };
    let mut g: Series = Vec::new();
    for k in 1..n {
        let m = k - 1;
        let mut s = DecInterval::point(0.0);
        if m == 0 {
            s = elem::sqr(&h[0]);
        } else {
            for i in 0..=m {
                s = elem::add(&s, &elem::mul(&h[i], &h[m - i]));
            }
        }
        let one = DecInterval::point(if m == 0 { 1.0 } else { 0.0 });
        let gm = if hyper {
            elem::sub(&one, &s)
        } else {
            elem::add(&one, &s)
        };
        g.push(elem::mul(&gm, &k_rad));
        let mut acc = DecInterval::point(0.0);
        for j in 1..=k {
            acc = elem::add(&acc, &elem::mul(&elem::mul(&pt(j), &u[j]), &g[k - j]));
        }
        h.push(elem::div(&acc, &pt(k)));
    }
    h
}

fn powi_ser(u: &Series, m: i32) -> Series {
    let n = u.len() - 1;
    if m == 0 {
        let mut r = one(n);
        r[0] = elem::powi(&u[0], 0);
        return r;
    }
    let mag = m.unsigned_abs();
    let mut result: Option<Series> = None;
    let mut base = u.clone();
    let mut e = mag;
    while e > 0 {
        if e & 1 == 1 {
            result = Some(match result {
                None => base.clone(),
                Some(r) => mul(&r, &base),
            });
        }
        e >>= 1;
        if e > 0 {
            base = sqr(&base);
        }
    }
    let mut r = result.expect("m ≠ 0");
    let c0 = elem::powi(&u[0], mag as i32);
    r[0] = c0.refine(&r[0]);
    if m < 0 {
        let mut q = div(&one(n), &r);
        q[0] = elem::powi(&u[0], m).refine(&q[0]);
        q
    } else {
        r
    }
}

/// u^(p/q), q > 1, real-root semantics.
fn powrat_ser(u: &Series, p: i32, q: i32) -> Series {
    let n = u.len() - 1;
    let r = elem::div(&DecInterval::point(p as f64), &DecInterval::point(q as f64));
    let c0 = elem::pow_rational(&u[0], p, q);
    let pos_branch = |w: &Series| exp(&scale(&ln(w), &r));
    if u[0].gt0() {
        let mut h = pos_branch(u);
        h[0] = c0.refine(&h[0]);
        return h;
    }
    if u[0].lt0() && q % 2 == 1 {
        let h = pos_branch(&neg(u));
        let mut h = if p % 2 != 0 { neg(&h) } else { h };
        h[0] = c0.refine(&h[0]);
        return h;
    }
    flat(c0, n)
}

fn pow(a: &Expr, b: &Expr, x: &Series, n: usize, ctx: &Ctx<'_>) -> Series {
    let u = ev(a, x, n, ctx);
    if let Some((p, q)) = syntactic_rational(b, crate::compile::Reading::Typed) {
        return if q == 1 {
            powi_ser(&u, p)
        } else {
            powrat_ser(&u, p, q)
        };
    }
    let v = ev(b, x, n, ctx);
    // A constant integer exponent written otherwise (x^(1+1)) is still an
    // integer power; an exponent that varies needs a positive base (TI),
    // and so does a constant that is no integer.
    let constant = !b.contains_x() && !b.contains_y();
    if constant && v[0].iv.is_point() && v[0].lo() == v[0].lo().trunc() && v[0].lo().abs() < 1e6 {
        // Defined only where the exponent is: x^⌊√(−10⁻³⁰)⌋ is nowhere
        // defined, though its exponent's enclosure is the point 0 (review
        // 13, R13-M-02: it was x⁰'s, 1 and continuous).
        return with_operand(powi_ser(&u, v[0].lo() as i32), &v[0]);
    }
    let c0 = elem::pow(&u[0], &v[0]);
    // A constant whose enclosure holds an integer may be that integer
    // (x^(0.1 + 0.9) is x¹; a typed 1.0000000000000001 is enclosed by 1 and
    // the double after it): a negative base may then be defined, with an
    // integer power's value. Possibly undefined, never claimed so.
    let (k0, k1) = (v[0].lo().ceil(), v[0].hi().floor());
    if constant && !v[0].is_empty() && k0 <= k1 && u[0].lo() < 0.0 && !u[0].gt0() {
        let mut iv = c0.iv;
        if k1 - k0 <= 4.0 && k0.abs() < 1e6 && k1.abs() < 1e6 {
            let mut k = k0;
            while k <= k1 {
                iv = iv.hull(elem::powi(&u[0], k as i32).iv);
                k += 1.0;
            }
        } else {
            iv = Interval::ENTIRE;
        }
        return flat(DecInterval::result(iv, Dec::Trv, &[&u[0], &v[0]]), n);
    }
    if u[0].gt0() {
        let mut h = exp(&mul(&v, &ln(&u)));
        h[0] = c0.refine(&h[0]);
        h
    } else {
        flat(c0, n)
    }
}

/// Inverse trig/hyperbolic functions: the value from [`elem`], the
/// derivatives by integrating g = φ′(w)·w′ for the base function φ of `w`
/// (`w` = u, or 1/u for arcsec and friends).
fn inverse(f: Func, u: &Series, n: usize, unit: TrigUnit) -> Series {
    use Func::*;
    let one = one(n);
    let (w, base) = match f {
        Asec => (recip(u), Acos),
        Acsc => (recip(u), Asin),
        Asech => (recip(u), Acosh),
        Acsch => (recip(u), Asinh),
        Acoth => (recip(u), Atanh),
        other => (u.clone(), other),
    };
    let w2 = sqr(&w);
    let g = match base {
        Atan => div(&one, &add(&one, &w2)),
        Acot => neg(&div(&one, &add(&one, &w2))),
        Asin => div(&one, &sqrt(&sub(&one, &w2))),
        Acos => neg(&div(&one, &sqrt(&sub(&one, &w2)))),
        Asinh => div(&one, &sqrt(&add(&one, &w2))),
        Acosh => div(&one, &sqrt(&sub(&w2, &one))),
        Atanh => div(&one, &sub(&one, &w2)),
        _ => unreachable!("an inverse function"),
    };
    // Trig results are in the unit: scale the derivative.
    let g = if f.returns_angle() && unit != TrigUnit::Radians {
        let s = elem::div(&DecInterval::point(1.0), &unit_scale(unit));
        scale(&g, &s)
    } else {
        g
    };
    let x0 = &u[0];
    let h0 = match f {
        Asin => elem::asin(x0, unit),
        Acos => elem::acos(x0, unit),
        Atan => elem::atan(x0, unit),
        Asec => elem::asec(x0, unit),
        Acsc => elem::acsc(x0, unit),
        Acot => elem::acot(x0, unit),
        Asinh => elem::asinh(x0),
        Acosh => elem::acosh(x0),
        Atanh => elem::atanh(x0),
        Asech => elem::asech(x0),
        Acsch => elem::acsch(x0),
        Acoth => elem::acoth(x0),
        _ => unreachable!(),
    };
    let mut h = integ(h0, &w, &g);
    // The derivatives exist only where the function does.
    if h0.dec < Dec::Dac {
        for c in h.iter_mut().skip(1) {
            *c = DecInterval::unknown();
        }
    }
    h
}

/// `h`, the series of an operation computed from some of its operands
/// alone (an integer power's from its base, the exponent only telling
/// which power), with the decoration of another operand `o` it leaves out:
/// f is defined (continuous, bounded) only where `o` is too, and a
/// possibly undefined `o` leaves f possibly undefined, its derivatives
/// unknown. (Every fast path must keep every operand's decoration:
/// [`DecInterval::refine`] keeps the better of two, so an enclosure that
/// forgot one would promote the result.)
fn with_operand(mut h: Series, o: &DecInterval) -> Series {
    if o.dec == Dec::Ill {
        h[0] = DecInterval::ill();
    } else if o.dec < h[0].dec {
        h[0].dec = o.dec;
        h[0] = h[0].normalized();
    }
    if h[0].dec < Dec::Dac {
        for c in h.iter_mut().skip(1) {
            *c = DecInterval::unknown();
        }
    }
    h
}

/// min or max where the ends decide that `winner` is the value on the whole
/// box: its series, if the losing argument is defined throughout the box
/// (a discontinuous but defined loser changes nothing); otherwise f may be
/// undefined somewhere on the box, and its value alone is kept, decorated
/// as the loser is (its derivatives then mean nothing, [`derivs_valid`]).
fn keep_winner(winner: Series, loser: &DecInterval) -> Series {
    if loser.dec >= Dec::Def {
        return winner;
    }
    let n = winner.len() - 1;
    let mut h0 = winner[0];
    h0.dec = h0.dec.min(loser.dec);
    flat(h0, n)
}

/// Whether a root degree written as a number (or its negative) is exactly
/// an odd integer, by its own exact value (the decimal typed: `Lit`);
/// `None` when that isn't known (a degree computed otherwise, or a number
/// known only to round to its double).
fn typed_odd(e: &Expr) -> Option<bool> {
    let r = match e {
        Expr::Num(v, lit) => lit.rat(*v)?,
        Expr::Neg(a) => match &**a {
            Expr::Num(v, lit) => lit.rat(*v)?,
            _ => return None,
        },
        _ => return None,
    };
    if !r.is_integer() {
        return Some(false);
    }
    Some(!r.div(&crate::big::Rat::int(2))?.is_integer())
}

/// True if the step function `step` takes one value on `v` widened by an
/// ulp each side (so it is constant on a neighbourhood of the box).
fn constant_near(v: &DecInterval, step: impl Fn(&DecInterval) -> DecInterval) -> bool {
    if v.is_empty() || v.dec < Dec::Dac {
        return false;
    }
    let w = Interval::new(v.lo().next_down(), v.hi().next_up());
    let r = step(&DecInterval::new(w));
    !r.is_empty() && r.iv.is_point() && r.dec >= Dec::Dac
}

fn ev(e: &Expr, x: &Series, n: usize, ctx: &Ctx<'_>) -> Series {
    let unit = ctx.opts.trig_unit;
    match e {
        Expr::Num(v, lit) => konst(DecInterval::new(lit.enclose(*v)), n),
        Expr::Const(Constant::Pi) => konst(DecInterval::new(elem::pi()), n),
        Expr::Const(Constant::E) => konst(DecInterval::new(elem::e()), n),
        Expr::X => x.clone(),
        Expr::Y => konst(ctx.y, n),
        Expr::Var(name) => konst(
            DecInterval::point(
                ctx.opts
                    .variables
                    .value(name)
                    .unwrap_or(DEFAULT_VARIABLE_VALUE),
            ),
            n,
        ),
        Expr::Neg(a) => neg(&ev(a, x, n, ctx)),
        Expr::Degrees(a) => {
            if unit != TrigUnit::Degrees {
                return flat(DecInterval::ill(), n);
            }
            ev(a, x, n, ctx)
        }
        Expr::Bin(BinOp::Pow, a, b) => pow(a, b, x, n, ctx),
        Expr::Bin(op, a, b) => {
            let (a, b) = (ev(a, x, n, ctx), ev(b, x, n, ctx));
            match op {
                BinOp::Add => add(&a, &b),
                BinOp::Sub => sub(&a, &b),
                BinOp::Mul => mul(&a, &b),
                BinOp::Div => div(&a, &b),
                BinOp::Pow => unreachable!(),
            }
        }
        Expr::Call(f, args) => call(*f, args, x, n, ctx),
    }
}

fn call(f: Func, args: &[Expr], x: &Series, n: usize, ctx: &Ctx<'_>) -> Series {
    use Func::*;
    let unit = ctx.opts.trig_unit;
    let arg = |i: usize| ev(&args[i], x, n, ctx);
    match f {
        Sin => sincos(&arg(0), unit, false).0,
        Cos => sincos(&arg(0), unit, false).1,
        Tan => tan(&arg(0), unit, false),
        Sec | Csc | Cot => {
            let u = arg(0);
            let (s, c) = sincos(&u, unit, false);
            let mut h = match f {
                Sec => recip(&c),
                Csc => recip(&s),
                _ => div(&c, &s),
            };
            let c0 = match f {
                Sec => elem::sec(&u[0], unit),
                Csc => elem::csc(&u[0], unit),
                _ => elem::cot(&u[0], unit),
            };
            h[0] = c0.refine(&h[0]);
            if c0.dec < Dec::Dac {
                h[0] = c0;
            }
            h
        }
        Sinh => sincos(&arg(0), unit, true).0,
        Cosh => sincos(&arg(0), unit, true).1,
        Tanh => tan(&arg(0), unit, true),
        Sech | Csch | Coth => {
            let u = arg(0);
            let (s, c) = sincos(&u, unit, true);
            let mut h = match f {
                Sech => recip(&c),
                Csch => recip(&s),
                _ => div(&c, &s),
            };
            let c0 = match f {
                Sech => elem::sech(&u[0]),
                Csch => elem::csch(&u[0]),
                _ => elem::coth(&u[0]),
            };
            h[0] = if c0.dec < Dec::Dac {
                c0
            } else {
                c0.refine(&h[0])
            };
            h
        }
        Asin | Acos | Atan | Asec | Acsc | Acot | Asinh | Acosh | Atanh | Asech | Acsch | Acoth => {
            inverse(f, &arg(0), n, unit)
        }
        Sqrt => sqrt(&arg(0)),
        Cbrt => {
            let u = arg(0);
            let mut h = powrat_ser(&u, 1, 3);
            h[0] = elem::cbrt(&u[0]).refine(&h[0]);
            h
        }
        Root => {
            let (u, k) = (arg(0), arg(1));
            let kv = k[0].iv;
            // A constant integer degree: its value a whole number and its
            // derivatives 0 (root(x, x) at 2 is 2 there, but its degree
            // varies: f′ is not the square root's).
            let constant = k[1..].iter().all(|c| c.iv.is_point() && c.lo() == 0.0);
            if constant
                && kv.is_point()
                && kv.lo() == kv.lo().trunc()
                && kv.lo().abs() < 1e6
                && kv.lo() != 0.0
            {
                let d = kv.lo() as i32;
                let mut h = match d {
                    1 => u.clone(),
                    -1 => recip(&u),
                    d if d > 0 => powrat_ser(&u, 1, d),
                    d => powrat_ser(&u, -1, -d),
                };
                h[0] = elem::root(&u[0], &k[0]).refine(&h[0]);
                // The series is u's alone: defined only where the degree
                // is too (root(x, 1 + ⌊√(−10⁻³⁰)⌋) is nowhere defined;
                // refined by u's series, its value was x, Com: R13-M-02).
                with_operand(h, &k[0])
            } else {
                // A degree written as a number is known exactly to be odd
                // or not where its enclosure can't say (a typed
                // 9007199254740993 is enclosed by 2⁵³ and the double after
                // it, both even): asked only where that matters.
                let odd = (u[0].lo() < 0.0 && elem::may_hold_odd(kv))
                    .then(|| typed_odd(&args[1]))
                    .flatten();
                let c0 = match odd {
                    Some(odd) => elem::root_of_parity(&u[0], &k[0], odd),
                    None => elem::root(&u[0], &k[0]),
                };
                let negative = odd == Some(true) && u[0].lt0();
                if (u[0].gt0() || negative) && k[0].ne0() && c0.dec >= Dec::Dac {
                    // u > 0: the root is e^(ln u / k), whatever k does; u < 0
                    // and an odd (constant) k: −e^(ln(−u) / k).
                    let mut h = if negative {
                        neg(&exp(&div(&ln(&neg(&u)), &k)))
                    } else {
                        exp(&div(&ln(&u), &k))
                    };
                    h[0] = c0.refine(&h[0]);
                    h
                } else {
                    flat(c0, n)
                }
            }
        }
        Ln => ln(&arg(0)),
        Log => {
            let u = arg(0);
            let inv_ln10 = elem::div(
                &DecInterval::point(1.0),
                &elem::ln(&DecInterval::point(10.0)),
            );
            let mut h = scale(&ln(&u), &inv_ln10);
            h[0] = elem::log10(&u[0]).refine(&h[0]);
            h
        }
        LogBase => div(&ln(&arg(1)), &ln(&arg(0))),
        Exp => exp(&arg(0)),
        Abs => {
            let u = arg(0);
            if u[0].gt0() {
                u
            } else if u[0].lt0() {
                neg(&u)
            } else {
                flat(elem::abs(&u[0]), n)
            }
        }
        Floor | Ceil | Round | Sign => {
            let u = arg(0);
            let step = |v: &DecInterval| match f {
                Floor => elem::floor(v),
                Ceil => elem::ceil(v),
                Round => elem::round(v),
                _ => elem::sign(v),
            };
            let h0 = step(&u[0]);
            // Derivative 0 only where the step is constant on a neighbourhood
            // of the box: a point box at a jump is continuous (restricted to
            // it) but not differentiable.
            if !h0.is_empty() && constant_near(&u[0], step) {
                konst(h0, n)
            } else {
                flat(h0, n)
            }
        }
        Mod => {
            let (a, b) = (arg(0), arg(1));
            let h0 = elem::modulo(&a[0], &b[0]);
            let quotient_steady = constant_near(&elem::div(&a[0], &b[0]), elem::floor);
            if h0.dec >= Dec::Dac && quotient_steady {
                // One integer quotient on the box: a − b·k.
                let k = elem::floor(&elem::div(&a[0], &b[0]));
                let mut h = sub(&a, &scale(&b, &k));
                h[0] = h0.refine(&h[0]);
                h
            } else {
                flat(h0, n)
            }
        }
        Min | Max => {
            let mut acc = arg(0);
            for i in 1..args.len() {
                let b = arg(i);
                // min and max are eager: undefined wherever either argument
                // is, whichever one wins elsewhere.
                if acc[0].is_empty() || b[0].is_empty() {
                    acc = flat(
                        DecInterval::result(Interval::EMPTY, Dec::Trv, &[&acc[0], &b[0]]),
                        n,
                    );
                    continue;
                }
                let a_below = acc[0].hi() < b[0].lo();
                let b_below = b[0].hi() < acc[0].lo();
                acc = match (f, a_below, b_below) {
                    (Min, true, _) | (Max, _, true) => keep_winner(acc, &b[0]),
                    (Min, _, true) | (Max, true, _) => keep_winner(b, &acc[0]),
                    _ => flat(
                        if f == Min {
                            elem::min(&acc[0], &b[0])
                        } else {
                            elem::max(&acc[0], &b[0])
                        },
                        n,
                    ),
                };
            }
            acc
        }
        Factorial => {
            let a = arg(0);
            let v = a[0].iv.lo() + 1.0;
            if ctx.pole_probe && a[0].iv.is_point() && v <= 0.0 && v == v.trunc() {
                let big = Interval::new(f64::MAX, f64::INFINITY);
                return flat(DecInterval::result(big, Dec::Trv, &[&a[0]]), n);
            }
            flat(elem::factorial(&a[0]), n)
        }
        DoubleFactorial => flat(elem::double_factorial(&arg(0)[0]), n),
        NCr | NPr => {
            let (a, b) = (arg(0), arg(1));
            let h0 = elem::ncr_npr(&a[0], &b[0], f == NPr);
            let constant = b[1..].iter().all(|c| c.iv.is_point() && c.lo() == 0.0);
            match elem::small_count(&b[0]) {
                // A polynomial in n where it is defined: n(n−1)…(n−k+1),
                // over k! for nCr.
                Some(k) if constant && h0.dec >= Dec::Dac => {
                    let mut h = konst(DecInterval::point(1.0), n);
                    let mut fact = DecInterval::point(1.0);
                    for i in 0..k {
                        h = mul(&h, &sub(&a, &konst(DecInterval::point(i as f64), n)));
                        fact = elem::mul(&fact, &DecInterval::point((i + 1) as f64));
                    }
                    if f == NCr {
                        h = scale(&h, &elem::recip(&fact));
                    }
                    h[0] = h0.refine(&h[0]);
                    h
                }
                _ => flat(h0, n),
            }
        }
    }
}
