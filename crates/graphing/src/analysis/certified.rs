//! The panel from the certified analysis ([`crate::certify`]): each row
//! shows what its certificate proves, and nothing else.
//!
//! A certified row is shown complete; a partial one (every item proven,
//! more may lie beyond the region its proof covers) is shown with a note
//! saying where it is complete; an unknown one says it couldn't be
//! calculated, never "none".
//!
//! Numbers are written exactly only when proven exact: a double the
//! certifier pinned exactly, or a closed form checked with exact
//! arithmetic ([`super::exact`]) against the claim that pins the value
//! down. A crossing the certificate claims unique on a box is the closed
//! form when the box holds it and the crossed expression takes the
//! crossed level there exactly (f(x̂) = 0 for an x-intercept, f′(x̂) = 0
//! for an extremum, a side expression's level for a domain end); a
//! table family of excluded points is the closed form when it is a zero of
//! the side expression and its period is the table's; a value is f at an
//! exact point, or an exact limit (rational functions, the simplifier's
//! limits and periods). Anything else is written to the digits its
//! enclosure fixes (at most six significant), marked "≈" when the text
//! could pass for exact.

use std::cell::OnceCell;

use super::exact::{self, Ex};
use super::format::{self, MINUS, format_decimal_digits};
use super::{
    AnalysisData, AsymptoteSide, Family as DataFamily, KeyGraphFeatures, Monotonicity, Parity,
    Periodicity, flags,
};
use crate::ast::{BinOp, Constant, Expr, Func};
use crate::certify::fun::{Fun, at_path};
use crate::certify::rows::EndSrc;
use crate::certify::{
    self, Analysis, Bound, Certificate, Claim, Dir, Enc, ExtKind, Piece, Region, Row, Spot,
    Subject, Tail, Via,
};
use crate::compile::{CompileOptions, VariableValues};
use crate::functions::TrigUnit;
use crate::interval::{Dec, Interval, Literals};
use crate::simplify::rational::{Poly, Rational};
use crate::simplify::{ExactLiterals, Limit, PiQ, Q, rational_form};
use crate::strings as s;

/// Lists longer than this are cut to the items nearest 0 (and then say
/// there may be more).
const MAX_ITEMS: usize = 20;

// ------------------------------------------------------------- numbers

/// The simplest rational in `[lo, hi]` (0 < lo ≤ hi): the one with the
/// smallest denominator (Stern–Brocot), if it fits.
fn simplest_pos(lo: Q, hi: Q, depth: u32) -> Option<Q> {
    if depth > 64 {
        return None;
    }
    let fl = lo.numer().div_euclid(lo.denom());
    if Q::int(fl) == lo {
        return Some(lo);
    }
    let up = Q::int(fl.checked_add(1)?);
    if up <= hi {
        return Some(up);
    }
    // lo, hi in (fl, fl + 1).
    let base = Q::int(fl);
    let r = simplest_pos(hi.sub(base)?.recip()?, lo.sub(base)?.recip()?, depth + 1)?;
    base.add(r.recip()?)
}

/// The simplest rational in `[lo, hi]`.
fn simplest(lo: f64, hi: f64) -> Option<Q> {
    if !(lo.is_finite() && hi.is_finite() && lo <= hi) {
        return None;
    }
    if lo <= 0.0 && 0.0 <= hi {
        return Some(Q::ZERO);
    }
    let (l, h) = (Q::from_f64(lo)?, Q::from_f64(hi)?);
    if hi < 0.0 {
        simplest_pos(h.neg()?, l.neg()?, 0)?.neg()
    } else {
        simplest_pos(l, h, 0)
    }
}

/// `[lo, hi]` widened by a few units in the last place.
fn widen(lo: f64, hi: f64) -> (f64, f64) {
    let (mut a, mut b) = (lo.min(hi), lo.max(hi));
    for _ in 0..4 {
        a = a.next_down();
        b = b.next_up();
    }
    (a, b)
}

/// Closed forms near an enclosure, to be checked: the simplest rational in
/// it, the simplest rational multiple of π (radians), ±√r for the simplest
/// rational r in its square.
fn nice(x: Enc, unit: TrigUnit) -> Vec<Ex> {
    let (lo, hi) = (x.lo.0, x.hi.0);
    let mut out = Vec::new();
    if let Some(q) = simplest(lo, hi) {
        out.push(Ex::q(q));
    }
    if unit == TrigUnit::Radians {
        let pi = std::f64::consts::PI;
        let (a, b) = widen(lo / pi, hi / pi);
        if let Some(q) = simplest(a, b) {
            out.push(Ex::Pi(q));
        }
    }
    if lo > 0.0 || hi < 0.0 {
        let (m, n) = (lo.abs().min(hi.abs()), lo.abs().max(hi.abs()));
        let (a, b) = widen(m * m, n * n);
        if let Some(r) = simplest(a, b)
            && let Some(root) = Ex::q(r).sqrt()
        {
            out.push(if hi < 0.0 {
                root.neg().unwrap_or(root)
            } else {
                root
            });
        }
    }
    out
}

/// Exactly equal.
fn same(a: Ex, b: Ex) -> bool {
    a == b || a.sub(b).is_some_and(Ex::is_zero)
}

fn poly_at(p: &Poly, x: Ex) -> Option<Ex> {
    let mut acc = Ex::int(0);
    for c in p.coefficients().iter().rev() {
        acc = acc.mul(x)?.add(Ex::q(*c))?;
    }
    Some(acc)
}

/// Significant digits in a decimal text (`1.5`, `−0.000123`, `1.2×10⁻⁷`).
fn sig_digits(t: &str) -> usize {
    let mantissa = t.split('×').next().unwrap_or(t);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let trimmed = digits.trim_start_matches('0');
    if trimmed.is_empty() { 1 } else { trimmed.len() }
}

/// A value known to lie in `e`: as many significant digits (up to six) as
/// every value in `e` rounds to alike, marked "≈" when the text has fewer
/// than six (it could read as exact: "≈1", "≈0.5").
pub(super) fn approx(e: Enc) -> String {
    let (lo, hi) = (e.lo.0, e.hi.0);
    if e.is_point() && lo == 0.0 {
        return "0".into();
    }
    // Zero within rounding.
    if lo <= 0.0 && 0.0 <= hi && hi - lo <= 1e-12 {
        return "≈0".into();
    }
    if !(lo <= 0.0 && 0.0 <= hi) {
        for sig in (1..=6).rev() {
            let (a, b) = (
                format_decimal_digits(lo, sig),
                format_decimal_digits(hi, sig),
            );
            if a == b {
                return if sig_digits(&a) < 6 {
                    format!("≈{a}")
                } else {
                    a
                };
            }
        }
    }
    // Not even one digit fixed: the enclosure itself, rounded outward.
    format!("[{}, {}]", outward(lo, false), outward(hi, true))
}

/// `v` rounded to two significant digits, up or down.
fn outward(v: f64, up: bool) -> String {
    if v == 0.0 || !v.is_finite() {
        return format_decimal_digits(v, 2);
    }
    let unit = 10f64.powi(v.abs().log10().floor() as i32 - 1);
    let mut k = (v / unit).floor();
    if up {
        k = (v / unit).ceil();
    }
    // The decimal k·unit, nudged past v if rounding put it inside.
    let mut d = k * unit;
    for _ in 0..4 {
        if (up && d >= v) || (!up && d <= v) {
            break;
        }
        k += if up { 1.0 } else { -1.0 };
        d = k * unit;
    }
    format_decimal_digits(d, 2)
}

/// A number: exact text if known (and not unwieldy), else [`approx`].
#[derive(Clone, Copy, Debug)]
struct Num {
    exact: Option<Ex>,
    enc: Enc,
}

impl Num {
    fn text(&self) -> String {
        self.exact
            .and_then(Ex::text)
            .unwrap_or_else(|| approx(self.enc))
    }

    fn value(&self) -> f64 {
        match self.exact {
            Some(e) => e.to_f64(),
            None => self.enc.mid(),
        }
    }

    fn is_zero(&self) -> bool {
        self.exact.is_some_and(Ex::is_zero) || (self.enc.is_point() && self.enc.lo.0 == 0.0)
    }
}

/// `k`-multiples of a period: `2kπ`, `kπ/2`, `360k`.
fn times_k(p: &Num) -> String {
    p.exact
        .and_then(Ex::times_k)
        .unwrap_or_else(|| format!("{}k", approx(p.enc).trim_start_matches('≈')))
}

/// `x₀ + k·P` (k ∈ ℤ implied).
fn family_text(x0: &Num, p: &Num) -> String {
    if x0.is_zero() {
        return times_k(p);
    }
    format!("{} + {}", x0.text(), times_k(p))
}

/// The family's representative moved to the member in (−P/2, P/2]
/// (exactly when both are exact).
fn normalize(x: Num, p: Num) -> (Num, Num) {
    let pm = p.value();
    if !(pm > 0.0) {
        return (x, p);
    }
    let n = match (x.exact, p.exact) {
        // n = ⌈x/P − 1/2⌉, exactly.
        (Some(xe), Some(pe)) => match xe.div(pe).and_then(Ex::rational) {
            Some(t) => {
                let Some(u) = t.sub(Q::new(1, 2).expect("1/2")) else {
                    return (x, p);
                };
                (-((-u.numer()).div_euclid(u.denom()))) as f64
            }
            None => (x.value() / pm - 0.5).ceil(),
        },
        _ => (x.value() / pm - 0.5).ceil(),
    };
    if n == 0.0 || !n.is_finite() || n.abs() > 1e12 {
        return (x, p);
    }
    (shift(x, &p, n), p)
}

/// `x − n·P`.
fn shift(x: Num, p: &Num, n: f64) -> Num {
    let exact = match (x.exact, p.exact) {
        (Some(xe), Some(pe)) => pe.mul(Ex::int(n as i128)).and_then(|s| xe.sub(s)),
        _ => None,
    };
    let (plo, phi) = if n > 0.0 {
        (p.enc.hi.0, p.enc.lo.0)
    } else {
        (p.enc.lo.0, p.enc.hi.0)
    };
    let (a, b) = widen(x.enc.lo.0 - n * plo, x.enc.hi.0 - n * phi);
    let slack = (n * p.enc.hi.0).abs() * 4.0 * f64::EPSILON;
    Num {
        exact,
        enc: Enc::new(a - slack, b + slack),
    }
}

/// Families with one period that are evenly spaced by P/n merge into one
/// with period P/n (the zeros of sin at 0 and π every 2π: kπ). `tag`
/// must agree too (an extremum's value).
fn merge(items: Vec<(Num, Num, String)>) -> Vec<(Num, Num, String)> {
    let mut out: Vec<(Num, Num, String)> = Vec::new();
    let mut used = vec![false; items.len()];
    for i in 0..items.len() {
        if used[i] {
            continue;
        }
        let (x, p, tag) = &items[i];
        let (Some(_), Some(pe)) = (x.exact, p.exact) else {
            used[i] = true;
            out.push(items[i].clone());
            continue;
        };
        let group: Vec<usize> = (i..items.len())
            .filter(|&j| {
                !used[j]
                    && items[j].2 == *tag
                    && items[j].0.exact.is_some()
                    && items[j].1.exact.is_some_and(|q| same(q, pe))
            })
            .collect();
        let n = group.len();
        let merged = (n >= 2)
            .then(|| {
                let step = pe.div(Ex::int(n as i128))?;
                let base = items[group[0]].0.exact?;
                let mut seen = vec![false; n];
                for &j in &group {
                    let t = items[j].0.exact?.sub(base)?.div(step)?.rational()?;
                    if !t.is_int() {
                        return None;
                    }
                    let r = t.numer().rem_euclid(n as i128) as usize;
                    if seen[r] {
                        return None;
                    }
                    seen[r] = true;
                }
                let x = items[group[0]].0;
                let (a, b) = widen(p.enc.lo.0 / n as f64, p.enc.hi.0 / n as f64);
                let p = Num {
                    exact: Some(step),
                    enc: Enc::new(a, b),
                };
                Some(normalize(x, p))
            })
            .flatten();
        match merged {
            Some((x, p)) => {
                for &j in &group {
                    used[j] = true;
                }
                out.push((x, p, tag.clone()));
            }
            None => {
                used[i] = true;
                out.push(items[i].clone());
            }
        }
    }
    out
}

// ------------------------------------------------------------- context

/// f = n/d in lowest terms, with the numerators of f′ = p1/d² and
/// f″ = p2/d³ (`p` = [n, p1, p2]).
struct Rat {
    d: Poly,
    p: [Poly; 3],
    roots: [OnceCell<Vec<Ex>>; 3],
}

impl Rat {
    fn new(r: &Rational) -> Option<Rat> {
        let (n, d) = (r.num.clone(), r.den.clone());
        let (dn, dd) = (exact::derivative(&n)?, exact::derivative(&d)?);
        let p1 = dn.mul(&d)?.sub(&n.mul(&dd)?)?;
        let p2 = exact::derivative(&p1)?
            .mul(&d)?
            .sub(&p1.mul(&dd)?.scale(Q::int(2))?)?;
        Some(Rat {
            d,
            p: [n, p1, p2],
            roots: Default::default(),
        })
    }

    fn roots(&self, k: usize) -> &[Ex] {
        self.roots[k].get_or_init(|| exact::exact_roots(&self.p[k]))
    }
}

/// Every Num in the tree's exponents is the double it was typed as (a
/// power's rational form and derivative read its exponent off the
/// double).
fn honest_exponents(e: &Expr, lits: &ExactLiterals) -> bool {
    let mut ok = true;
    e.visit(&mut |n| {
        if let Expr::Bin(BinOp::Pow, _, b) = n {
            b.visit(&mut |m| {
                if let Expr::Num(v) = m
                    && lits.exact(*v) != Q::from_f64(*v)
                {
                    ok = false;
                }
            });
        }
    });
    ok
}

/// f′ and f″ as trees whose constants read back exactly: the derivative
/// copies f's own numbers (read as typed), makes integers from exponents
/// (checked honest), and multiplies in ln 10 and the angle-unit factor
/// (no typed number may have their bits). Numbers typed with more than 15
/// significant digits are refused: a constant folded from one could round
/// to a different exact value.
fn derivative_trees(f: &Expr, unit: TrigUnit, lits: &ExactLiterals) -> [Option<Expr>; 2] {
    let none = [None, None];
    if !honest_exponents(f, lits) {
        return none;
    }
    let mut ok = true;
    let reserved = [
        std::f64::consts::LN_10.to_bits(),
        unit.to_radians_factor().to_bits(),
    ];
    f.visit(&mut |n| {
        if let Expr::Num(v) = n {
            let short = lits.exact(*v).is_some_and(|q| {
                q.denom() <= 1_000_000_000_000_000
                    && q.numer().unsigned_abs() <= 1_000_000_000_000_000
            });
            if !short || reserved.contains(&v.to_bits()) {
                ok = false;
            }
        }
    });
    if !ok {
        return none;
    }
    let d1 = crate::diff::derivative(f, unit);
    let d2 = d1.as_ref().and_then(|d| crate::diff::derivative(d, unit));
    [d1, d2]
}

fn cert_of<T>(r: &Row<T>) -> Option<&Certificate> {
    match r {
        Row::Certified { cert, .. } | Row::Partial { cert, .. } => Some(cert),
        Row::Unknown { .. } => None,
    }
}

fn enc_of(b: &certify::XBox) -> Enc {
    Enc { lo: b.a, hi: b.b }
}

struct Ctx<'a> {
    f: Expr,
    unit: TrigUnit,
    lits: &'a ExactLiterals,
    vars: &'a dyn VariableValues,
    /// f with interval literals, for "finite here" checks.
    fun: Fun<'a>,
    rat: Option<Rat>,
    /// The reduced rational form (asymptotes, hole values).
    reduced: Option<Rational>,
    d: [Option<Expr>; 2],
    /// Every crossing claimed unique on a box: (box, subject, level).
    cross: Vec<(Enc, Subject, f64)>,
    /// Every exact crossing claimed, and every value claimed exact at a
    /// double: (box, subject, level, where).
    exact_at: Vec<(Enc, Subject, f64, f64)>,
    /// Every table family: (side expression, x₀, period).
    fams: Vec<(Subject, Enc, Enc)>,
    /// A proven period of f (the one the certifier's periodic window used).
    period: Option<Ex>,
    /// Its enclosure, as the certifier encloses it.
    period_enc: Option<Enc>,
    /// The excluded points exactly, when the domain is the line less
    /// table families and each family is exact.
    xfams: Option<Vec<(Ex, Ex)>>,
}

impl<'a> Ctx<'a> {
    fn new(
        f: Expr,
        opts: &'a CompileOptions<'a>,
        lits: &'a ExactLiterals,
        ilits: &'a Literals,
        a: &Analysis,
    ) -> Ctx<'a> {
        let unit = opts.trig_unit;
        let rf = honest_exponents(&f, lits)
            .then(|| rational_form(&f, lits))
            .flatten();
        let rat = rf.as_ref().and_then(|r| Rat::new(&r.reduced));
        let d = if rat.is_some() {
            [None, None]
        } else {
            derivative_trees(&f, unit, lits)
        };
        let mut certs: Vec<&Certificate> = Vec::new();
        certs.extend(cert_of(&a.domain));
        certs.extend(cert_of(&a.x_intercepts));
        certs.extend(cert_of(&a.y_intercept));
        certs.extend(cert_of(&a.extrema));
        certs.extend(cert_of(&a.inflections));
        certs.extend(cert_of(&a.monotonicity));
        certs.extend(cert_of(&a.range));
        certs.extend(cert_of(&a.vertical));
        let (mut cross, mut exact_at, mut fams) = (Vec::new(), Vec::new(), Vec::new());
        for c in certs {
            for cl in &c.claims {
                match cl {
                    Claim::OneCross { x, of, c } => cross.push((enc_of(x), of.clone(), c.0)),
                    Claim::ExactAt { x, of, c, at } => {
                        exact_at.push((enc_of(x), of.clone(), c.0, at.0));
                    }
                    // The subject equals the level exactly at a double.
                    Claim::Value { x, of, lo, hi } if x.a == x.b && lo == hi => {
                        exact_at.push((enc_of(x), of.clone(), lo.0, x.a.0));
                    }
                    Claim::Family { of, x0, period } => {
                        fams.push((of.clone(), enc_of(x0), enc_of(period)));
                    }
                    _ => {}
                }
            }
        }
        let piq = crate::simplify::period::prove_period(&f, unit, lits).map(|p| p.value);
        let period = piq.and_then(Ex::from_piq);
        let period_enc = piq
            .and_then(certify::rows::piq_interval)
            .map(|iv| Enc::new(iv.lo(), iv.hi()));
        let fun = Fun::new(&f, ilits, *opts, 1_000_000, None);
        let mut cx = Ctx {
            f,
            unit,
            lits,
            vars: opts.variables,
            fun,
            rat,
            reduced: rf.map(|r| r.reduced),
            d,
            cross,
            exact_at,
            fams,
            period,
            period_enc,
            xfams: None,
        };
        if let Row::Certified { value, .. } = &a.domain
            && value.pieces.len() == 1
            && value.pieces[0].lo == Bound::NegInf
            && value.pieces[0].hi == Bound::PosInf
        {
            cx.xfams = value
                .excluded
                .iter()
                .map(|fam| cx.table_family(fam.x0, fam.period))
                .collect();
        }
        cx
    }

    fn eval(&self, e: &Expr, x: Ex) -> Option<Ex> {
        exact::eval(e, x, self.unit, self.lits, self.vars)
    }

    /// e is defined, finite (and, with `nonzero`, away from 0) on the box.
    fn finite_on(&self, e: &Expr, x: Enc, nonzero: bool) -> bool {
        let Ok(s) = self.fun.ser_of(e, Interval::new(x.lo.0, x.hi.0), 0) else {
            return false;
        };
        let v = s[0];
        !v.is_empty()
            && v.iv.is_bounded()
            && v.dec >= Dec::Def
            && (!nonzero || !v.iv.contains_zero())
    }

    /// e is 0 at x̂ (inside the box `x`): exactly, or as a product with a
    /// factor exactly 0 and the others finite there (f′ = e⁻ˣ·(1 − x) at
    /// 1).
    fn zero_at(&self, e: &Expr, at: Ex, x: Enc) -> bool {
        if let Some(v) = self.eval(e, at) {
            return v.is_zero();
        }
        match e {
            Expr::Neg(a) => self.zero_at(a, at, x),
            Expr::Bin(BinOp::Mul, a, b) => {
                (self.zero_at(a, at, x) && self.finite_on(b, x, false))
                    || (self.zero_at(b, at, x) && self.finite_on(a, x, false))
            }
            Expr::Bin(BinOp::Div, a, b) => self.zero_at(a, at, x) && self.finite_on(b, x, true),
            _ => false,
        }
    }

    /// The side expression a claim's subject names.
    fn side(&self, path: &[u8], via: Via) -> Option<Expr> {
        let node = at_path(&self.f, path)?;
        Some(match via {
            Via::Itself => node.clone(),
            Via::CosOfArg | Via::SinOfArg => {
                let Expr::Call(_, args) = node else {
                    return None;
                };
                let func = if via == Via::CosOfArg {
                    Func::Cos
                } else {
                    Func::Sin
                };
                Expr::Call(func, vec![args.first()?.clone()])
            }
        })
    }

    /// The subject takes the value `c` at x̂ (inside the box `x`), exactly.
    fn holds(&self, s: &Subject, at: Ex, c: f64, x: Enc) -> bool {
        let Some(cq) = Q::from_f64(c) else {
            return false;
        };
        let target = Ex::q(cq);
        let tree = match s {
            Subject::F(o) => match (o.k(), &self.rat) {
                (0, _) => self.f.clone(),
                (k, Some(r)) => {
                    // f⁽ᵏ⁾ = pₖ/dᵏ⁺¹ wherever f is defined.
                    return cq.is_zero()
                        && poly_at(&r.d, at).is_some_and(|v| !v.is_zero())
                        && poly_at(&r.p[k], at).is_some_and(Ex::is_zero);
                }
                (k, None) => match &self.d[k - 1] {
                    Some(d) => d.clone(),
                    None => return false,
                },
            },
            Subject::G { path, via } => match self.side(path, *via) {
                Some(g) => g,
                None => return false,
            },
        };
        if cq.is_zero() {
            self.zero_at(&tree, at, x)
        } else {
            self.eval(&tree, at).is_some_and(|v| same(v, target))
        }
    }

    /// Closed forms to try for a crossing of `s` at level `c`.
    fn structural(&self, s: &Subject, c: f64) -> Vec<Ex> {
        match s {
            Subject::F(o) => self
                .rat
                .as_ref()
                .map(|r| r.roots(o.k()).to_vec())
                .unwrap_or_default(),
            Subject::G { path, via } => {
                let Some(g) = self.side(path, *via) else {
                    return Vec::new();
                };
                if !honest_exponents(&g, self.lits) {
                    return Vec::new();
                }
                let (Some(rf), Some(cq)) = (rational_form(&g, self.lits), Q::from_f64(c)) else {
                    return Vec::new();
                };
                let r = rf.reduced;
                r.den
                    .scale(cq)
                    .and_then(|cd| r.num.sub(&cd))
                    .map(|p| exact::exact_roots(&p))
                    .unwrap_or_default()
            }
        }
    }

    /// x exactly: a double the certifier pinned, or a closed form inside
    /// `x` that meets every crossing claimed unique on exactly that box
    /// (the point is one of them, so it is the point).
    fn exact_x(&self, x: Enc) -> Option<Ex> {
        if x.is_point() {
            return Q::from_f64(x.lo.0).map(Ex::q);
        }
        let claims: Vec<&(Enc, Subject, f64)> = self
            .cross
            .iter()
            .filter(|(b, _, _)| b.lo.0 == x.lo.0 && b.hi.0 == x.hi.0)
            .collect();
        if claims.is_empty() {
            return None;
        }
        let mut cands = nice(x, self.unit);
        for (_, s, c) in &claims {
            cands.extend(self.structural(s, *c));
        }
        cands.into_iter().find(|&cand| {
            cand.within(x.lo.0, x.hi.0) == Some(true)
                && claims.iter().all(|(_, s, c)| self.holds(s, cand, *c, x))
        })
    }

    fn x(&self, x: Enc) -> Num {
        Num {
            exact: self.exact_x(x),
            enc: x,
        }
    }

    /// f(x̂) exactly, checked against its enclosure `y`.
    fn y_at(&self, x: Option<Ex>, y: Enc) -> Num {
        let exact = x
            .and_then(|x| self.eval(&self.f, x))
            .filter(|v| v.within(y.lo.0, y.hi.0) == Some(true));
        Num { exact, enc: y }
    }

    fn half_turn(&self) -> Ex {
        match self.unit {
            TrigUnit::Radians => Ex::Pi(Q::ONE),
            TrigUnit::Degrees => Ex::int(180),
            TrigUnit::Grads => Ex::int(200),
        }
    }

    /// A table family of excluded points, exactly: its period is the
    /// table's (a half or whole turn over |α| for the argument α·x + β),
    /// and x̂₀ is a zero of the side expression inside x₀'s enclosure
    /// (narrower than the period: the only one there).
    fn table_family(&self, x0: Enc, period: Enc) -> Option<(Ex, Ex)> {
        let (s, _, _) = self
            .fams
            .iter()
            .find(|(_, a, p)| *a == x0 && *p == period)?;
        let Subject::G { path, via } = s else {
            return None;
        };
        let g = self.side(path, *via)?;
        let mut u = None;
        g.visit(&mut |n| {
            if u.is_none()
                && let Expr::Call(Func::Sin | Func::Cos | Func::Tan | Func::Cot, args) = n
            {
                u = args.first().cloned();
            }
        });
        let u = u?;
        let beta = self.eval(&u, Ex::int(0))?;
        let alpha = self.eval(&u, Ex::int(1))?.sub(beta)?;
        let abs = if alpha.sign()? < 0 {
            alpha.neg()?
        } else {
            alpha
        };
        let h = self.half_turn();
        let mut found = None;
        for m in [1, 2] {
            let p = h.mul(Ex::int(m))?.div(abs)?;
            if p.within(period.lo.0, period.hi.0) == Some(true) {
                if found.is_some() {
                    return None;
                }
                found = Some(p);
            }
        }
        let p = found?;
        let pf = p.to_f64();
        if !(x0.hi.0 - x0.lo.0 < pf / 2.0 && period.hi.0 - period.lo.0 < pf / 4.0) {
            return None;
        }
        // Candidates: (u₀ − β)/α for the table's u₀, moved into x₀'s box.
        let mut cands = nice(x0, self.unit);
        let half = h.div(Ex::int(2))?;
        for u0 in [Ex::int(0), half, h, half.neg()?] {
            if let Some(c) = u0.sub(beta).and_then(|d| d.div(alpha)) {
                let n = ((x0.mid() - c.to_f64()) / pf).round();
                if n.is_finite() && n.abs() < 1e12 {
                    cands.extend(p.mul(Ex::int(n as i128)).and_then(|s| c.add(s)));
                }
            }
        }
        let x = cands.into_iter().find(|&c| {
            c.within(x0.lo.0, x0.hi.0) == Some(true) && self.eval(&g, c).is_some_and(Ex::is_zero)
        })?;
        Some((x, p))
    }

    /// The one excluded point inside `x`, exactly (the domain is the line
    /// less exact families: any excluded point in `x` is one of theirs).
    fn member(&self, x: Enc) -> Option<Ex> {
        let fams = self.xfams.as_ref()?;
        let mut found: Option<Ex> = None;
        for &(x0, p) in fams {
            let pf = p.to_f64();
            let k0 = ((x.mid() - x0.to_f64()) / pf).round();
            if !k0.is_finite() || k0.abs() > 1e12 {
                return None;
            }
            for dk in [-1.0, 0.0, 1.0] {
                let c = x0.add(p.mul(Ex::int((k0 + dk) as i128))?)?;
                if c.within(x.lo.0, x.hi.0) == Some(true) {
                    match found {
                        Some(f) if !same(f, c) => return None,
                        _ => found = Some(c),
                    }
                }
            }
        }
        found
    }

    /// An excluded family's exact period inside `p`.
    fn xfam_period(&self, p: Enc) -> Option<Ex> {
        let fams = self.xfams.as_ref()?;
        let mut found: Option<Ex> = None;
        for &(_, per) in fams {
            if per.within(p.lo.0, p.hi.0) == Some(true) {
                match found {
                    Some(f) if !same(f, per) => return None,
                    _ => found = Some(per),
                }
            }
        }
        found
    }

    /// A closed end of a monotone piece of a periodic f, a period away from
    /// where it was found (the piece that wrapped around the window): f′
    /// is 0 there and, by periodicity, has exactly one zero in a box a
    /// period away from one where its crossing is claimed unique (the box
    /// shifted by every period in the period's enclosure, rounded inward,
    /// must hold `x`); the closed form a period back is checked to be that
    /// zero.
    fn periodic_end(&self, x: Enc) -> Option<Ex> {
        let (p, pe) = (self.period?, self.period_enc?);
        let inside = |b: &Enc, k: f64| {
            let (lo, hi) = if k > 0.0 {
                ((b.lo.0 + pe.hi.0).next_up(), (b.hi.0 + pe.lo.0).next_down())
            } else {
                ((b.lo.0 - pe.lo.0).next_up(), (b.hi.0 - pe.hi.0).next_down())
            };
            lo <= x.lo.0 && x.hi.0 <= hi
        };
        let first_order = |s: &Subject| matches!(s, Subject::F(o) if o.k() == 1);
        for k in [-1.0f64, 1.0] {
            let shift = p.mul(Ex::int(k as i128))?;
            // f′ = 0 exactly at a double, a period back.
            for (b, s, c, at) in &self.exact_at {
                // (A zero at a double, alone in its box: shifted, it is the
                // one in x.)
                if first_order(s)
                    && *c == 0.0
                    && (b.is_point() || inside(b, k))
                    && let Some(e) = Q::from_f64(*at).and_then(|q| Ex::q(q).add(shift))
                    && e.within(x.lo.0, x.hi.0) == Some(true)
                {
                    return Some(e);
                }
            }
            for (b, s, c) in &self.cross {
                if !first_order(s) || *c != 0.0 || !inside(b, k) {
                    continue;
                }
                let (lo, hi) = widen(x.lo.0 - k * p.to_f64(), x.hi.0 - k * p.to_f64());
                for z in nice(Enc::new(lo, hi), self.unit) {
                    if z.within(b.lo.0, b.hi.0) == Some(true)
                        && self.holds(s, z, 0.0, *b)
                        && let Some(e) = z.add(shift)
                        && e.within(x.lo.0, x.hi.0) == Some(true)
                    {
                        return Some(e);
                    }
                }
            }
        }
        None
    }

    /// The proven period of f, if `p` (an enclosure of the period the
    /// certifier used, or of the least period) holds it or a whole
    /// fraction of it: two periods that close are equal, as every period
    /// is a multiple of the least.
    fn period_in(&self, p: Enc) -> Option<Ex> {
        let big = self.period?;
        let pf = big.to_f64();
        for n in 1..=12 {
            let cand = big.div(Ex::int(n))?;
            if cand.within(p.lo.0, p.hi.0) == Some(true) {
                let width = p.hi.0 - p.lo.0;
                return (width < pf / ((n * (n + 1)) as f64)).then_some(cand);
            }
        }
        None
    }

    /// A family x₀ + k·P of excluded points (or of poles at them).
    fn family(&self, x0: Enc, p: Enc) -> (Num, Num) {
        let (x, p) = match self.table_family(x0, p) {
            Some((x, per)) => (
                Num {
                    exact: Some(x),
                    enc: x0,
                },
                Num {
                    exact: Some(per),
                    enc: p,
                },
            ),
            None => (
                Num {
                    exact: self.exact_x(x0).or_else(|| self.member(x0)),
                    enc: x0,
                },
                Num {
                    exact: self.xfam_period(p).or_else(|| self.period_in(p)),
                    enc: p,
                },
            ),
        };
        normalize(x, p)
    }

    /// An end of a monotone piece of a periodic f: an excluded point (open)
    /// or where f′ is 0 (closed), maybe a period from where it was found.
    fn periodic_xnum(&self, b: &Bound) -> Option<Num> {
        let Bound::At { x, closed } = *b else {
            return None;
        };
        let exact = self.exact_x(x).or_else(|| {
            if closed {
                self.periodic_end(x)
            } else {
                self.member(x)
            }
        });
        Some(Num { exact, enc: x })
    }

    fn settings(&self) -> crate::simplify::Settings<'a> {
        let opts = CompileOptions {
            trig_unit: self.unit,
            variables: self.vars,
        };
        crate::simplify::Settings::new(&opts, self.lits)
    }

    fn dir(side: Tail) -> crate::simplify::Dir {
        match side {
            Tail::Left => crate::simplify::Dir::NegInf,
            Tail::Right => crate::simplify::Dir::PosInf,
        }
    }

    /// A limit at ±∞, exactly: the rational form's, or the simplifier's.
    fn tail_limit(&self, side: Tail, y: Enc) -> Option<Ex> {
        let v = match &self.reduced {
            Some(r) => Ex::q(r.horizontal()?),
            None => match crate::simplify::limit_at(&self.f, Self::dir(side), &self.settings()) {
                Limit::Exact(v) => Ex::from_piq(v)?,
                _ => return None,
            },
        };
        (v.within(y.lo.0, y.hi.0) == Some(true)).then_some(v)
    }

    /// The oblique asymptote's slope and intercept, exactly.
    fn oblique_exact(&self, side: Tail, m: Enc, b: Enc) -> Option<(Ex, Ex)> {
        let (mv, bv) = match &self.reduced {
            Some(r) => {
                let (m, b) = r.oblique()?;
                (Ex::q(m), Ex::q(b))
            }
            None => {
                let s = self.settings();
                let dir = Self::dir(side);
                let over = Expr::bin(BinOp::Div, self.f.clone(), Expr::X);
                let Limit::Exact(mq) = crate::simplify::limit_at(&over, dir, &s) else {
                    return None;
                };
                let mx = piq_expr(mq)?;
                let rest = Expr::bin(
                    BinOp::Sub,
                    self.f.clone(),
                    Expr::bin(BinOp::Mul, mx, Expr::X),
                );
                let Limit::Exact(bq) = crate::simplify::limit_at(&rest, dir, &s) else {
                    return None;
                };
                (Ex::from_piq(mq)?, Ex::from_piq(bq)?)
            }
        };
        (mv.within(m.lo.0, m.hi.0) == Some(true) && bv.within(b.lo.0, b.hi.0) == Some(true))
            .then_some((mv, bv))
    }
}

fn piq_expr(v: PiQ) -> Option<Expr> {
    let q = crate::simplify::rational::q_expr(v.q);
    match v.k {
        0 => Some(q),
        1 => Some(Expr::bin(BinOp::Mul, q, Expr::Const(Constant::Pi))),
        _ => None,
    }
}

// ------------------------------------------------------------- sets

/// One end of an interval, for display.
#[derive(Clone, Debug)]
struct End {
    text: String,
    closed: bool,
    /// ±∞ (`Some(true)` for +∞).
    inf: Option<bool>,
    value: f64,
}

fn infinite(up: bool) -> End {
    End {
        text: if up {
            "∞".into()
        } else {
            format!("{MINUS}∞")
        },
        closed: false,
        inf: Some(up),
        value: if up { f64::INFINITY } else { f64::NEG_INFINITY },
    }
}

fn finite(n: Num, closed: bool) -> End {
    End {
        text: n.text(),
        closed,
        inf: None,
        value: n.value(),
    }
}

impl Ctx<'_> {
    fn end(&self, b: &Bound) -> End {
        match b {
            Bound::NegInf => infinite(false),
            Bound::PosInf => infinite(true),
            Bound::At { x, closed } => finite(self.x(*x), *closed),
        }
    }

    /// A range end: exact from where the certifier says it comes from
    /// (every source exact and equal, when it is the hull of several).
    fn range_end(&self, b: &Bound, srcs: Option<&[EndSrc]>) -> End {
        let Bound::At { x: y, closed } = *b else {
            return self.end(b);
        };
        let one = |src: &EndSrc| -> Option<Ex> {
            match *src {
                EndSrc::At(x) => self.y_at(self.exact_x(x), y).exact,
                EndSrc::Tail(side) => self.tail_limit(side, y),
                EndSrc::Hole(x) => self.reduced.as_ref().and_then(|r| {
                    // The reduced rational form at the hole.
                    let xe = self.exact_x(x)?;
                    let v = poly_at(&r.num, xe)?.div(poly_at(&r.den, xe)?)?;
                    (v.within(y.lo.0, y.hi.0) == Some(true)).then_some(v)
                }),
                _ => None,
            }
        };
        let exact = if y.is_point() {
            Q::from_f64(y.lo.0).map(Ex::q)
        } else {
            srcs.filter(|s| !s.is_empty()).and_then(|s| {
                let vals: Option<Vec<Ex>> = s.iter().map(one).collect();
                let vals = vals?;
                vals.iter().all(|v| same(*v, vals[0])).then_some(vals[0])
            })
        };
        finite(Num { exact, enc: y }, closed)
    }
}

fn interval_text(a: &End, b: &End) -> String {
    format!(
        "{}{}, {}{}",
        if a.closed { "[" } else { "(" },
        a.text,
        b.text,
        if b.closed { "]" } else { ")" }
    )
}

/// A union of disjoint intervals as `var ∈ …` (`x ∈ ℝ`, `x ∈ ℝ \ {0}`,
/// `y ∈ {5}`, `x ∈ (−∞, −1] ∪ [1, ∞)`), like [`format::format_set`];
/// `joined(i)`: pieces i and i + 1 meet at one excluded point.
fn set_text(var: &str, parts: &[(End, End)], joined: &dyn Fn(usize) -> bool) -> String {
    if parts.is_empty() {
        return format!("{var} ∈ ∅");
    }
    let n = parts.len();
    let all_but_points = parts[0].0.inf == Some(false)
        && parts[n - 1].1.inf == Some(true)
        && (0..n - 1).all(|i| joined(i) && !parts[i].1.closed && !parts[i + 1].0.closed);
    if all_but_points {
        if n == 1 {
            return format!("{var} ∈ ℝ");
        }
        let pts: Vec<&str> = parts[..n - 1].iter().map(|p| p.1.text.as_str()).collect();
        return format!("{var} ∈ ℝ \\ {{{}}}", pts.join(", "));
    }
    let point =
        |p: &(End, End)| p.0.inf.is_none() && p.0.text == p.1.text && p.0.closed && p.1.closed;
    if parts.iter().all(point) {
        let pts: Vec<&str> = parts.iter().map(|p| p.0.text.as_str()).collect();
        return format!("{var} ∈ {{{}}}", pts.join(", "));
    }
    let items: Vec<String> = parts
        .iter()
        .map(|p| {
            if point(p) {
                format!("{{{}}}", p.0.text)
            } else {
                interval_text(&p.0, &p.1)
            }
        })
        .collect();
    format!("{var} ∈ {}", items.join(" ∪ "))
}

fn data_interval(a: &End, b: &End) -> format::Interval {
    format::Interval {
        lo: format::Bound {
            value: a.value,
            closed: a.closed,
        },
        hi: format::Bound {
            value: b.value,
            closed: b.closed,
        },
    }
}

// ------------------------------------------------------------- rows

/// How far a list row's proof reaches.
#[derive(Clone, Copy)]
enum Reach {
    /// Complete.
    All,
    /// Correct, complete only over a window (or only the items named).
    Part(Option<(f64, f64)>),
    Unknown,
}

fn reach<T>(r: &Row<T>) -> Reach {
    match r {
        Row::Certified { .. } => Reach::All,
        Row::Partial { cert, .. } => Reach::Part(match cert.covers {
            Region::Window { a, b } => Some((a.0, b.0)),
            _ => None,
        }),
        Row::Unknown { .. } => Reach::Unknown,
    }
}

/// Keeps the `MAX_ITEMS` items nearest 0, in x order; true if any were
/// cut.
fn cut<T>(items: &mut Vec<(f64, T)>) -> bool {
    items.sort_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
    let cut = items.len() > MAX_ITEMS;
    items.truncate(MAX_ITEMS);
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    cut
}

struct Out {
    k: KeyGraphFeatures,
}

impl Out {
    fn unknown(&mut self, flag: u32) {
        self.k.too_complex_features |= flag;
    }

    fn note(&mut self, flag: u32, note: String) {
        self.k.partial_features |= flag;
        self.k.notes.push((flag, note));
    }

    /// Flags and the note for one list row of the panel: `empty`, it shows
    /// no item; `was_cut`, items were left out.
    fn settle(&mut self, flag: u32, r: Reach, empty: bool, was_cut: bool) {
        let window = |t: &str, a: f64, b: f64| {
            t.replace("%1", &approx(Enc::point(a)))
                .replace("%2", &approx(Enc::point(b)))
        };
        match r {
            Reach::All if was_cut => self.note(flag, s::KGF_PARTIAL_SOME.into()),
            Reach::All => {}
            // Nothing found, and no proof there is nothing: unknown.
            Reach::Part(w) if empty => {
                self.unknown(flag);
                if let Some((a, b)) = w {
                    self.note(flag, window(s::KGF_PARTIAL_NONE_WINDOW, a, b));
                }
            }
            Reach::Part(Some((a, b))) if !was_cut => {
                self.note(flag, window(s::KGF_PARTIAL_WINDOW, a, b));
            }
            Reach::Part(_) => self.note(flag, s::KGF_PARTIAL_SOME.into()),
            Reach::Unknown => self.unknown(flag),
        }
    }
}

/// The panel for `f` from its certified analysis.
pub(super) fn features(
    f: &Expr,
    opts: &CompileOptions<'_>,
    lits: &ExactLiterals,
    ilits: &Literals,
    a: &Analysis,
) -> KeyGraphFeatures {
    let cx = Ctx::new(certify::canonical(f), opts, lits, ilits, a);
    let mut out = Out {
        k: KeyGraphFeatures::default(),
    };
    let mut data = AnalysisData::default();

    // Constant on its domain: the range is one value.
    let constant: Option<Enc> = match &a.range {
        Row::Certified { value, .. } if value.len() == 1 => match value[0] {
            Piece {
                lo: Bound::At { x: l, closed: true },
                hi: Bound::At { x: h, closed: true },
            } if l == h && l.is_point() => Some(l),
            _ => None,
        },
        _ => None,
    };
    let zero_function = constant.is_some_and(|c| c.lo.0 == 0.0);

    // Domain.
    let mut domain_whole = false;
    match &a.domain {
        Row::Certified { value, .. } => {
            let parts: Vec<(End, End)> = value
                .pieces
                .iter()
                .map(|p| (cx.end(&p.lo), cx.end(&p.hi)))
                .collect();
            let joined = |i: usize| match (value.pieces[i].hi, value.pieces[i + 1].lo) {
                (Bound::At { x: a, .. }, Bound::At { x: b, .. }) => a == b,
                _ => false,
            };
            let mut text = set_text("x", &parts, &joined);
            domain_whole = value.pieces.len() == 1
                && value.pieces[0].lo == Bound::NegInf
                && value.pieces[0].hi == Bound::PosInf;
            if !value.excluded.is_empty() {
                let fams: Vec<(Num, Num, String)> = value
                    .excluded
                    .iter()
                    .map(|fam| {
                        let (x, p) = cx.family(fam.x0, fam.period);
                        (x, p, String::new())
                    })
                    .collect();
                let texts: Vec<String> = merge(fams)
                    .iter()
                    .map(|(x, p, _)| {
                        data.excluded.push(DataFamily {
                            x: x.value(),
                            period: Some(p.value()),
                        });
                        family_text(x, p)
                    })
                    .collect();
                let base = if domain_whole {
                    "x ∈ ℝ".to_string()
                } else {
                    text.clone()
                };
                text = format!("{base} \\ {{{} | k ∈ ℤ}}", texts.join(", "));
                domain_whole = false;
            }
            data.domain = parts.iter().map(|(a, b)| data_interval(a, b)).collect();
            out.k.domain = text;
        }
        _ => out.unknown(flags::DOMAIN),
    }

    // Range.
    match &a.range {
        Row::Certified { value, .. } => {
            let parts: Vec<(End, End)> = value
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let srcs = a.range_ends.get(i);
                    (
                        cx.range_end(&p.lo, srcs.map(|s| s[0].as_slice())),
                        cx.range_end(&p.hi, srcs.map(|s| s[1].as_slice())),
                    )
                })
                .collect();
            let joined = |i: usize| match (value[i].hi, value[i + 1].lo) {
                (Bound::At { x: a, .. }, Bound::At { x: b, .. }) => a == b,
                _ => false,
            };
            out.k.range = set_text("y", &parts, &joined);
            data.range = parts.iter().map(|(a, b)| data_interval(a, b)).collect();
        }
        _ => out.unknown(flags::RANGE),
    }

    // x-intercepts.
    match a.x_intercepts.value() {
        Some(v) => {
            let mut items: Vec<(f64, (Num, Option<Num>))> = Vec::new();
            let mut fams: Vec<(Num, Num, String)> = Vec::new();
            for spot in v {
                match spot {
                    Spot::At(x) => {
                        let n = cx.x(*x);
                        items.push((n.value(), (n, None)));
                    }
                    Spot::Every(fam) => {
                        let p = Num {
                            exact: cx.period_in(fam.period),
                            enc: fam.period,
                        };
                        let (x, p) = normalize(cx.x(fam.x0), p);
                        fams.push((x, p, String::new()));
                    }
                }
            }
            for (x, p, _) in merge(fams) {
                items.push((x.value(), (x, Some(p))));
            }
            let was_cut = cut(&mut items);
            out.settle(
                flags::ZEROS,
                reach(&a.x_intercepts),
                items.is_empty(),
                was_cut,
            );
            let mut texts = Vec::new();
            let mut any_family = false;
            for (_, (x, p)) in &items {
                match p {
                    Some(p) => {
                        any_family = true;
                        texts.push(family_text(x, p));
                        data.zeros.push(DataFamily {
                            x: x.value(),
                            period: Some(p.value()),
                        });
                    }
                    None => {
                        texts.push(x.text());
                        data.zeros.push(DataFamily::single(x.value()));
                    }
                }
            }
            let mut t = texts.join(", ");
            if any_family {
                t.push_str(", k ∈ ℤ");
            }
            if out.k.too_complex_features & flags::ZEROS == 0 {
                out.k.x_intercept = t;
            }
        }
        // f ≡ 0: every x of the domain.
        None if zero_function && !out.k.domain.is_empty() => {
            out.k.x_intercept = out.k.domain.clone();
        }
        None => out.unknown(flags::ZEROS),
    }

    // y-intercept.
    match &a.y_intercept {
        Row::Certified { value, .. } | Row::Partial { value, .. } => {
            if let Some(y) = value {
                let n = cx.y_at(Some(Ex::int(0)), *y);
                out.k.y_intercept = n.text();
                data.y_intercept = Some(n.value());
            }
        }
        Row::Unknown { .. } => out.unknown(flags::Y_INTERCEPT),
    }

    // Extrema.
    match a.extrema.value() {
        Some(v) => {
            for kind in [ExtKind::Min, ExtKind::Max] {
                let pts: Vec<(Enc, Enc, Option<Enc>)> = v
                    .iter()
                    .filter(|e| e.kind == kind)
                    .map(|e| (e.x, e.y, e.every))
                    .collect();
                let (texts, d, was_cut) = points(&cx, &pts);
                let flag = if kind == ExtKind::Min {
                    flags::MINIMA
                } else {
                    flags::MAXIMA
                };
                out.settle(flag, reach(&a.extrema), texts.is_empty(), was_cut);
                if out.k.too_complex_features & flag != 0 {
                    continue;
                }
                if kind == ExtKind::Min {
                    out.k.minima = texts;
                    data.minima = d;
                } else {
                    out.k.maxima = texts;
                    data.maxima = d;
                }
            }
        }
        None => out.unknown(flags::MINIMA | flags::MAXIMA),
    }

    // Inflection points.
    match a.inflections.value() {
        Some(v) => {
            let pts: Vec<(Enc, Enc, Option<Enc>)> = v.iter().map(|i| (i.x, i.y, i.every)).collect();
            let (texts, d, was_cut) = points(&cx, &pts);
            out.settle(
                flags::INFLECTION_POINTS,
                reach(&a.inflections),
                texts.is_empty(),
                was_cut,
            );
            if out.k.too_complex_features & flags::INFLECTION_POINTS == 0 {
                out.k.inflection_points = texts;
                data.inflection_points = d;
            }
        }
        None => out.unknown(flags::INFLECTION_POINTS),
    }

    // Vertical asymptotes.
    match a.vertical.value() {
        Some(v) => {
            let mut items: Vec<(f64, String)> = Vec::new();
            let mut fams: Vec<(Num, Num, String)> = Vec::new();
            for spot in v {
                match spot {
                    Spot::At(x) => {
                        let n = cx.x(*x);
                        data.vertical_asymptotes.push(DataFamily::single(n.value()));
                        items.push((n.value(), format!("x = {}", n.text())));
                    }
                    Spot::Every(fam) => {
                        let (x, p) = cx.family(fam.x0, fam.period);
                        fams.push((x, p, String::new()));
                    }
                }
            }
            for (x, p, _) in merge(fams) {
                data.vertical_asymptotes.push(DataFamily {
                    x: x.value(),
                    period: Some(p.value()),
                });
                items.push((x.value(), format!("x = {}, k ∈ ℤ", family_text(&x, &p))));
            }
            let was_cut = cut(&mut items);
            out.settle(
                flags::VERTICAL_ASYMPTOTES,
                reach(&a.vertical),
                items.is_empty(),
                was_cut,
            );
            if out.k.too_complex_features & flags::VERTICAL_ASYMPTOTES == 0 {
                out.k.vertical_asymptotes = items.into_iter().map(|(_, t)| t).collect();
            }
        }
        None => out.unknown(flags::VERTICAL_ASYMPTOTES),
    }

    // A function that is a line on its domain (a constant, m·x + b with
    // holes) is not its own asymptote: its graph is the line.
    let line_degree = cx
        .reduced
        .as_ref()
        .filter(|r| r.den.degree() == Some(0))
        .map(|r| r.num.degree().unwrap_or(0));
    let is_constant = constant.is_some() || line_degree == Some(0);

    // Horizontal asymptotes (one line for both sides when they agree; +∞
    // first, like the original).
    match a.horizontal.value() {
        Some(_) if is_constant => {}
        Some(v) => {
            let mut lines: Vec<(Num, AsymptoteSide)> = Vec::new();
            for h in v.iter().rev() {
                let n = Num {
                    exact: cx.tail_limit(h.side, h.y),
                    enc: h.y,
                };
                let side = match h.side {
                    Tail::Left => AsymptoteSide::NegativeInfinity,
                    Tail::Right => AsymptoteSide::PositiveInfinity,
                };
                let agree = |m: &Num| match (m.exact, n.exact) {
                    (Some(a), Some(b)) => same(a, b),
                    _ => m.enc == n.enc && m.enc.is_point(),
                };
                if let Some(l) = lines.iter_mut().find(|(m, _)| agree(m)) {
                    l.1 = AsymptoteSide::AnyInfinity;
                } else {
                    lines.push((n, side));
                }
            }
            out.settle(
                flags::HORIZONTAL_ASYMPTOTES,
                reach(&a.horizontal),
                lines.is_empty(),
                false,
            );
            if out.k.too_complex_features & flags::HORIZONTAL_ASYMPTOTES == 0 {
                out.k.horizontal_asymptotes = lines
                    .iter()
                    .map(|(n, _)| format!("y = {}", n.text()))
                    .collect();
                data.horizontal_asymptotes = lines.iter().map(|(n, s)| (n.value(), *s)).collect();
            }
        }
        None => out.unknown(flags::HORIZONTAL_ASYMPTOTES),
    }

    // Oblique asymptotes (+∞ first).
    match a.oblique.value() {
        Some(_) if line_degree == Some(1) => {}
        Some(v) => {
            let mut lines: Vec<(Num, Num, AsymptoteSide)> = Vec::new();
            for o in v.iter().rev() {
                let exact = cx.oblique_exact(o.side, o.m, o.b);
                let (m, b) = (
                    Num {
                        exact: exact.map(|e| e.0),
                        enc: o.m,
                    },
                    Num {
                        exact: exact.map(|e| e.1),
                        enc: o.b,
                    },
                );
                let side = match o.side {
                    Tail::Left => AsymptoteSide::NegativeInfinity,
                    Tail::Right => AsymptoteSide::PositiveInfinity,
                };
                let agree = |l: &(Num, Num, AsymptoteSide)| match (
                    l.0.exact, l.1.exact, m.exact, b.exact,
                ) {
                    (Some(m1), Some(b1), Some(m2), Some(b2)) => same(m1, m2) && same(b1, b2),
                    _ => false,
                };
                if let Some(l) = lines.iter_mut().find(|l| agree(l)) {
                    l.2 = AsymptoteSide::AnyInfinity;
                } else {
                    lines.push((m, b, side));
                }
            }
            out.settle(
                flags::OBLIQUE_ASYMPTOTES,
                reach(&a.oblique),
                lines.is_empty(),
                false,
            );
            if out.k.too_complex_features & flags::OBLIQUE_ASYMPTOTES == 0 {
                out.k.oblique_asymptotes = lines.iter().map(|(m, b, _)| line_text(m, b)).collect();
                data.oblique_asymptotes = lines
                    .iter()
                    .map(|(m, b, s)| (m.value(), b.value(), *s))
                    .collect();
            }
        }
        None => out.unknown(flags::OBLIQUE_ASYMPTOTES),
    }

    // Parity.
    out.k.parity = match a.parity.value() {
        Some(certify::Parity::Even | certify::Parity::Odd) if zero_function => Parity::Both,
        Some(certify::Parity::Even) => Parity::Even,
        Some(certify::Parity::Odd) => Parity::Odd,
        Some(certify::Parity::Neither) => Parity::Neither,
        None => {
            out.unknown(flags::PARITY);
            Parity::Unknown
        }
    };

    // Period.
    if constant.is_some() && domain_whole {
        out.k.periodicity_direction = Periodicity::Constant;
    } else {
        match a.period.value() {
            Some(Some(p)) => {
                let n = Num {
                    exact: cx.period_in(*p),
                    enc: *p,
                };
                out.k.periodicity_direction = Periodicity::Periodic;
                out.k.periodicity_expression = n.text();
                data.period = Some(n.value());
            }
            Some(None) => out.k.periodicity_direction = Periodicity::NotPeriodic,
            None => out.unknown(flags::PERIODICITY),
        }
    }

    // Monotonicity.
    let per: Option<Num> = match &a.monotonicity {
        Row::Certified { cert, .. } => match cert.covers {
            Region::Period { a: s0, b: s1 } => {
                let w = s1.0 - s0.0;
                cx.period
                    .filter(|p| (p.to_f64() - w).abs() <= 1e-9 * w)
                    .zip(cx.period_enc)
                    .map(|(p, enc)| Num {
                        exact: Some(p),
                        enc,
                    })
            }
            _ => None,
        },
        _ => None,
    };
    match a.monotonicity.value() {
        Some(v) => {
            let mut pieces: Vec<(f64, (String, Monotonicity, End, End))> = Vec::new();
            for m in v {
                let dir = match m.dir {
                    Dir::Increasing => Monotonicity::Increasing,
                    Dir::Decreasing => Monotonicity::Decreasing,
                    Dir::Constant => Monotonicity::Constant,
                };
                let ends = match &per {
                    Some(p) => cx
                        .periodic_xnum(&m.on.lo)
                        .zip(cx.periodic_xnum(&m.on.hi))
                        .map(|(l, h)| {
                            // Moved so the piece starts in [−P/2, P/2).
                            let pm = p.value();
                            let n = ((l.value() + pm / 2.0) / pm).floor();
                            if n == 0.0 || !n.is_finite() {
                                (l, h)
                            } else {
                                (shift(l, p, n), shift(h, p, n))
                            }
                        }),
                    None => None,
                };
                // Strictly monotone pieces are written open (the panel's
                // convention).
                let (lo, hi, text) = match (ends, &per) {
                    (Some((l, h)), Some(p)) => (
                        finite(l, false),
                        finite(h, false),
                        format!("({}, {}), k ∈ ℤ", family_text(&l, p), family_text(&h, p)),
                    ),
                    _ => {
                        let (mut lo, mut hi) = (cx.end(&m.on.lo), cx.end(&m.on.hi));
                        lo.closed = false;
                        hi.closed = false;
                        let t = interval_text(&lo, &hi);
                        (lo, hi, t)
                    }
                };
                pieces.push((lo.value, (text, dir, lo, hi)));
            }
            pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
            out.settle(
                flags::MONOTONE_INTERVALS,
                reach(&a.monotonicity),
                pieces.is_empty(),
                false,
            );
            if out.k.too_complex_features & flags::MONOTONE_INTERVALS == 0 {
                for (_, (text, dir, lo, hi)) in pieces {
                    data.monotonicity.push((data_interval(&lo, &hi), dir));
                    out.k.monotonicity.push((text, dir));
                }
            }
        }
        None => out.unknown(flags::MONOTONE_INTERVALS),
    }

    out.k.data = data;
    out.k
}

/// Extrema or inflection points: `(x, y)` texts, their numbers, and
/// whether the list was cut.
#[allow(clippy::type_complexity)]
fn points(
    cx: &Ctx<'_>,
    pts: &[(Enc, Enc, Option<Enc>)],
) -> (Vec<String>, Vec<(DataFamily, f64)>, bool) {
    let mut items: Vec<(f64, (Num, Option<Num>, Num))> = Vec::new();
    let mut fams: Vec<(Num, Num, String)> = Vec::new();
    let mut fam_y: Vec<Num> = Vec::new();
    for &(x, y, every) in pts {
        let xn = cx.x(x);
        let yn = cx.y_at(xn.exact, y);
        match every {
            None => items.push((xn.value(), (xn, None, yn))),
            Some(p) => {
                let pn = Num {
                    exact: cx.period_in(p),
                    enc: p,
                };
                let (xn, pn) = normalize(xn, pn);
                fams.push((xn, pn, yn.text()));
                fam_y.push(yn);
            }
        }
    }
    for (x, p, tag) in merge(fams) {
        let y = fam_y
            .iter()
            .find(|y| y.text() == tag)
            .copied()
            .expect("a family's value");
        items.push((x.value(), (x, Some(p), y)));
    }
    let was_cut = cut(&mut items);
    let mut texts = Vec::new();
    let mut data = Vec::new();
    for (_, (x, p, y)) in items {
        match p {
            Some(p) => {
                texts.push(format!("({}, {}), k ∈ ℤ", family_text(&x, &p), y.text()));
                data.push((
                    DataFamily {
                        x: x.value(),
                        period: Some(p.value()),
                    },
                    y.value(),
                ));
            }
            None => {
                texts.push(format!("({}, {})", x.text(), y.text()));
                data.push((DataFamily::single(x.value()), y.value()));
            }
        }
    }
    (texts, data, was_cut)
}

/// `y = m·x + b` in the panel's style: `y = x + 1`, `y = −2x`,
/// `y = x/2 − 3`.
fn line_text(m: &Num, b: &Num) -> String {
    let mx = match m.exact.and_then(Ex::rational) {
        Some(q) => {
            let (p, d) = (q.numer(), q.denom());
            let sign = if p < 0 { MINUS } else { "" };
            let num = if p.unsigned_abs() == 1 {
                "x".to_string()
            } else {
                format!("{}x", p.unsigned_abs())
            };
            if d == 1 {
                format!("{sign}{num}")
            } else {
                format!("{sign}{num}/{d}")
            }
        }
        None => format!("{}x", m.text()),
    };
    if b.is_zero() {
        return format!("y = {mx}");
    }
    let neg = match b.exact {
        Some(e) => e.sign() == Some(-1),
        None => b.enc.hi.0 < 0.0,
    };
    if neg {
        let nb = Num {
            exact: b.exact.and_then(Ex::neg),
            enc: Enc::new(-b.enc.hi.0, -b.enc.lo.0),
        };
        format!("y = {mx} {MINUS} {}", nb.text())
    } else {
        format!("y = {mx} + {}", b.text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplest_rationals() {
        assert_eq!(simplest(0.49, 0.51), Q::new(1, 2));
        assert_eq!(simplest(-0.34, -0.33), Q::new(-1, 3));
        assert_eq!(simplest(2.9, 3.1), Some(Q::int(3)));
        assert_eq!(simplest(-1.0, 1.0), Some(Q::ZERO));
    }

    #[test]
    fn approximate_text() {
        assert_eq!(
            approx(Enc::new(1.4142135623730949, 1.4142135623730951)),
            "1.41421"
        );
        assert_eq!(
            approx(Enc::new(0.9999999999999999, 1.0000000000000002)),
            "≈1"
        );
        assert_eq!(approx(Enc::new(-1e-17, 1e-17)), "≈0");
        assert_eq!(approx(Enc::point(0.0)), "0");
        assert_eq!(
            approx(Enc::new(0.49999999999999994, 0.5000000000000001)),
            "≈0.5"
        );
        // Only the digits the enclosure fixes.
        assert_eq!(approx(Enc::new(2.41, 2.42)), "≈2.4");
        // None fixed: the enclosure, rounded outward.
        assert_eq!(approx(Enc::new(-1.5707963267948966, 0.0)), "[−1.6, 0]");
        assert_eq!(approx(Enc::new(0.851, 1.05)), "[0.85, 1.1]");
    }
}
