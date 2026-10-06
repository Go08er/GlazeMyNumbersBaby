//! Taylor series over a box: `s[k]` encloses f⁽ᵏ⁾(x)/k! for every x in
//! the box. `s[0]`'s decoration says where f is defined and continuous;
//! `s[k].def` (k ≥ 1) says the k-th derivative exists throughout the box
//! (and then `s[k]` encloses it). The recurrences are the standard ones for
//! automatic differentiation of power series; written for the replay.

use super::iv::{self, Iv};

pub type S = Vec<Iv>;

/// A coefficient that doesn't exist (a kink, a jump, a pole inside).
fn none() -> Iv {
    Iv::unknown()
}

thread_local! {
    static LENIENT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn lenient() -> bool {
    LENIENT.get()
}

/// Runs `f` with coefficients enclosed *where they exist*: past a domain
/// end inside the box (x ≥ 0 under √x, a divisor's zero at an end) they
/// are still computed, for the points where f is defined, and only marked
/// not valid throughout. For boxes beside an excluded point (the gaps),
/// where nothing else can be said.
pub fn where_defined<T>(f: impl FnOnce() -> T) -> T {
    LENIENT.set(true);
    let r = f();
    LENIENT.set(false);
    r
}

fn zero() -> Iv {
    Iv::of(0.0)
}

/// Marks coefficients from `from` on as nonexistent.
fn invalid_from(mut s: S, from: usize) -> S {
    for c in s.iter_mut().skip(from) {
        if !c.empty {
            *c = none();
        }
    }
    s
}

/// Higher coefficients exist only where every input coefficient up to
/// that order does.
fn valid_upto(s: &S, k: usize) -> bool {
    s.iter().take(k + 1).all(|c| !c.empty && c.def)
}

fn restrict_validity(mut out: S, ins: &[&S]) -> S {
    for (k, c) in out.iter_mut().enumerate().skip(1) {
        if !ins.iter().all(|s| valid_upto(s, k)) && !c.empty {
            *c = not_throughout(c);
        }
    }
    out
}

/// A coefficient that may not exist throughout the box: none, or (where
/// defined) its bounds for the points where it does.
fn not_throughout(c: &Iv) -> Iv {
    if lenient() {
        // Where defined: the bounds hold where the inputs exist.
        c.clone().with(false, false)
    } else {
        none()
    }
}

/// A series built from a value alone, constant near each point of the box
/// where its argument `a` stays near its value there (u⁰ with u ≠ 0, a
/// step clear of its jumps): its coefficients (0) exist only where `a` is
/// continuous at each point of the box, beside it included. `a`'s own
/// first derivative existing throughout the box shows that
/// (differentiable, so continuous); the box's decoration doesn't, a point
/// box being continuous restricted to itself (review 16, R16-L-01: ⌊x⌋⁰ at
/// 1 had derivatives 0, though just left of 1 it is 0⁰, undefined; ⌊√x +
/// 0.5⌋ at 0 too, though √x ends there).
fn beside(mut s: S, a: &S) -> S {
    if !valid_upto(a, 1) {
        for c in s.iter_mut().skip(1) {
            if !c.empty {
                *c = not_throughout(c);
            }
        }
    }
    s
}

/// A value constant in x: its derivatives 0, existing throughout the box
/// only where the value is defined throughout it.
pub fn constant(c: Iv, n: usize) -> S {
    let mut s = vec![c.clone()];
    for _ in 0..n {
        let mut z = zero();
        z.def = c.def;
        s.push(if c.empty { Iv::empty() } else { z });
    }
    s
}

/// `s`, worked out from its other operands with a constant operand `o`
/// set aside (an integer power's exponent, a root's degree, read from its
/// enclosure as a point): f exists only where `o` does, so the value is
/// only as defined and continuous as `o` too, and the coefficients from 1
/// on are f's only where `o` is defined and continuous throughout (then
/// constant). An `o` defined nowhere leaves f defined nowhere. (Review 17,
/// R17-M-02: (x + 1)^⌊√(sin²4 + cos²4 − 1 − 10⁻¹⁰⁰)⌋ at 160 bits has the
/// exponent's enclosure the point 0, possibly undefined; read as x⁰ alone,
/// f(0) = 1 was proved, though f is defined nowhere.)
pub fn with_operand(mut s: S, o: &Iv) -> S {
    if o.empty {
        return vec![Iv::empty(); s.len()];
    }
    s[0] = s[0].clone().with(o.def, o.cont);
    if !(o.def && o.cont) {
        s = invalid_from(s, 1);
    }
    s
}

pub fn var(x: Iv, n: usize) -> S {
    let mut s = vec![x];
    if n >= 1 {
        s.push(Iv::of(1.0));
    }
    for _ in 2..=n {
        s.push(zero());
    }
    s
}

pub fn order(s: &S) -> usize {
    s.len() - 1
}

pub fn add(a: &S, b: &S) -> S {
    let out = a.iter().zip(b).map(|(x, y)| iv::add(x, y)).collect();
    restrict_validity(out, &[a, b])
}

pub fn sub(a: &S, b: &S) -> S {
    let out = a.iter().zip(b).map(|(x, y)| iv::sub(x, y)).collect();
    restrict_validity(out, &[a, b])
}

pub fn neg(a: &S) -> S {
    a.iter().map(iv::neg).collect()
}

pub fn scale(a: &S, k: &Iv) -> S {
    let out = a.iter().map(|x| iv::mul(x, k)).collect();
    restrict_validity(out, &[a])
}

pub fn mul(a: &S, b: &S) -> S {
    let n = order(a);
    let mut out = Vec::with_capacity(n + 1);
    for k in 0..=n {
        let mut acc = zero();
        for j in 0..=k {
            acc = iv::add(&acc, &iv::mul(&a[j], &b[k - j]));
        }
        out.push(acc);
    }
    // The value: its own decoration (a product is defined and continuous
    // where both factors are).
    out[0] = iv::mul(&a[0], &b[0]);
    restrict_validity(out, &[a, b])
}

thread_local! {
    static SHIFT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` with quotients 0/0 at a point cancelled: where a divisor's
/// series at the point starts with m exact zeros and the dividend's with
/// at least m, both are shifted down by m (exact, x ≠ the point), so the
/// result is the series of the quotient's continuous extension (to m
/// orders less). Only for series at a point.
pub fn with_shift<T>(f: impl FnOnce() -> T) -> T {
    SHIFT.set(true);
    let r = f();
    SHIFT.set(false);
    r
}

fn div_shift(a: &S, b: &S) -> Option<S> {
    let n = order(a);
    let m = (0..=n).find(|&j| !b[j].is_exactly(0.0))?;
    if !b[m].ne0() || !valid_upto(b, m) || !valid_upto(a, m) {
        return None;
    }
    if !(0..m).all(|j| a[j].is_exactly(0.0)) {
        return None;
    }
    let pad = |s: &S| -> S {
        let mut t: S = s[m..].to_vec();
        while t.len() < n + 1 {
            t.push(none());
        }
        t
    };
    Some(div(&pad(a), &pad(b)))
}

pub fn div(a: &S, b: &S) -> S {
    let n = order(a);
    if SHIFT.get()
        && b[0].is_exactly(0.0)
        && let Some(s) = div_shift(a, b)
    {
        return s;
    }
    let mut out: S = Vec::with_capacity(n + 1);
    out.push(iv::div(&a[0], &b[0]));
    if out[0].empty {
        return vec![Iv::empty(); n + 1];
    }
    let ok = b[0].ne0() || (lenient() && !b[0].is_exactly(0.0));
    for k in 1..=n {
        if !ok {
            out.push(none());
            continue;
        }
        let mut acc = a[k].clone();
        for j in 1..=k {
            acc = iv::sub(&acc, &iv::mul(&b[j], &out[k - j]));
        }
        out.push(iv::div(&acc, &b[0]));
    }
    restrict_validity(out, &[a, b])
}

pub fn recip(b: &S) -> S {
    div(&constant(Iv::of(1.0), order(b)), b)
}

/// aⁿ for an integer n by repeated squaring of the series (the value from
/// the interval power, which knows x² ≥ 0).
pub fn powi(a: &S, n: i64) -> S {
    let ord = order(a);
    let value = iv::powi(&a[0], n);
    if value.empty {
        return vec![Iv::empty(); ord + 1];
    }
    if n == 0 {
        let mut s = constant(Iv::of(1.0), ord);
        s[0] = value.clone();
        if !value.def {
            s = invalid_from(s, 1);
        }
        // 1 where a ≠ 0: constant beside the box only where a stays ≠ 0.
        return beside(s, a);
    }
    if n < 0 {
        let mut s = recip(&powi(a, -n));
        s[0] = s[0].meet(&value);
        return s;
    }
    let mut result: Option<S> = None;
    let mut base = a.clone();
    let mut e = n;
    while e > 0 {
        if e & 1 == 1 {
            result = Some(match result {
                None => base.clone(),
                Some(r) => mul(&r, &base),
            });
        }
        e >>= 1;
        if e > 0 {
            base = mul(&base, &base);
        }
    }
    let mut s = result.expect("n > 0");
    s[0] = s[0].meet(&value);
    s
}

/// e^a as e^(a₀)·e^(a − a₀): the second factor's coefficients b_k (b₀ =
/// 1; k·b_k = Σ j·a_j·b_(k−j)) don't involve e^(a₀), so its value enters
/// each coefficient once (e^(−x²)'s second coefficient is e^(−x²)·(2x² −
/// 1), positive for |x| > 0.71 however wide the box).
pub fn exp(a: &S) -> S {
    let n = order(a);
    let e0 = iv::exp(&a[0]);
    let mut b = vec![Iv::of(1.0)];
    for k in 1..=n {
        let mut acc = zero();
        for j in 1..=k {
            acc = iv::add(&acc, &iv::mul(&iv::scale(&a[j], j as f64), &b[k - j]));
        }
        b.push(iv::div(&acc, &Iv::of(k as f64)));
    }
    let mut out = vec![e0.clone()];
    for bk in b.iter().skip(1) {
        out.push(iv::mul(&e0, bk));
    }
    restrict_validity(out, &[a])
}

pub fn ln(a: &S) -> S {
    let n = order(a);
    let mut out = vec![iv::ln(&a[0])];
    let ok = a[0].gt(0.0) || (lenient() && a[0].hi > 0);
    for k in 1..=n {
        if !ok {
            out.push(none());
            continue;
        }
        let mut acc = zero();
        for j in 1..k {
            acc = iv::add(&acc, &iv::mul(&iv::scale(&out[j], j as f64), &a[k - j]));
        }
        let t = iv::sub(&a[k], &iv::div(&acc, &Iv::of(k as f64)));
        out.push(iv::div(&t, &a[0]));
    }
    restrict_validity(out, &[a])
}

pub fn sqrt(a: &S) -> S {
    let n = order(a);
    let mut out = vec![iv::sqrt(&a[0])];
    let ok = a[0].gt(0.0) || (lenient() && a[0].hi > 0);
    for k in 1..=n {
        if !ok {
            out.push(none());
            continue;
        }
        let mut acc = a[k].clone();
        for j in 1..k {
            acc = iv::sub(&acc, &iv::mul(&out[j], &out[k - j]));
        }
        out.push(iv::div(&acc, &iv::scale(&out[0], 2.0)));
    }
    restrict_validity(out, &[a])
}

/// g(a) from g's Taylor coefficients at a's value (`gs[j]` ∋ g⁽ʲ⁾(a₀)/j!
/// for every value a₀ of a on the box): y_k = Σ_{j=1..k} gs[j]·[(a −
/// a₀)^j]_k. No division by a₀: at a tail, where a₀ reaches ±∞, the
/// coefficients stay finite (x^0.9's derivative 0.9·x^(−0.1), not
/// 0.9·x^0.9/x = ∞/∞).
pub fn compose(gs: &[Iv], a: &S) -> S {
    let n = order(a);
    let mut d = a.clone();
    d[0] = zero();
    let mut out: S = (0..=n).map(|_| zero()).collect();
    out[0] = gs[0].clone();
    let mut pw = d.clone();
    for (j, g) in gs.iter().enumerate().take(n + 1).skip(1) {
        for k in j..=n {
            out[k] = iv::add(&out[k], &iv::mul(g, &pw[k]));
        }
        if j < n {
            pw = mul(&pw, &d);
        }
    }
    restrict_validity(out, &[a])
}

/// a^p for a > 0 and a constant exponent p, by [`compose`] with the
/// coefficients C(p, j)·a₀^(p−j). `value` is the value's own enclosure
/// (with the domain's decoration).
pub fn pow_real(a: &S, p: &Iv, value: Iv) -> S {
    let n = order(a);
    if value.empty {
        return vec![Iv::empty(); n + 1];
    }
    let ok = a[0].gt(0.0) || (lenient() && a[0].hi > 0);
    let mut gs = vec![value];
    let mut binom = Iv::of(1.0);
    for j in 1..=n {
        if !ok {
            gs.push(none());
            continue;
        }
        // C(p, j) = C(p, j − 1)·(p − j + 1)/j.
        binom = iv::div(
            &iv::mul(&binom, &iv::sub(p, &Iv::of((j - 1) as f64))),
            &Iv::of(j as f64),
        );
        let e = iv::sub(p, &Iv::of(j as f64));
        gs.push(iv::mul(&binom, &iv::pow_var(&a[0], &e)));
    }
    let mut s = compose(&gs, a);
    if !ok {
        s = invalid_from(s, 1);
    }
    s
}

/// (sin a, cos a) with the argument already in radians, `s0`/`c0` the
/// values (exact where the unit makes them so).
pub fn sin_cos(a: &S, s0: Iv, c0: Iv) -> (S, S) {
    let n = order(a);
    let (mut s, mut c) = (vec![s0], vec![c0]);
    for k in 1..=n {
        let (mut ss, mut cc) = (zero(), zero());
        for j in 1..=k {
            let ja = iv::scale(&a[j], j as f64);
            ss = iv::add(&ss, &iv::mul(&ja, &c[k - j]));
            cc = iv::add(&cc, &iv::mul(&ja, &s[k - j]));
        }
        let kk = Iv::of(k as f64);
        s.push(iv::div(&ss, &kk));
        c.push(iv::neg(&iv::div(&cc, &kk)));
    }
    (restrict_validity(s, &[a]), restrict_validity(c, &[a]))
}

/// (sinh a, cosh a).
pub fn sinh_cosh(a: &S) -> (S, S) {
    let n = order(a);
    let (mut s, mut c) = (vec![iv::sinh(&a[0])], vec![iv::cosh(&a[0])]);
    for k in 1..=n {
        let (mut ss, mut cc) = (zero(), zero());
        for j in 1..=k {
            let ja = iv::scale(&a[j], j as f64);
            ss = iv::add(&ss, &iv::mul(&ja, &c[k - j]));
            cc = iv::add(&cc, &iv::mul(&ja, &s[k - j]));
        }
        let kk = Iv::of(k as f64);
        s.push(iv::div(&ss, &kk));
        c.push(iv::div(&cc, &kk));
    }
    (restrict_validity(s, &[a]), restrict_validity(c, &[a]))
}

/// y = F(a) from F's value `y0` and the series of F′(a) (`d`, at least to
/// order n − 1): y_k = (1/k) Σ_{j=1..k} j a_j d_{k−j}.
pub fn integrate(a: &S, y0: Iv, d: &S) -> S {
    let n = order(a);
    let mut out = vec![y0];
    for k in 1..=n {
        let mut acc = zero();
        for j in 1..=k {
            acc = iv::add(&acc, &iv::mul(&iv::scale(&a[j], j as f64), &d[k - j]));
        }
        out.push(iv::div(&acc, &Iv::of(k as f64)));
    }
    restrict_validity(out, &[a, d])
}

/// tan(a) from its value `t0`, by tan′ = (1 + tan²)·a′:
/// t_k = (1/k) Σ_{j=1..k} j a_j p_{k−j} with p = 1 + t². Near a pole,
/// where sin/cos's coefficients are wide, p₀ ≥ 1 still holds.
pub fn tan_ode(a: &S, t0: Iv) -> S {
    let n = order(a);
    let mut t = vec![t0];
    let mut p: Vec<Iv> = Vec::new();
    for k in 1..=n {
        let m = k - 1;
        p.push(if m == 0 {
            iv::add(&Iv::of(1.0), &iv::powi(&t[0], 2))
        } else {
            let mut acc = zero();
            for i in 0..=m {
                acc = iv::add(&acc, &iv::mul(&t[i], &t[m - i]));
            }
            acc
        });
        let mut acc = zero();
        for j in 1..=k {
            acc = iv::add(&acc, &iv::mul(&iv::scale(&a[j], j as f64), &p[k - j]));
        }
        t.push(iv::div(&acc, &Iv::of(k as f64)));
    }
    restrict_validity(t, &[a])
}

/// Replaces coefficients from 1 on as nonexistent unless `ok`.
pub fn only_value_unless(s: S, ok: bool) -> S {
    if ok { s } else { invalid_from(s, 1) }
}

/// A step function's series of the argument `a`: constant (derivatives 0)
/// where its argument stays strictly between jumps (`smooth`; a box ending
/// at a jump is continuous on the box, but the derivative doesn't exist at
/// the jump) and does so beside the box too ([`beside`]: ⌊⌊x⌋ + 0.5⌋ at 1
/// takes one value on the box, but ⌊x⌋ + 0.5 jumps there), else only the
/// value.
pub fn step(a: &S, value: Iv, smooth: bool) -> S {
    let n = order(a);
    if value.empty {
        return vec![Iv::empty(); n + 1];
    }
    if smooth && value.cont && value.is_point() {
        beside(constant(value, n), a)
    } else {
        invalid_from(constant(value, n), 1)
    }
}

/// min or max where the ends put `w` strictly beyond the other argument
/// on the box: f is `w` there only where the other is defined too (min
/// and max are undefined wherever either argument is), and its
/// coefficients are `w`'s only where `w` stays the one beside the box, so
/// where the other argument's own derivatives exist (it can't jump past
/// `w`): min(x, 2⌊x⌋ − 0.5) at 1 jumps.
pub fn winner(w: S, other: &S) -> S {
    let mut s = w;
    s[0] = s[0].clone().with(other[0].def, other[0].def);
    if s.len() > 1 && !(other[0].cont && valid_upto(other, 1)) {
        return invalid_from(s, 1);
    }
    s
}

pub fn with_value(mut s: S, v: Iv) -> S {
    s[0] = v;
    s
}
