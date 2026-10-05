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
//! enclosure fixes (at most six significant; from 10⁶ on as m×10ⁿ),
//! marked "≈": every value that isn't exact is. Two different numbers of
//! a row that would read alike get up to fifteen, to tell them apart (or
//! the row is unknown); equal texts never make two numbers one.

use std::cell::OnceCell;
use std::sync::atomic::AtomicBool;

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
/// rational r in its square, ±eᵏ for the simplest rational k in its
/// logarithm, ln(r)/m for the simplest rational r in eᵐˣ (m ≤ 3).
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
        let (m, n) = (lo.abs().min(hi.abs()), lo.abs().max(hi.abs()));
        let (a, b) = widen(m.ln(), n.ln());
        if let Some(k) = simplest(a, b)
            && let Some(v) = Ex::int(1).exp_times(k)
        {
            out.extend(if hi < 0.0 { v.neg() } else { Some(v) });
        }
    }
    // ln(r)/m: eᵐˣ = r (e²ˣ − 4x turns at ln(2)/2).
    for m in 1..=3 {
        let (a, b) = widen((lo * f64::from(m)).exp(), (hi * f64::from(m)).exp());
        if a > 0.0
            && b.is_finite()
            && let Some(r) = simplest(a, b)
            && let Some(v) = Ex::ln_q(r).and_then(|v| v.div(Ex::int(m.into())))
        {
            out.push(v);
        }
    }
    out
}

/// Whether two of `texts` read alike (they can't be told apart).
fn alike(texts: &[String]) -> bool {
    texts
        .iter()
        .enumerate()
        .any(|(i, t)| texts[..i].contains(t))
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

thread_local! {
    /// Set when a number was written as its enclosure (not one digit
    /// known): the row it is in is shown as unknown instead.
    static UNFIXED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether a number since the last call was written as an enclosure.
fn take_unfixed() -> bool {
    UNFIXED.with(|u| u.replace(false))
}

/// The fewest significant digits a rounded value is shown with: an
/// enclosure that fixes fewer leaves its row unknown. This is the display
/// policy's one switch (docs/ti-conventions.md): 6 restores the strict
/// rule, all six digits fixed or nothing shown.
pub const MIN_SHOWN_DIGITS: i32 = 3;

/// A value known to lie in `e`: as many significant digits (up to six, at
/// least [`MIN_SHOWN_DIGITS`]) as every value in `e` rounds to alike,
/// marked "≈" as it isn't exact ("≈1", "≈0.5", "≈1.41421", "≈2.30062×10⁶");
/// a point is its double, exactly, when that has a short exact text.
pub(super) fn approx(e: Enc) -> String {
    approx_sig(e, 6)
}

/// The text of `e` to `sig` significant digits when every value in it
/// rounds alike to that many (and it can't be 0).
fn fixed_to(e: Enc, sig: i32) -> Option<String> {
    let (lo, hi) = (e.lo.0, e.hi.0);
    if lo <= 0.0 && 0.0 <= hi {
        return None;
    }
    let a = format_decimal_digits(lo, sig);
    (a == format_decimal_digits(hi, sig)).then_some(a)
}

/// [`approx`] with up to `max` significant digits (more than six to tell
/// two close points apart), marked "≈" whatever their number.
fn approx_sig(e: Enc, max: i32) -> String {
    let (lo, hi) = (e.lo.0, e.hi.0);
    if e.is_point() && lo == 0.0 {
        return "0".into();
    }
    // A point is that double, exactly: its own text when it has a short one.
    if e.is_point()
        && let Some(t) = Q::from_f64(lo).and_then(|q| Ex::q(q).text())
    {
        return t;
    }
    // A value that may be 0 is never written as 0, nor ≈0: not one digit
    // of it is known (`fixed_to`). As many significant digits as are
    // known, up to six (or the more asked for), and at least the minimum.
    for sig in (MIN_SHOWN_DIGITS.min(6)..=max.max(6)).rev() {
        if let Some(a) = fixed_to(e, sig) {
            return format!("≈{a}");
        }
    }
    // Too few digits fixed: the enclosure itself, rounded outward (the
    // row it is in is shown as unknown).
    UNFIXED.with(|u| u.set(true));
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
        self.text_sig(6)
    }

    fn text_sig(&self, max: i32) -> String {
        self.exact
            .and_then(Ex::text)
            .unwrap_or_else(|| approx_sig(self.enc, max))
    }

    fn value(&self) -> f64 {
        match self.exact {
            Some(e) => e.to_f64(),
            None => self.enc.mid(),
        }
    }

    /// Exact, or with at least one significant digit fixed.
    fn fixed(&self) -> bool {
        let unfixed = take_unfixed();
        let ok = self.exact.is_some() || !approx(self.enc).starts_with('[');
        take_unfixed();
        if unfixed {
            UNFIXED.with(|u| u.set(true));
        }
        ok
    }

    fn is_zero(&self) -> bool {
        self.exact.is_some_and(Ex::is_zero) || (self.enc.is_point() && self.enc.lo.0 == 0.0)
    }
}

/// `k`-multiples of a period: `2kπ`, `kπ/2`, `360k`, `≈9.8696k` (an
/// approximate period stays marked).
fn times_k(p: &Num) -> String {
    p.exact
        .and_then(Ex::times_k)
        .unwrap_or_else(|| format!("{}k", approx(p.enc)))
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
    if pm.is_nan() || pm <= 0.0 {
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
    // The same family twice (tan's poles from tan itself and from it as a
    // divisor): once.
    let mut items = items;
    let mut i = 0;
    while i < items.len() {
        let dup = (0..i).any(|j| {
            let (a, b) = (&items[j], &items[i]);
            a.2 == b.2
                && match (a.0.exact, a.1.exact, b.0.exact, b.1.exact) {
                    (Some(xa), Some(pa), Some(xb), Some(pb)) => {
                        same(pa, pb)
                            && xb
                                .sub(xa)
                                .and_then(|d| d.div(pa))
                                .and_then(Ex::rational)
                                .is_some_and(|t| t.is_int())
                    }
                    // The certifier's very enclosures twice (one family
                    // from two side conditions): the same set.
                    _ => a.0.enc == b.0.enc && a.1.enc == b.1.enc,
                }
        });
        if dup {
            items.remove(i);
        } else {
            i += 1;
        }
    }
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
    /// Points the certifier pinned exactly, tried first as roots.
    hints: Vec<Q>,
}

impl Rat {
    fn new(r: &Rational, hints: Vec<Q>) -> Option<Rat> {
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
            hints,
        })
    }

    fn roots(&self, k: usize) -> &[Ex] {
        self.roots[k].get_or_init(|| exact::exact_roots_hinted(&self.p[k], &self.hints))
    }
}

/// Every number in the tree's exponents is exactly its double (a power's
/// rational form and derivative read its exponent off the double).
fn honest_exponents(e: &Expr, _: &ExactLiterals) -> bool {
    let mut ok = true;
    e.visit(&mut |n| {
        if let Expr::Bin(BinOp::Pow, _, b) = n {
            b.visit(&mut |m| {
                if let Expr::Num(_, lit) = m
                    && !lit.is_exact()
                {
                    ok = false;
                }
            });
        }
    });
    ok
}

/// f′ and f″ as trees whose constants read back exactly: the derivative
/// copies f's own numbers (each with its exact value), makes integers from
/// exponents (checked honest), folds numbers only where that is exact, and
/// multiplies in ln 10 and the angle-unit factor (numbers not known
/// exactly, which no typed number is taken for). Numbers typed with more
/// than 15 significant digits are refused.
fn derivative_trees(f: &Expr, unit: TrigUnit, lits: &ExactLiterals) -> [Option<Expr>; 2] {
    let none = [None, None];
    if !honest_exponents(f, lits) {
        return none;
    }
    let mut ok = true;
    f.visit(&mut |n| {
        if let Expr::Num(v, lit) = n {
            let short = lit.q(*v).is_some_and(|q| {
                q.denom() <= 1_000_000_000_000_000
                    && q.numer().unsigned_abs() <= 1_000_000_000_000_000
            });
            if !short {
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
    /// The exact evaluations' work so far (nodes evaluated).
    work: std::cell::Cell<u64>,
}

/// The largest tree the panel evaluates exactly (f″ of sin nested forty
/// deep has 26 000 nodes: 9 ms an evaluation).
const EXACT_NODES: usize = 4096;
/// The panel's exact work, in nodes evaluated (about 0.3 µs each): a few
/// hundred milliseconds at most.
const EXACT_WORK: u64 = 1_000_000;

/// The certifier's evaluation budget for the panel's own checks (f
/// finite on a box), in its units.
const PANEL_BUDGET: u64 = 20_000;

impl<'a> Ctx<'a> {
    fn new(
        f: Expr,
        opts: &'a CompileOptions<'a>,
        lits: &'a ExactLiterals,
        ilits: &'a Literals,
        a: &Analysis,
        cancel: Option<&'a AtomicBool>,
    ) -> Ctx<'a> {
        let unit = opts.trig_unit;
        // (Sliders at their values: a/x with a at 0 is 0 off x = 0.)
        let fv = with_values(&f, opts.variables);
        let rf = honest_exponents(&fv, lits)
            .then(|| rational_form(&fv, lits))
            .flatten();
        // Every x the certifier pinned to a double.
        let mut hints: Vec<Q> = Vec::new();
        let mut hint = |e: Enc| {
            if e.is_point()
                && let Some(q) = Q::from_f64(e.lo.0)
                && !hints.contains(&q)
            {
                hints.push(q);
            }
        };
        for e in a.extrema.value().into_iter().flatten() {
            hint(e.x);
        }
        for i in a.inflections.value().into_iter().flatten() {
            hint(i.x);
        }
        for s in a.x_intercepts.value().into_iter().flatten() {
            if let Spot::At(x) = s {
                hint(*x);
            }
        }
        let rat = rf.as_ref().and_then(|r| Rat::new(&r.reduced, hints));
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
        let fun = Fun::new(&f, ilits, *opts, PANEL_BUDGET, cancel);
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
            work: std::cell::Cell::new(0),
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

    /// e at x̂ exactly, within the panel's exact work ([`EXACT_NODES`],
    /// [`EXACT_WORK`]): past it (or once cancelled), not known exactly,
    /// and the panel writes the certifier's enclosure instead.
    fn eval(&self, e: &Expr, x: Ex) -> Option<Ex> {
        if self.fun.cancelled() {
            return None;
        }
        let size = e.depth_and_size().1;
        let work = self.work.get() + size as u64;
        if size > EXACT_NODES || work > EXACT_WORK {
            return None;
        }
        self.work.set(work);
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
            Expr::Bin(BinOp::Add | BinOp::Sub, _, _) => self.common_factor_zero(e, at, x),
            _ => false,
        }
    }

    /// A sum whose terms share a factor c (eˣ·(1 − x) written out as
    /// eˣ − x·eˣ): c finite on the box and the sum of the rest exactly 0.
    fn common_factor_zero(&self, e: &Expr, at: Ex, x: Enc) -> bool {
        let mut ts = Vec::new();
        terms(e, false, &mut ts);
        let split: Vec<(bool, Vec<&Expr>)> = ts
            .iter()
            .map(|&(neg, t)| {
                let mut neg = neg;
                let mut fs = Vec::new();
                factors(t, &mut neg, &mut fs);
                (neg, fs)
            })
            .collect();
        let Some((_, first)) = split.first() else {
            return false;
        };
        for c in first.iter().filter(|c| c.contains_x()) {
            if !split.iter().all(|(_, fs)| fs.contains(c)) || !self.finite_on(c, x, false) {
                continue;
            }
            let mut sum = Some(Ex::int(0));
            for (neg, fs) in &split {
                let mut rest = fs.clone();
                let i = rest.iter().position(|f| f == c).expect("shared");
                rest.remove(i);
                let mut prod = Some(Ex::int(if *neg { -1 } else { 1 }));
                for f in rest {
                    prod = prod.and_then(|p| p.mul(self.eval(f, at)?));
                }
                sum = sum.zip(prod).and_then(|(s, p)| s.add(p));
            }
            if sum.is_some_and(Ex::is_zero) {
                return true;
            }
        }
        false
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
            .filter(|v| v.agrees(y.lo.0, y.hi.0));
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
        let (g, alpha, beta, p) = self.table_period(x0, period)?;
        let h = self.half_turn();
        let pf = p.to_f64();
        if x0.hi.0 - x0.lo.0 >= pf / 2.0 || pf.is_nan() {
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

    /// A table family's period, exactly (a half or whole turn over |α| for
    /// the argument α·x + β of its side expression g), with g, α and β:
    /// exact even where x₀ has no closed form (tan(x + 0.5)'s poles every
    /// π).
    fn table_period(&self, x0: Enc, period: Enc) -> Option<(Expr, Ex, Ex, Ex)> {
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
        if period.hi.0 - period.lo.0 >= pf / 4.0 || pf.is_nan() {
            return None;
        }
        Some((g, alpha, beta, p))
    }

    /// The domain equals its mirror image: no excluded families, and its
    /// pieces' ends, exactly, are those of a piece mirrored.
    fn symmetric(&self, d: &certify::DomainValue) -> bool {
        if !d.excluded.is_empty() {
            return false;
        }
        // An end as (exact value, closed); ±∞ as None.
        let end = |b: &Bound| -> Option<Option<(Ex, bool)>> {
            match b {
                Bound::NegInf | Bound::PosInf => Some(None),
                Bound::At { x, closed } => Some(Some((self.exact_x(*x)?, *closed))),
            }
        };
        let same_end = |a: &Option<(Ex, bool)>, b: &Option<(Ex, bool)>, neg: bool| match (a, b) {
            (None, None) => true,
            (Some((x, c)), Some((y, d))) => {
                c == d && (if neg { y.neg() } else { Some(*y) }).is_some_and(|y| same(*x, y))
            }
            _ => false,
        };
        let mut ends = Vec::new();
        for p in &d.pieces {
            let (Some(lo), Some(hi)) = (end(&p.lo), end(&p.hi)) else {
                return false;
            };
            // A piece reaching −∞ must mirror one reaching +∞.
            let (lo_inf, hi_inf) = (p.lo == Bound::NegInf, p.hi == Bound::PosInf);
            ends.push((lo, hi, lo_inf, hi_inf));
        }
        ends.iter().all(|(lo, hi, li, hi_inf)| {
            ends.iter().any(|(lo2, hi2, li2, hi2_inf)| {
                li == hi2_inf && hi_inf == li2 && same_end(lo, hi2, true) && same_end(hi, lo2, true)
            })
        })
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
                    exact: self
                        .table_period(x0, p)
                        .map(|t| t.3)
                        .or_else(|| self.xfam_period(p))
                        .or_else(|| self.period_in(p)),
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
        v.agrees(y.lo.0, y.hi.0).then_some(v)
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

/// A sum's terms, with whether each is subtracted.
fn terms<'e>(e: &'e Expr, neg: bool, out: &mut Vec<(bool, &'e Expr)>) {
    match e {
        Expr::Bin(BinOp::Add, a, b) => {
            terms(a, neg, out);
            terms(b, neg, out);
        }
        Expr::Bin(BinOp::Sub, a, b) => {
            terms(a, neg, out);
            terms(b, !neg, out);
        }
        Expr::Neg(a) => terms(a, !neg, out),
        _ => out.push((neg, e)),
    }
}

/// A product's factors (negations folded into `neg`).
fn factors<'e>(e: &'e Expr, neg: &mut bool, out: &mut Vec<&'e Expr>) {
    match e {
        Expr::Bin(BinOp::Mul, a, b) => {
            factors(a, neg, out);
            factors(b, neg, out);
        }
        Expr::Neg(a) => {
            *neg = !*neg;
            factors(a, neg, out);
        }
        _ => out.push(e),
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
    /// The number, for a finite end.
    num: Option<Num>,
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
        num: None,
    }
}

fn finite(n: Num, closed: bool) -> End {
    End {
        text: n.text(),
        closed,
        inf: None,
        value: n.value(),
        num: Some(n),
    }
}

/// Two numbers proven equal: exact and the same, or both exactly the same
/// double. (Equal texts prove nothing: ≈0.841471 is a great many numbers.)
fn identical(a: &Num, b: &Num) -> bool {
    match (a.exact, b.exact) {
        (Some(x), Some(y)) if same(x, y) => true,
        _ => a.enc.is_point() && a.enc == b.enc,
    }
}

/// What `n` reads as to `sig` significant digits, for telling numbers
/// apart (an exact one by its value): `None` if its enclosure doesn't fix
/// that many.
fn reading(n: &Num, sig: i32) -> Option<String> {
    if n.is_zero() {
        return Some("0".into());
    }
    match n.exact {
        Some(e) => Some(format_decimal_digits(e.to_f64(), sig)),
        None => fixed_to(n.enc, sig),
    }
}

/// The digits that tell `a` and `b` apart (7 to 15 significant), if any.
fn apart(a: &Num, b: &Num) -> Option<i32> {
    (7..=15).find(|&s| matches!((reading(a, s), reading(b, s)), (Some(x), Some(y)) if x != y))
}

/// Gives the ends of one row that would read alike where that would say
/// something false the significant digits that tell them apart (up to 15,
/// as the points' rows do; exact ones stay exact). `sets` (indices into
/// `ends`) are where distinct numbers must read apart: an interval's two
/// bounds, and the points a set lists (excluded points, or a range of
/// single values). Ends in one class (`ends[i].0`) are one number, and
/// so are numbers proven equal: never told apart. Two that still read
/// alike can't be told apart: the row is shown as unknown.
fn tell_apart(ends: &mut [(usize, &mut End)], sets: &[Vec<usize>]) {
    use std::collections::{HashMap, HashSet};
    let mut need: HashMap<usize, i32> = HashMap::new();
    let mut done: HashSet<(usize, usize)> = HashSet::new();
    for set in sets {
        // One finite end per class, grouped by what it reads as (its text,
        // and its value to six digits): only ends of one group read alike.
        let mut classes_seen: HashSet<usize> = HashSet::new();
        let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
        for &i in set {
            let (c, e) = (&ends[i].0, &ends[i].1);
            let Some(n) = e.num else { continue };
            if !classes_seen.insert(*c) {
                continue;
            }
            groups.entry(format!("t{}", e.text)).or_default().push(i);
            if let Some(r) = reading(&n, 6) {
                groups.entry(format!("r{r}")).or_default().push(i);
            }
        }
        for g in groups.values().filter(|g| g.len() > 1) {
            // So many numbers alike can't be told apart in a row.
            if g.len() > 64 {
                UNFIXED.with(|u| u.set(true));
                continue;
            }
            for (k, &i) in g.iter().enumerate() {
                for &j in &g[k + 1..] {
                    let (Some(a), Some(b)) = (ends[i].1.num, ends[j].1.num) else {
                        continue;
                    };
                    if !done.insert((i.min(j), i.max(j))) || identical(&a, &b) {
                        continue;
                    }
                    match apart(&a, &b) {
                        Some(s) => {
                            for c in [ends[i].0, ends[j].0] {
                                let e = need.entry(c).or_insert(6);
                                *e = (*e).max(s);
                            }
                        }
                        None => UNFIXED.with(|u| u.set(true)),
                    }
                }
            }
        }
    }
    // One class, one text.
    for (c, e) in ends.iter_mut() {
        if let Some(&s) = need.get(c)
            && s > 6
            && let Some(num) = e.num
            && num.exact.and_then(Ex::text).is_none()
        {
            e.text = num.text_sig(s);
        }
    }
}

/// Classes for [`tell_apart`]: ends with the same enclosure (an excluded
/// point that ends one piece and starts the next) are one number.
fn classes(ends: &[&End]) -> Vec<usize> {
    let mut seen: std::collections::HashMap<(u64, u64), usize> = Default::default();
    (0..ends.len())
        .map(|i| match ends[i].num {
            Some(n) => *seen
                .entry((n.enc.lo.0.to_bits(), n.enc.hi.0.to_bits()))
                .or_insert(i),
            None => i,
        })
        .collect()
}

impl Ctx<'_> {
    fn end(&self, b: &Bound) -> End {
        match b {
            Bound::NegInf => infinite(false),
            Bound::PosInf => infinite(true),
            Bound::At { x, closed } => {
                let mut n = self.x(*x);
                if n.exact.is_none() && !*closed {
                    // An excluded point of an exact family.
                    n.exact = self.member(*x);
                }
                finite(n, *closed)
            }
        }
    }

    /// A range end: exact from where the certifier says it comes from
    /// (every source exact and equal, when it is the hull of several).
    fn range_end(&self, b: &Bound, srcs: Option<&[EndSrc]>, low: bool) -> End {
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
        // A closed end is a value f takes: written only when exact or known
        // to the last few doubles (no rounding allowance for "attained").
        let tight = |e: Enc| {
            let m = e.lo.0.abs().max(e.hi.0.abs());
            e.hi.0 - e.lo.0 <= 32.0 * f64::EPSILON * m
        };
        if closed && exact.is_none() && !tight(y) {
            UNFIXED.with(|u| u.set(true));
        }
        let mut e = finite(Num { exact, enc: y }, closed);
        // An open end known only to an enclosure: the numbers keep its
        // outer end (the range lies within it).
        if exact.is_none() && !closed {
            e.value = if low { y.lo.0 } else { y.hi.0 };
        }
        e
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
/// `joined(i)`: pieces i and i + 1 meet at one excluded point; `one(i)`:
/// piece i's two ends are proven one number (a single point). Ends that
/// read alike are told apart, or the row is unknown.
fn set_text(
    var: &str,
    parts: &mut [(End, End)],
    joined: &dyn Fn(usize) -> bool,
    one: &dyn Fn(usize) -> bool,
) -> String {
    if parts.is_empty() {
        return format!("{var} ∈ ∅");
    }
    let n = parts.len();
    // A single point only when its ends are proven one number.
    let single: Vec<bool> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            p.0.inf.is_none()
                && p.0.closed
                && p.1.closed
                && (one(i) || matches!((p.0.num, p.1.num), (Some(a), Some(b)) if identical(&a, &b)))
        })
        .collect();
    let all_but_points = parts[0].0.inf == Some(false)
        && parts[n - 1].1.inf == Some(true)
        && (0..n - 1).all(|i| joined(i) && !parts[i].1.closed && !parts[i + 1].0.closed);
    {
        // Where distinct numbers must read apart: an interval's two
        // bounds, and the points a set lists (the excluded points of
        // ℝ \ {…}, the single values of a range).
        let mut sets: Vec<Vec<usize>> = (0..n)
            .filter(|&i| !single[i])
            .map(|i| vec![2 * i, 2 * i + 1])
            .collect();
        if all_but_points {
            sets.push((0..n - 1).map(|i| 2 * i + 1).collect());
        }
        sets.push((0..n).filter(|&i| single[i]).map(|i| 2 * i).collect());
        let refs: Vec<&End> = parts.iter().flat_map(|p| [&p.0, &p.1]).collect();
        let mut cls = classes(&refs);
        // A point's two ends are one number; an interval's never are.
        for (i, p) in single.iter().enumerate() {
            let (a, b) = (2 * i, 2 * i + 1);
            if *p {
                cls[b] = cls[a];
            } else if cls[b] == cls[a] {
                cls[b] = refs.len() + b;
            }
        }
        let mut ends: Vec<(usize, &mut End)> = parts
            .iter_mut()
            .flat_map(|p| [&mut p.0, &mut p.1])
            .enumerate()
            .map(|(i, e)| (cls[i], e))
            .collect();
        tell_apart(&mut ends, &sets);
    }
    // An interval whose ends still read alike says nothing.
    if parts
        .iter()
        .zip(&single)
        .any(|(p, s)| !s && p.0.inf.is_none() && p.0.text == p.1.text)
    {
        UNFIXED.with(|u| u.set(true));
    }
    if all_but_points {
        if n == 1 {
            return format!("{var} ∈ ℝ");
        }
        let pts: Vec<String> = parts[..n - 1].iter().map(|p| p.1.text.clone()).collect();
        // Two excluded points that read alike can't be told apart.
        if alike(&pts) {
            UNFIXED.with(|u| u.set(true));
        }
        return format!("{var} ∈ ℝ \\ {{{}}}", pts.join(", "));
    }
    if single.iter().all(|s| *s) {
        let pts: Vec<&str> = parts.iter().map(|p| p.0.text.as_str()).collect();
        return format!("{var} ∈ {{{}}}", pts.join(", "));
    }
    let items: Vec<String> = parts
        .iter()
        .zip(&single)
        .map(|(p, point)| {
            if *point {
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

    /// Drops the notes of rows now shown as unknown.
    fn drop_notes(&mut self, flags: u32) {
        self.k.partial_features &= !flags;
        self.k.notes.retain(|(f, _)| f & flags == 0);
    }

    /// Flags and the note for one list row of the panel: `empty`, it shows
    /// no item; `was_cut`, items were left out.
    fn settle(&mut self, flag: u32, r: Reach, empty: bool, was_cut: bool) {
        let window = |t: &str, a: f64, b: f64| {
            let num = |v: f64| Num {
                exact: Q::from_f64(v).map(Ex::q),
                enc: Enc::point(v),
            };
            t.replace("%1", &num(a).text())
                .replace("%2", &num(b).text())
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
    cancel: Option<&AtomicBool>,
) -> Option<KeyGraphFeatures> {
    let cx = Ctx::new(certify::canonical(f), opts, lits, ilits, a, cancel);
    let mut out = Out {
        k: KeyGraphFeatures::default(),
    };
    take_unfixed();
    let mut data = AnalysisData::default();

    // A rational function that reduces to a constant c is c wherever it is
    // defined (its holes aside).
    let rat_const: Option<Q> = cx
        .reduced
        .as_ref()
        .filter(|r| r.den.degree() == Some(0) && r.num.degree().unwrap_or(0) == 0)
        .map(|r| {
            r.num
                .coefficients()
                .first()
                .copied()
                .unwrap_or(Q::ZERO)
                .div(r.den.lead())
                .unwrap_or(Q::ZERO)
        });
    let domain_nonempty =
        matches!(&a.domain, Row::Certified { value, .. } if !value.pieces.is_empty());
    // Or one free of x whose value is exact (sin(π) is 0, not the residue
    // its double leaves).
    let exact_const: Option<Ex> = rat_const
        .map(Ex::q)
        .or_else(|| {
            (!cx.f.contains_x())
                .then(|| cx.eval(&cx.f, Ex::int(0)))
                .flatten()
        })
        .filter(|_| domain_nonempty);
    let const_enc = |c: Ex| -> Enc {
        match c.rational() {
            Some(q) => {
                let iv = q.interval();
                Enc::new(iv.lo(), iv.hi())
            }
            None => {
                let (lo, hi) = widen(c.to_f64(), c.to_f64());
                Enc::new(lo, hi)
            }
        }
    };

    // Constant on its domain: the range is one value.
    let constant: Option<Enc> = match &a.range {
        _ if exact_const.is_some() => exact_const.map(const_enc),
        Row::Certified { value, .. } if value.len() == 1 && one_value(a, 0) => match value[0] {
            Piece {
                lo: Bound::At { x: l, .. },
                ..
            } => Some(l),
            _ => None,
        },
        _ => None,
    };
    let zero_function = exact_const.map_or(
        constant.is_some_and(|c| c.is_point() && c.lo.0 == 0.0),
        Ex::is_zero,
    );
    // A constant on its domain has no strict extremum, no inflection, no
    // pole, no zero unless it is 0, and is constant on each piece of its
    // domain: rows the certifier left open follow from that.
    let owned;
    let a = match constant {
        Some(c) => {
            owned = constant_rows(a, c);
            &owned
        }
        None => a,
    };

    // Domain.
    let mut domain_whole = false;
    match &a.domain {
        Row::Certified { value, .. } => {
            let mut parts: Vec<(End, End)> = value
                .pieces
                .iter()
                .map(|p| (cx.end(&p.lo), cx.end(&p.hi)))
                .collect();
            let joined = |i: usize| match (value.pieces[i].hi, value.pieces[i + 1].lo) {
                (Bound::At { x: a, .. }, Bound::At { x: b, .. }) => a == b,
                _ => false,
            };
            // A piece written from one bound to itself is that one point.
            let one = |i: usize| value.pieces[i].lo == value.pieces[i].hi;
            let mut text = set_text("x", &mut parts, &joined, &one);
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

    if take_unfixed() {
        out.k.domain.clear();
        data.domain.clear();
        data.excluded.clear();
        out.unknown(flags::DOMAIN);
    }

    // Range.
    let range_row = match exact_const {
        // Its one value, exactly.
        Some(_) => Row::unknown("written from the exact constant"),
        None => a.range.clone(),
    };
    match &range_row {
        Row::Certified { value, .. } => {
            let mut parts: Vec<(End, End)> = value
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let srcs = a.range_ends.get(i);
                    (
                        cx.range_end(&p.lo, srcs.map(|s| s[0].as_slice()), true),
                        cx.range_end(&p.hi, srcs.map(|s| s[1].as_slice()), false),
                    )
                })
                .collect();
            let joined = |i: usize| match (value[i].hi, value[i + 1].lo) {
                (Bound::At { x: a, .. }, Bound::At { x: b, .. }) => a == b,
                _ => false,
            };
            let one = |i: usize| one_value(a, i);
            out.k.range = set_text("y", &mut parts, &joined, &one);
            data.range = parts.iter().map(|(a, b)| data_interval(a, b)).collect();
        }
        _ => match exact_const {
            Some(c) => {
                let n = Num {
                    exact: Some(c),
                    enc: constant.expect("a constant"),
                };
                out.k.range = format!("y ∈ {{{}}}", n.text());
                data.range = vec![format::Interval::closed(n.value(), n.value())];
            }
            None => out.unknown(flags::RANGE),
        },
    }

    if take_unfixed() {
        out.k.range.clear();
        data.range.clear();
        out.unknown(flags::RANGE);
    }

    // x-intercepts: the row's, and any extremum or inflection whose value
    // is exactly 0 (a touching zero the row couldn't prove); with any of
    // those, the list is only some of them unless the row was complete.
    let mut spots: Vec<Spot> = a.x_intercepts.value().cloned().unwrap_or_default();
    let mut added = false;
    if !matches!(a.x_intercepts, Row::Certified { .. }) {
        let mut pts: Vec<(Enc, Enc, Option<Enc>)> = Vec::new();
        if let Some(v) = a.extrema.value() {
            pts.extend(v.iter().map(|e| (e.x, e.y, e.every)));
        }
        if let Some(v) = a.inflections.value() {
            pts.extend(v.iter().map(|i| (i.x, i.y, i.every)));
        }
        for (x, y, every) in pts {
            let xe = cx.exact_x(x);
            let zero =
                cx.y_at(xe, y).exact.is_some_and(Ex::is_zero) || (y.is_point() && y.lo.0 == 0.0);
            let known = spots.iter().any(|s| match s {
                Spot::At(z) => z.lo.0 <= x.hi.0 && x.lo.0 <= z.hi.0,
                Spot::Every(f) => f.x0.lo.0 <= x.hi.0 && x.lo.0 <= f.x0.hi.0,
            });
            if zero && !known {
                spots.push(match every {
                    None => Spot::At(x),
                    Some(p) => Spot::Every(certify::Family { x0: x, period: p }),
                });
                added = true;
            }
        }
    }
    let zeros_reach = match reach(&a.x_intercepts) {
        Reach::Unknown if added => Reach::Part(None),
        r => r,
    };
    match (!spots.is_empty() || a.x_intercepts.value().is_some()).then_some(&spots) {
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
            out.settle(flags::ZEROS, zeros_reach, items.is_empty(), was_cut);
            // Two zeros that read alike get the digits that tell them
            // apart (1 ± 10⁻¹⁵), as the points' rows do; if they still
            // read alike, the row is unknown.
            let xs: Vec<String> = items.iter().map(|(_, (x, _))| x.text()).collect();
            let mut texts = Vec::new();
            let mut any_family = false;
            for (i, (_, (x, p))) in items.iter().enumerate() {
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
                        let twins = xs.iter().filter(|t| **t == xs[i]).count() > 1;
                        texts.push(x.text_sig(if twins { 15 } else { 6 }));
                        data.zeros.push(DataFamily::single(x.value()));
                    }
                }
            }
            if alike(&texts) {
                UNFIXED.with(|u| u.set(true));
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

    if take_unfixed() {
        out.k.x_intercept.clear();
        data.zeros.clear();
        out.drop_notes(flags::ZEROS);
        out.unknown(flags::ZEROS);
    }

    // y-intercept (a constant's own value, exactly, when 0 is in its
    // domain: its enclosure at 0 may be wider, or not proven).
    let zero_in_domain = match &a.domain {
        Row::Certified { value, .. } => {
            value.excluded.is_empty()
                && value.pieces.iter().any(|p| {
                    let lo_ok = match p.lo {
                        Bound::NegInf => true,
                        Bound::At { x, closed } => {
                            x.hi.0 < 0.0 || (closed && x.is_point() && x.lo.0 == 0.0)
                        }
                        Bound::PosInf => false,
                    };
                    let hi_ok = match p.hi {
                        Bound::PosInf => true,
                        Bound::At { x, closed } => {
                            x.lo.0 > 0.0 || (closed && x.is_point() && x.lo.0 == 0.0)
                        }
                        Bound::NegInf => false,
                    };
                    lo_ok && hi_ok
                })
        }
        _ => false,
    };
    let const_y = constant.filter(|c| c.is_point() && zero_in_domain);
    let y_row = match (&a.y_intercept, const_y) {
        (_, Some(c)) => Row::Certified {
            value: Some(c),
            cert: Certificate::new(Region::Points),
        },
        (r, None) => r.clone(),
    };
    match &y_row {
        Row::Certified { value, .. } | Row::Partial { value, .. } => {
            if let Some(y) = value {
                let n = cx.y_at(Some(Ex::int(0)), *y);
                if n.fixed() {
                    out.k.y_intercept = n.text();
                    data.y_intercept = Some(n.value());
                } else {
                    // Proven to exist, but not one digit of it is known.
                    out.unknown(flags::Y_INTERCEPT);
                }
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

    if take_unfixed() {
        out.k.minima.clear();
        out.k.maxima.clear();
        data.minima.clear();
        data.maxima.clear();
        out.drop_notes(flags::MINIMA | flags::MAXIMA);
        out.unknown(flags::MINIMA | flags::MAXIMA);
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

    if take_unfixed() {
        out.k.inflection_points.clear();
        data.inflection_points.clear();
        out.drop_notes(flags::INFLECTION_POINTS);
        out.unknown(flags::INFLECTION_POINTS);
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

    if take_unfixed() {
        out.k.vertical_asymptotes.clear();
        data.vertical_asymptotes.clear();
        out.drop_notes(flags::VERTICAL_ASYMPTOTES);
        out.unknown(flags::VERTICAL_ASYMPTOTES);
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
        _ if is_constant => {}
        Some(v) => {
            let mut lines: Vec<(Num, AsymptoteSide)> = Vec::new();
            for h in v.iter().rev() {
                let n = Num {
                    exact: cx
                        .tail_limit(h.side, h.y)
                        .or_else(|| constant_tail(&cx, a, h.side, h.y)),
                    enc: h.y,
                };
                let side = match h.side {
                    Tail::Left => AsymptoteSide::NegativeInfinity,
                    Tail::Right => AsymptoteSide::PositiveInfinity,
                };
                // (Limits that agree to fifteen places are one line.)
                let agree = |m: &Num| match (m.exact, n.exact) {
                    (Some(a), Some(b)) => same(a, b),
                    _ => {
                        (m.enc == n.enc && m.enc.is_point())
                            || fixed_to(m.enc, 15).is_some_and(|t| Some(t) == fixed_to(n.enc, 15))
                    }
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
                // Two lines that read alike get the digits that tell them
                // apart.
                let texts: Vec<String> = lines.iter().map(|(n, _)| n.text()).collect();
                out.k.horizontal_asymptotes = lines
                    .iter()
                    .enumerate()
                    .map(|(i, (n, _))| {
                        let twin = texts.iter().filter(|t| **t == texts[i]).count() > 1;
                        format!("y = {}", if twin { n.text_sig(15) } else { n.text() })
                    })
                    .collect();
                // Two lines that still read alike can't be told apart.
                if alike(&out.k.horizontal_asymptotes) {
                    UNFIXED.with(|u| u.set(true));
                }
                data.horizontal_asymptotes = lines.iter().map(|(n, s)| (n.value(), *s)).collect();
            }
        }
        None => out.unknown(flags::HORIZONTAL_ASYMPTOTES),
    }

    if take_unfixed() {
        out.k.horizontal_asymptotes.clear();
        data.horizontal_asymptotes.clear();
        out.drop_notes(flags::HORIZONTAL_ASYMPTOTES);
        out.unknown(flags::HORIZONTAL_ASYMPTOTES);
    }

    // Oblique asymptotes (+∞ first).
    match a.oblique.value() {
        _ if is_constant || line_degree == Some(1) => {}
        Some(v) => {
            let mut lines: Vec<(Num, Num, AsymptoteSide)> = Vec::new();
            for o in v.iter().rev() {
                // (A proven enclosure that is one double is that value
                // exactly: a line from f's expansion, y = x + 1.)
                let point = |e: Enc| {
                    e.is_point()
                        .then(|| crate::simplify::Q::from_f64(e.lo.0))
                        .flatten()
                        .map(Ex::q)
                };
                let exact = cx
                    .oblique_exact(o.side, o.m, o.b)
                    .or_else(|| Some((point(o.m)?, point(o.b)?)));
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

    if take_unfixed() {
        out.k.oblique_asymptotes.clear();
        data.oblique_asymptotes.clear();
        out.drop_notes(flags::OBLIQUE_ASYMPTOTES);
        out.unknown(flags::OBLIQUE_ASYMPTOTES);
    }

    // Parity. A constant on a domain symmetric about 0 is even (both, if
    // 0).
    let symmetric = match &a.domain {
        Row::Certified { value, .. } => cx.symmetric(value),
        _ => false,
    };
    out.k.parity = match a.parity.value() {
        Some(certify::Parity::Even | certify::Parity::Odd) if zero_function => Parity::Both,
        _ if zero_function && symmetric => Parity::Both,
        None if constant.is_some() && symmetric => Parity::Even,
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

    if take_unfixed() {
        out.k.periodicity_direction = Periodicity::Unknown;
        out.k.periodicity_expression.clear();
        data.period = None;
        out.unknown(flags::PERIODICITY);
    }

    // Monotonicity: over one period, each piece repeats every P (exact,
    // or only enclosed: π², 360/π).
    let per: Option<Num> = match &a.monotonicity {
        Row::Certified { cert, .. } => match cert.covers {
            Region::Period { a: s0, b: s1 } => {
                let w = s1.0 - s0.0;
                cx.period_enc
                    .filter(|p| (p.mid() - w).abs() <= 1e-9 * w)
                    .map(|enc| Num {
                        exact: cx.period.filter(|p| (p.to_f64() - w).abs() <= 1e-9 * w),
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
                                return (l, h);
                            }
                            // (An end moved onto 0 with an enclosed period,
                            // P − P, has no digit fixed: it stays put.)
                            let (sl, sh) = (shift(l, p, n), shift(h, p, n));
                            let on_zero = |e: &Num| {
                                e.exact.is_none()
                                    && !e.enc.is_point()
                                    && e.enc.lo.0 <= 0.0
                                    && 0.0 <= e.enc.hi.0
                            };
                            if on_zero(&sl) || on_zero(&sh) {
                                (l, h)
                            } else {
                                (sl, sh)
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
                        // Written once the row's bounds are told apart.
                        (lo, hi, String::new())
                    }
                };
                pieces.push((lo.value, (text, dir, lo, hi)));
            }
            pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
            // Bounds that read alike get the digits that tell them apart;
            // a piece whose bounds still read alike says nothing.
            {
                let open: Vec<usize> = (0..pieces.len())
                    .filter(|&i| pieces[i].1.0.is_empty())
                    .collect();
                let refs: Vec<&End> = open
                    .iter()
                    .flat_map(|&i| [&pieces[i].1.2, &pieces[i].1.3])
                    .collect();
                let mut cls = classes(&refs);
                for k in 0..open.len() {
                    if cls[2 * k + 1] == cls[2 * k] {
                        cls[2 * k + 1] = refs.len() + 2 * k + 1;
                    }
                }
                let mut ends: Vec<(usize, &mut End)> = Vec::new();
                for (_, (text, _, lo, hi)) in pieces.iter_mut() {
                    if text.is_empty() {
                        ends.push((0, lo));
                        ends.push((0, hi));
                    }
                }
                for (k, e) in ends.iter_mut().enumerate() {
                    e.0 = cls[k];
                }
                // Each piece's two bounds.
                let sets: Vec<Vec<usize>> =
                    (0..open.len()).map(|k| vec![2 * k, 2 * k + 1]).collect();
                tell_apart(&mut ends, &sets);
                for (_, (text, _, lo, hi)) in pieces.iter_mut() {
                    if text.is_empty() {
                        if lo.inf.is_none() && hi.inf.is_none() && lo.text == hi.text {
                            UNFIXED.with(|u| u.set(true));
                        }
                        *text = interval_text(lo, hi);
                    }
                }
            }
            out.settle(
                flags::MONOTONE_INTERVALS,
                reach(&a.monotonicity),
                pieces.is_empty(),
                false,
            );
            if out.k.too_complex_features & flags::MONOTONE_INTERVALS == 0 {
                if data.period.is_none() {
                    data.repeat = per.map(|p| p.value());
                }
                for (_, (text, dir, lo, hi)) in pieces {
                    data.monotonicity.push((data_interval(&lo, &hi), dir));
                    out.k.monotonicity.push((text, dir));
                }
            }
        }
        None => out.unknown(flags::MONOTONE_INTERVALS),
    }

    if take_unfixed() {
        out.k.monotonicity.clear();
        data.monotonicity.clear();
        out.drop_notes(flags::MONOTONE_INTERVALS);
        out.unknown(flags::MONOTONE_INTERVALS);
    }

    out.k.data = data;
    // Cut short by the flag: rows may be missing what it stopped.
    (!cx.fun.cancelled()).then_some(out.k)
}

/// Whether range piece `i` is one value: both ends closed, the same
/// enclosure, and either exactly one double or taken from one source (f
/// at one place, one limit). Equal enclosures alone prove nothing: two
/// values an ulp apart have the same.
fn one_value(a: &Analysis, i: usize) -> bool {
    let Row::Certified { value, .. } = &a.range else {
        return false;
    };
    let Some(Piece {
        lo: Bound::At { x: l, closed: true },
        hi: Bound::At { x: h, closed: true },
    }) = value.get(i)
    else {
        return false;
    };
    if l != h {
        return false;
    }
    l.is_point()
        || a.range_ends
            .get(i)
            .is_some_and(|[s0, s1]| s0.len() == 1 && s0 == s1 && !matches!(s0[0], EndSrc::Other))
}

/// `a` with the rows a constant c (on its domain) decides filled in where
/// the certifier left them open.
fn constant_rows(a: &Analysis, c: Enc) -> Analysis {
    let mut a = a.clone();
    let proof = || {
        let mut cert = Certificate::new(Region::Line);
        cert.push(Claim::Simplifier {
            fact: "f is constant on its domain".into(),
        });
        cert
    };
    fn open<T>(r: &Row<T>) -> bool {
        !r.is_certified()
    }
    if open(&a.extrema) {
        a.extrema = Row::Certified {
            value: Vec::new(),
            cert: proof(),
        };
    }
    if open(&a.inflections) {
        a.inflections = Row::Certified {
            value: Vec::new(),
            cert: proof(),
        };
    }
    if open(&a.vertical) {
        a.vertical = Row::Certified {
            value: Vec::new(),
            cert: proof(),
        };
    }
    // Not 0: no zeros (0 itself is handled as the zero function).
    if open(&a.x_intercepts) && (c.lo.0 > 0.0 || c.hi.0 < 0.0) {
        a.x_intercepts = Row::Certified {
            value: Vec::new(),
            cert: proof(),
        };
    }
    if open(&a.monotonicity)
        && let Row::Certified { value, .. } = &a.domain
        && value.excluded.is_empty()
    {
        a.monotonicity = Row::Certified {
            value: value
                .pieces
                .iter()
                .filter(|p| p.lo != p.hi)
                .map(|p| certify::Monotone {
                    on: *p,
                    dir: Dir::Constant,
                })
                .collect(),
            cert: proof(),
        };
    }
    a
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
    // Two points that read alike get the digits that tell them apart.
    let xs: Vec<String> = items.iter().map(|(_, (x, _, _))| x.text()).collect();
    let sig = |i: usize| {
        if xs.iter().filter(|t| **t == xs[i]).count() > 1 {
            15
        } else {
            6
        }
    };
    let mut texts: Vec<String> = Vec::new();
    let mut data = Vec::new();
    for (i, (_, (x, p, y))) in items.into_iter().enumerate() {
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
                texts.push(format!("({}, {})", x.text_sig(sig(i)), y.text()));
                data.push((DataFamily::single(x.value()), y.value()));
            }
        }
    }
    // Two points that still read alike can't be told apart.
    if alike(&texts) {
        UNFIXED.with(|u| u.set(true));
    }
    (texts, data, was_cut)
}

/// The limit at ±∞ of an f proven constant on a piece reaching it: f at a
/// whole number inside the piece, exactly (atan(x) + atan(1/x) → π/2, as
/// its range says).
fn constant_tail(cx: &Ctx<'_>, a: &Analysis, side: Tail, y: Enc) -> Option<Ex> {
    let Row::Certified { value, .. } = &a.monotonicity else {
        return None;
    };
    let m = value.iter().find(|m| {
        m.dir == Dir::Constant
            && match side {
                Tail::Right => m.on.hi == Bound::PosInf,
                Tail::Left => m.on.lo == Bound::NegInf,
            }
    })?;
    let c = match (side, m.on.lo, m.on.hi) {
        (Tail::Right, Bound::At { x, .. }, _) => x.hi.0.floor() + 1.0,
        (Tail::Left, _, Bound::At { x, .. }) => x.lo.0.ceil() - 1.0,
        (Tail::Right, _, _) => 1.0,
        (Tail::Left, _, _) => -1.0,
    };
    if c.abs() >= 1e15 || c.is_nan() {
        return None;
    }
    let v = cx.eval(&cx.f, Ex::int(c as i128))?;
    v.agrees(y.lo.0, y.hi.0).then_some(v)
}

/// `e` with each slider at its value, where that is a whole number (the
/// rational form takes exact literals only; it is the value exact
/// evaluation uses too). The value is a number of its own, exactly its
/// double (`Lit::Exact`): never a typed literal that shares that double
/// (a = 1 beside a typed 1.0000000000000001, review 13).
fn with_values(e: &Expr, vars: &dyn VariableValues) -> Expr {
    let go = |a: &Expr| Box::new(with_values(a, vars));
    match e {
        Expr::Var(name) => {
            let v = vars
                .value(name)
                .unwrap_or(crate::compile::DEFAULT_VARIABLE_VALUE);
            if v == v.trunc() && v.abs() <= 9007199254740992.0 {
                Expr::exact(v)
            } else {
                e.clone()
            }
        }
        Expr::Neg(a) => Expr::Neg(go(a)),
        Expr::Degrees(a) => Expr::Degrees(go(a)),
        Expr::Bin(op, a, b) => Expr::Bin(*op, go(a), go(b)),
        Expr::Call(f, args) => Expr::Call(*f, args.iter().map(|a| with_values(a, vars)).collect()),
        _ => e.clone(),
    }
}

/// `y = m·x + b` in the panel's style: `y = x + 1`, `y = −2x`,
/// `y = x/2 − 3`.
fn line_text(m: &Num, b: &Num) -> String {
    // (A slope enclosed by one double is that double: y = x − 3.14159,
    // not y = 1x − 3.14159.)
    let exact = m.exact.or_else(|| {
        m.enc
            .is_point()
            .then(|| Q::from_f64(m.enc.lo.0).map(Ex::q))
            .flatten()
    });
    let mx = match exact.and_then(Ex::rational) {
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
        let r2 = std::f64::consts::SQRT_2;
        // Not exact, whatever its digits: marked.
        assert_eq!(approx(Enc::new(r2.next_down(), r2.next_up())), "≈1.41421");
        // Never more than six digits; from 10⁶ on m×10ⁿ, the form the
        // rounded value's, so ends straddling 10⁶ read alike.
        assert_eq!(approx(Enc::new(2300620.0, 2300620.006)), "≈2.30062×10⁶");
        assert_eq!(approx(Enc::new(545843449.3, 545843449.5)), "≈5.45843×10⁸");
        assert_eq!(approx(Enc::new(999999.9999982, 1000000.0000001)), "≈1×10⁶");
        assert_eq!(approx(Enc::new(166253.76, 166253.77)), "≈166254");
        // An exact integer keeps its exact text.
        assert_eq!(approx(Enc::point(1000000.0)), "1000000");
        assert_eq!(
            approx(Enc::new(0.9999999999999999, 1.0000000000000002)),
            "≈1"
        );
        // Maybe 0: no digit known.
        assert_eq!(approx(Enc::new(-1e-17, 1e-17)), "[−1×10⁻¹⁷, 1×10⁻¹⁷]");
        assert_eq!(approx(Enc::point(0.0)), "0");
        assert_eq!(
            approx(Enc::new(0.49999999999999994, 0.5000000000000001)),
            "≈0.5"
        );
        // Fewer than six digits fixed: as many as are, marked.
        assert_eq!(approx(Enc::new(1.41419, 1.41431)), "≈1.414");
        assert_eq!(approx(Enc::new(-0.0012341, -0.0012339)), "≈−0.001234");
        // Fewer than three: not a value to show.
        assert_eq!(approx(Enc::new(2.41, 2.42)), "[2.4, 2.5]");
        // None fixed: the enclosure, rounded outward.
        assert_eq!(
            approx(Enc::new(-std::f64::consts::FRAC_PI_2, 0.0)),
            "[−1.6, 0]"
        );
        assert_eq!(approx(Enc::new(0.851, 1.05)), "[0.85, 1.1]");
    }
}
