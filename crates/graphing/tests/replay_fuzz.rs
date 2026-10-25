//! A claim-set fuzzer for the replay's row inference
//! (`tests/replay/rows.rs`), against an exact truth (`--features
//! mpfr-oracle`).
//!
//! Three review rounds running, a reviewer built a set of true claims by
//! hand that the replay turned into a false row, every premise strong: a
//! window's end behind a gap from a turn (review 22, R22-M-01), a
//! derivative zero at the next double taken for a flat stretch reaching
//! back to a turn (R23-M-01), a continuity claim at a turn recording no
//! zero at all (R23-M-02). The mutation tester (`certify_mutants.rs`)
//! changes genuine certificates, so it only meets the shapes the certifier
//! writes. This makes up claim sets and rows of its own, true and false,
//! and checks every row the replay accepts against the truth.
//!
//! **Functions** whose features are known exactly: polynomials with
//! rational coefficients, built from a factored f, f′ or f″ (integrated
//! once or twice) whose roots are aimed at doubles, at adjacent doubles
//! (1 and 1 + 2⁻⁵²), at points no double holds (1 + 2⁻⁵³, 1/3), as
//! double, triple and fourfold roots; constants; each sometimes with
//! `+0*sqrt(x−c)`, `+0*sqrt(c−x)` or `+0/(x−c)` for a closed domain end
//! or a hole. The truth needs no sampling: exact rational arithmetic
//! (GMP) on the expanded polynomial, Sturm sequences for the roots in any
//! box, square-free (Yun) parts for which roots change sign, and signs at
//! algebraic points by isolating intervals and gcds.
//!
//! **Claims** from the replay's own vocabulary, true by that truth, on
//! boxes around and between the features: sign stretches (Beyond,
//! NoCross, Factors, Kink on a box), zeros (ExactAt, OneCross, Touch, a
//! point Value or Kink, KinkSides placed at a turn), gaps (Gap, GapClear),
//! Defined, Continuous, Undefined, tails (TailBeyond), and the values of f
//! at the listed items; point boxes, one-ulp gaps, boxes ending on a
//! feature or one double off it, overlapping boxes, windows whose ends sit
//! on a feature or beside one. Each premise goes through the replay's own
//! claim check, and only those it proves are kept (strong, or weak: shown
//! on a tree the fuzzer supplied, its factored f′ or f″, which the replay
//! verifies exactly). Every kept premise is also held to the truth: one
//! the replay proves that is false is a finding of its own.
//!
//! **Rows**: x-intercepts, extrema, inflections and monotone pieces;
//! Certified over the line, Partial over a window (complete there), or
//! Partial over points; their items the true ones, with one dropped,
//! moved a double, merged with the next, made up at a feature's neighbour,
//! of the wrong kind or direction, or pieces cut short or joined.
//!
//! **Verdict**: a row the replay accepts is held to the truth; an accepted
//! false row is a failure, minimised (claims and items dropped while the
//! replay still accepts it) and printed with its certificate. The run also
//! counts the rows the replay refuses that are true (not a bug: the replay
//! may not see what the claims don't show, but its false refusals show up
//! here), and holds every genuine certificate of the functions it makes to
//! the same truth: the oracle's own check, and the certifier's. A genuine
//! Certified or Partial row the truth refuses fails the run whether the
//! replay accepts it or not: the panel shows it either way. (Failing only
//! where the replay accepted it too, the run passed over the certifier's
//! turn beside a hole: `1/3*(x+2)^3-2^(-53)*(x+2)^2+1/3+
//! 0/(x-(-9007199254740991*2^(-52)))`, a maximum at −2 with its hole a
//! double away, certified "no extrema"; the replay refuses it, f′'s sign
//! after −2 not decided.) A true genuine row the replay refuses is
//! counted and printed, not failed.
//!
//! ```text
//! cargo test -p graphing --features mpfr-oracle --test replay_fuzz          # fixed seed, seconds
//! cargo test --release -p graphing --features mpfr-oracle --test replay_fuzz -- \
//!     --ignored long_run --nocapture
//! ```
//!
//! The long run takes `REPLAY_FUZZ_SEED`, `REPLAY_FUZZ_FUNCTIONS` (cases
//! come `REPLAY_FUZZ_CASES` to a function, 24 unless set) and
//! `REPLAY_FUZZ_THREADS`; `REPLAY_FUZZ_SHOW=n` prints the first n
//! failures in full.
#![cfg(feature = "mpfr-oracle")]

mod replay;

use graphing::certify::{DEFAULT_BUDGET, certify_text};
use graphing::compile::CompileOptions;
use replay::{Claim, Class, KinkSign, Side, Subject};
use rug::{Integer, Rational};
use serde_json::{Value, json};
use std::cmp::Ordering::{self, Equal, Greater, Less};
use std::collections::BTreeMap;
use std::sync::Mutex;

// ------------------------------------------------------------ randomness

struct Rng(u64);

impl Rng {
    fn new(seed: u64, a: u64, b: u64) -> Rng {
        let mut r = Rng(seed ^ a.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ b.rotate_left(29));
        r.next();
        r
    }

    fn next(&mut self) -> u64 {
        // SplitMix64.
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn chance(&mut self, p: f64) -> bool {
        ((self.next() >> 11) as f64) < p * 9_007_199_254_740_992.0
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
}

// ------------------------------------------------------------ exact polynomials

fn qi(n: i64) -> Rational {
    Rational::from(n)
}

fn qd(d: f64) -> Rational {
    Rational::from_f64(d).expect("a finite double")
}

fn sign(q: &Rational) -> Ordering {
    q.cmp0()
}

/// A polynomial with rational coefficients, lowest degree first, no
/// trailing zeros (the zero polynomial has none at all).
#[derive(Clone, Debug, Default, PartialEq)]
struct Poly(Vec<Rational>);

impl Poly {
    fn new(mut c: Vec<Rational>) -> Poly {
        while c.last().is_some_and(|x| x.cmp0() == Equal) {
            c.pop();
        }
        Poly(c)
    }

    fn constant(c: Rational) -> Poly {
        Poly::new(vec![c])
    }

    /// x − r.
    fn x_minus(r: &Rational) -> Poly {
        Poly::new(vec![Rational::from(-r), qi(1)])
    }

    fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    fn degree(&self) -> usize {
        self.0.len().saturating_sub(1)
    }

    fn lead(&self) -> Rational {
        self.0.last().cloned().unwrap_or_default()
    }

    fn eval(&self, x: &Rational) -> Rational {
        let mut acc = Rational::new();
        for c in self.0.iter().rev() {
            acc *= x;
            acc += c;
        }
        acc
    }

    fn sign(&self, x: &Rational) -> Ordering {
        sign(&self.eval(x))
    }

    fn deriv(&self) -> Poly {
        Poly::new(
            self.0
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, c)| Rational::from(c * i as u32))
                .collect(),
        )
    }

    fn integral(&self) -> Poly {
        let mut c = vec![Rational::new()];
        for (i, a) in self.0.iter().enumerate() {
            c.push(Rational::from(a / (i as u32 + 1)));
        }
        Poly::new(c)
    }

    fn add(&self, o: &Poly) -> Poly {
        let n = self.0.len().max(o.0.len());
        Poly::new(
            (0..n)
                .map(|i| {
                    let mut s = self.0.get(i).cloned().unwrap_or_default();
                    if let Some(b) = o.0.get(i) {
                        s += b;
                    }
                    s
                })
                .collect(),
        )
    }

    fn scale(&self, k: &Rational) -> Poly {
        Poly::new(self.0.iter().map(|c| Rational::from(c * k)).collect())
    }

    fn sub(&self, o: &Poly) -> Poly {
        self.add(&o.scale(&qi(-1)))
    }

    fn mul(&self, o: &Poly) -> Poly {
        if self.is_zero() || o.is_zero() {
            return Poly::default();
        }
        let mut c = vec![Rational::new(); self.0.len() + o.0.len() - 1];
        for (i, a) in self.0.iter().enumerate() {
            for (j, b) in o.0.iter().enumerate() {
                c[i + j] += Rational::from(a * b);
            }
        }
        Poly::new(c)
    }

    fn pow(&self, n: u32) -> Poly {
        (0..n).fold(Poly::constant(qi(1)), |acc, _| acc.mul(self))
    }

    fn divrem(&self, d: &Poly) -> (Poly, Poly) {
        assert!(!d.is_zero(), "division by the zero polynomial");
        if self.0.len() < d.0.len() {
            return (Poly::default(), self.clone());
        }
        let dd = d.degree();
        let dl = d.lead();
        let mut r = self.0.clone();
        let mut q = vec![Rational::new(); self.0.len() - d.0.len() + 1];
        for i in (0..q.len()).rev() {
            let c = Rational::from(&r[i + dd] / &dl);
            if c.cmp0() != Equal {
                for (j, dc) in d.0.iter().enumerate() {
                    r[i + j] -= Rational::from(&c * dc);
                }
            }
            q[i] = c;
        }
        r.truncate(dd);
        (Poly::new(q), Poly::new(r))
    }

    fn div(&self, d: &Poly) -> Poly {
        self.divrem(d).0
    }

    fn monic(&self) -> Poly {
        if self.is_zero() {
            return Poly::default();
        }
        let l = self.lead();
        self.scale(&Rational::from(l.recip_ref()))
    }

    /// This polynomial in u, as one in x for u = x − a.
    fn at_shift(&self, a: &Rational) -> Poly {
        let u = Poly::x_minus(a);
        let mut acc = Poly::default();
        for c in self.0.iter().rev() {
            acc = acc.mul(&u).add(&Poly::constant(c.clone()));
        }
        acc
    }
}

fn gcd(a: &Poly, b: &Poly) -> Poly {
    let (mut a, mut b) = (a.monic(), b.monic());
    while !b.is_zero() {
        let (_, r) = a.divrem(&b);
        a = b;
        b = r.monic();
    }
    a
}

/// p with each root once.
fn squarefree(p: &Poly) -> Poly {
    if p.degree() == 0 {
        return Poly::constant(qi(1));
    }
    p.div(&gcd(p, &p.deriv())).monic()
}

/// The product of p's square-free factors of odd multiplicity (Yun): its
/// roots are those where p changes sign.
fn odd_part(p: &Poly) -> Poly {
    if p.degree() == 0 {
        return Poly::constant(qi(1));
    }
    let dp = p.deriv();
    let a0 = gcd(p, &dp);
    let mut b = p.div(&a0);
    let c = dp.div(&a0);
    let mut d = c.sub(&b.deriv());
    let mut out = Poly::constant(qi(1));
    let mut i = 1;
    while b.degree() > 0 {
        let a = gcd(&b, &d);
        if i % 2 == 1 {
            out = out.mul(&a);
        }
        b = b.div(&a);
        let c = d.div(&a);
        d = c.sub(&b.deriv());
        i += 1;
    }
    out.monic()
}

/// A Sturm sequence of a square-free polynomial.
struct Sturm(Vec<Poly>);

impl Sturm {
    fn new(p: &Poly) -> Sturm {
        let mut s = vec![p.clone()];
        if p.degree() >= 1 {
            s.push(p.deriv());
            loop {
                let n = s.len();
                let (_, r) = s[n - 2].divrem(&s[n - 1]);
                if r.is_zero() {
                    break;
                }
                // −r, scaled by a positive number.
                let l = r.lead().abs();
                s.push(r.scale(&(-l.recip())));
            }
        }
        Sturm(s)
    }

    fn changes(signs: impl Iterator<Item = Ordering>) -> usize {
        let mut last = Equal;
        let mut n = 0;
        for s in signs.filter(|s| *s != Equal) {
            if last != Equal && s != last {
                n += 1;
            }
            last = s;
        }
        n
    }

    fn at(&self, x: &Rational) -> usize {
        Sturm::changes(self.0.iter().map(|p| p.sign(x)))
    }

    fn at_inf(&self, positive: bool) -> usize {
        Sturm::changes(self.0.iter().map(|p| {
            let s = sign(&p.lead());
            if positive || p.degree() % 2 == 0 {
                s
            } else {
                s.reverse()
            }
        }))
    }

    /// Distinct roots in (a, b] (None: −∞ for a, +∞ for b).
    fn count(&self, a: Option<&Rational>, b: Option<&Rational>) -> usize {
        let va = a.map_or_else(|| self.at_inf(false), |a| self.at(a));
        let vb = b.map_or_else(|| self.at_inf(true), |b| self.at(b));
        va.saturating_sub(vb)
    }

    /// Distinct roots in [a, b].
    fn closed(&self, a: &Rational, b: &Rational) -> usize {
        if a > b {
            return 0;
        }
        self.count(Some(a), Some(b)) + usize::from(self.0[0].sign(a) == Equal)
    }

    /// Distinct roots in (a, b).
    fn open(&self, a: &Rational, b: &Rational) -> usize {
        if a >= b {
            return 0;
        }
        self.count(Some(a), Some(b)) - usize::from(self.0[0].sign(b) == Equal)
    }
}

/// A real root of a square-free polynomial: exactly `lo` when `lo == hi`,
/// else the one root strictly between, p nonzero at both (of opposite
/// signs).
#[derive(Clone, Debug)]
struct Root {
    p: Poly,
    lo: Rational,
    hi: Rational,
}

impl Root {
    fn exact(&self) -> Option<&Rational> {
        (self.lo == self.hi).then_some(&self.lo)
    }

    fn bisect(&mut self) {
        if self.lo == self.hi {
            return;
        }
        let m = Rational::from(&self.lo + &self.hi) / 2u32;
        match self.p.sign(&m) {
            Equal => {
                self.lo = m.clone();
                self.hi = m;
            }
            s if s == self.p.sign(&self.lo) => self.lo = m,
            _ => self.hi = m,
        }
    }

    /// The root against q.
    fn cmp(&self, q: &Rational) -> Ordering {
        if self.lo == self.hi {
            return self.lo.cmp(q);
        }
        if *q <= self.lo {
            return Greater;
        }
        if *q >= self.hi {
            return Less;
        }
        match self.p.sign(q) {
            Equal => Equal,
            s if s == self.p.sign(&self.lo) => Greater,
            _ => Less,
        }
    }

    /// g's sign at the root.
    fn sign_of(&mut self, g: &Poly) -> Ordering {
        if let Some(x) = self.exact() {
            return g.sign(x);
        }
        if g.is_zero() {
            return Equal;
        }
        let h = gcd(&self.p, g);
        if h.degree() >= 1 && Sturm::new(&h).open(&self.lo, &self.hi) > 0 {
            return Equal;
        }
        let gs = Sturm::new(&squarefree(g));
        loop {
            if let Some(x) = self.exact() {
                return g.sign(x);
            }
            if gs.closed(&self.lo, &self.hi) == 0 {
                return g.sign(&self.lo);
            }
            self.bisect();
        }
    }

    /// The largest double at or below the root, and the least at or above
    /// it (one double if the root is one).
    fn doubles(&mut self) -> (f64, f64) {
        for _ in 0..4000 {
            if self.exact().is_some() {
                break;
            }
            let w = Rational::from(&self.hi - &self.lo);
            let m = Rational::from(&self.hi + &self.lo).abs();
            if &w * Rational::from(Integer::from(1) << 60) <= m
                || w <= Rational::from((Integer::from(1), Integer::from(1) << 1100))
            {
                break;
            }
            self.bisect();
        }
        let mid = Rational::from(&self.lo + &self.hi) / 2u32;
        let mut d = mid.to_f64();
        while self.cmp(&qd(d)) == Less {
            d = d.next_down();
        }
        while self.cmp(&qd(d.next_up())) != Less {
            d = d.next_up();
        }
        if self.cmp(&qd(d)) == Equal {
            (d, d)
        } else {
            (d, d.next_up())
        }
    }
}

/// The distinct real roots of p (not the zero polynomial), in order.
fn real_roots(p: &Poly) -> Vec<Root> {
    if p.degree() == 0 {
        return Vec::new();
    }
    let s = squarefree(p);
    let st = Sturm::new(&s);
    // Cauchy's bound.
    let lead = s.lead();
    let mut m = qi(1);
    for c in &s.0[..s.0.len() - 1] {
        let r = Rational::from(c / &lead).abs();
        if r > m {
            m = r;
        }
    }
    m += 1;
    let mut out = Vec::new();
    let mut stack = vec![(Rational::from(-&m), m)];
    while let Some((a, b)) = stack.pop() {
        match st.count(Some(&a), Some(&b)) {
            0 => {}
            1 => {
                if s.sign(&b) == Equal {
                    out.push(Root {
                        p: s.clone(),
                        lo: b.clone(),
                        hi: b,
                    });
                    continue;
                }
                // The root is in (a, b); a may be another root.
                let (mut a, mut b) = (a, b);
                while s.sign(&a) == Equal {
                    let mid = Rational::from(&a + &b) / 2u32;
                    if s.sign(&mid) == Equal {
                        // The one root in (a, b).
                        a = mid.clone();
                        b = mid;
                        break;
                    }
                    if st.count(Some(&mid), Some(&b)) == 1 {
                        a = mid;
                    } else {
                        b = mid;
                    }
                }
                out.push(Root {
                    p: s.clone(),
                    lo: a,
                    hi: b,
                });
            }
            _ => {
                let mid = Rational::from(&a + &b) / 2u32;
                stack.push((a, mid.clone()));
                stack.push((mid, b));
            }
        }
    }
    out.sort_by(|x, y| x.lo.cmp(&y.lo));
    out
}

/// p's sign just left and just right of the root r (of p's square-free
/// part).
fn sides(p: &Poly, r: &Root) -> (Ordering, Ordering) {
    match r.exact() {
        None => (p.sign(&r.lo), p.sign(&r.hi)),
        Some(x) => {
            let st = Sturm::new(&r.p);
            let mut e = qi(1);
            loop {
                let a = Rational::from(x - &e);
                let b = Rational::from(x + &e);
                if st.closed(&a, &b) == 1 {
                    return (p.sign(&a), p.sign(&b));
                }
                e /= 2u32;
            }
        }
    }
}

/// p's sign just right (or left) of the rational x, x itself left out.
fn sign_beside(p: &Poly, x: &Rational, right: bool) -> Ordering {
    if p.is_zero() {
        return Equal;
    }
    let st = Sturm::new(&squarefree(p));
    let mut e = qi(1);
    loop {
        let y = if right {
            Rational::from(x + &e)
        } else {
            Rational::from(x - &e)
        };
        let (a, b) = if right { (x, &y) } else { (&y, x) };
        if st.open(a, b) == 0 && p.sign(&y) != Equal {
            return p.sign(&y);
        }
        e /= 2u32;
    }
}

/// The values of p over [a, b] lie in [lo, hi] (exactly: its ends and its
/// turns between).
fn values_within(p: &Poly, a: &Rational, b: &Rational, lo: &Rational, hi: &Rational) -> bool {
    let inside = |v: Rational| v >= *lo && v <= *hi;
    if !inside(p.eval(a)) || !inside(p.eval(b)) {
        return false;
    }
    let d = p.deriv();
    if d.is_zero() {
        return true;
    }
    for mut r in real_roots(&d) {
        if r.cmp(a) == Greater && r.cmp(b) == Less {
            let below = p.sub(&Poly::constant(lo.clone()));
            let above = p.sub(&Poly::constant(hi.clone()));
            if r.sign_of(&below) == Less || r.sign_of(&above) == Greater {
                return false;
            }
        }
    }
    true
}

// ------------------------------------------------------------ doubles

fn floor_double(q: &Rational) -> f64 {
    let mut d = q.to_f64();
    while qd(d) > *q {
        d = d.next_down();
    }
    while qd(d.next_up()) <= *q {
        d = d.next_up();
    }
    d
}

fn ceil_double(q: &Rational) -> f64 {
    let d = floor_double(q);
    if qd(d) == *q { d } else { d.next_up() }
}

fn doubles_out(mut x: f64, n: i32) -> f64 {
    for _ in 0..n.unsigned_abs() {
        x = if n > 0 { x.next_up() } else { x.next_down() };
    }
    x
}

/// At most 64 doubles from a to b.
fn narrow(a: f64, b: f64) -> bool {
    a <= b && a.is_finite() && b.is_finite() && doubles_out(a, 64) >= b
}

// ------------------------------------------------------------ functions

/// q in the graphing language, exactly: a whole number, n/d, times a
/// power of two (n and d odd, below 2⁵³); None if it won't fit.
fn rat_text(q: &Rational) -> Option<String> {
    let (neg, no, dno, e) = rat_parts(q)?;
    let mut s = String::new();
    if neg {
        s.push('-');
    }
    let pow = |e: i64| {
        if e < 0 {
            format!("2^({e})")
        } else {
            format!("2^{e}")
        }
    };
    match (no == 1, dno == 1, e == 0) {
        (_, true, true) => s.push_str(&no.to_string()),
        (_, false, true) => s.push_str(&format!("{no}/{dno}")),
        (true, true, false) => s.push_str(&pow(e)),
        (true, false, false) => s.push_str(&format!("{}/{dno}", pow(e))),
        (false, true, false) => s.push_str(&format!("{no}*{}", pow(e))),
        (false, false, false) => s.push_str(&format!("{no}/{dno}*{}", pow(e))),
    }
    Some(s)
}

/// q's sign, odd numerator and denominator and power of two (small powers
/// folded into the whole numbers).
fn rat_parts(q: &Rational) -> Option<(bool, Integer, Integer, i64)> {
    if q.cmp0() == Equal {
        return Some((false, Integer::new(), Integer::from(1), 0));
    }
    let neg = q.cmp0() == Less;
    let n = Integer::from(q.numer().abs_ref());
    let d = q.denom().clone();
    let tn = n.find_one(0)?;
    let td = d.find_one(0)?;
    let (mut no, mut dno) = (Integer::from(&n >> tn), Integer::from(&d >> td));
    let mut e = i64::from(tn) - i64::from(td);
    if e > 0 && e <= 16 && no.significant_bits() + e as u32 <= 53 {
        no <<= e as u32;
        e = 0;
    }
    if (-16..0).contains(&e) && dno.significant_bits() + (-e) as u32 <= 53 {
        dno <<= (-e) as u32;
        e = 0;
    }
    if no.significant_bits() > 53 || dno.significant_bits() > 53 {
        return None;
    }
    Some((neg, no, dno, e))
}

/// q as the certificate's formulas write it (prefix).
fn rat_formula(q: &Rational) -> Option<String> {
    let (neg, no, dno, e) = rat_parts(q)?;
    let mut s = no.to_string();
    if dno != 1 {
        s = format!("Div({s},{dno})");
    }
    if e != 0 {
        let p = if e < 0 {
            format!("Pow(2,Neg({}))", -e)
        } else {
            format!("Pow(2,{e})")
        };
        s = if no == 1 && dno == 1 {
            p
        } else {
            format!("Mul({s},{p})")
        };
    }
    Some(if neg { format!("Neg({s})") } else { s })
}

/// Where f is defined.
#[derive(Clone, Debug)]
enum Dom {
    Line,
    /// [c, ∞): `+0*sqrt(x−c)`.
    From(Rational),
    /// (−∞, c]: `+0*sqrt(c−x)`.
    Upto(Rational),
    /// ℝ ∖ {c}: `+0/(x−c)`.
    Hole(Rational),
}

impl Dom {
    fn holds_q(&self, x: &Rational) -> bool {
        match self {
            Dom::Line => true,
            Dom::From(c) => x >= c,
            Dom::Upto(c) => x <= c,
            Dom::Hole(c) => x != c,
        }
    }

    /// The closed box [a, b] lies in the domain.
    fn holds_box(&self, a: f64, b: f64) -> bool {
        match self {
            Dom::Line => true,
            Dom::From(c) => a.is_finite() && qd(a) >= *c,
            Dom::Upto(c) => b.is_finite() && qd(b) <= *c,
            Dom::Hole(c) => !((!a.is_finite() || qd(a) <= *c) && (!b.is_finite() || *c <= qd(b))),
        }
    }

    /// The closed box [a, b] misses the domain.
    fn misses_box(&self, a: f64, b: f64) -> bool {
        match self {
            Dom::Line => false,
            Dom::From(c) => b.is_finite() && qd(b) < *c,
            Dom::Upto(c) => a.is_finite() && qd(a) > *c,
            Dom::Hole(c) => a == b && a.is_finite() && qd(a) == *c,
        }
    }

    fn holds(&self, p: &Place) -> bool {
        match self {
            Dom::Line => true,
            Dom::From(c) => p.cmp(c) != Less,
            Dom::Upto(c) => p.cmp(c) != Greater,
            Dom::Hole(c) => p.cmp(c) != Equal,
        }
    }

    /// Strictly inside the domain (an end left out).
    fn inside(&self, p: &Place) -> bool {
        match self {
            Dom::From(c) => p.cmp(c) == Greater,
            Dom::Upto(c) => p.cmp(c) == Less,
            _ => self.holds(p),
        }
    }

    fn point(&self) -> Option<&Rational> {
        match self {
            Dom::Line => None,
            Dom::From(c) | Dom::Upto(c) | Dom::Hole(c) => Some(c),
        }
    }
}

/// A function the fuzzer made: its text, the exact polynomial its values
/// are, and its domain.
#[derive(Clone, Debug)]
struct Fun {
    text: String,
    /// The trees given the certificate as `evaluated` ("f′ = …").
    evaluated: Vec<String>,
    f: Poly,
    dom: Dom,
    /// The points it was built about (roots planted, the centre).
    planted: Vec<Rational>,
    /// f′ and f″ as formulas, multiplied out (given with a domain end or a
    /// hole, where the replay's own algebra doesn't read f as a rational
    /// function and needs a tree to factor).
    expanded: Vec<String>,
}

/// p(x − c) as a formula (p in u = x − c).
fn poly_formula(p: &Poly, c: &Rational) -> Option<String> {
    let (_, u) = x_minus_text(c, &qi(0))?;
    let mut out: Option<String> = None;
    for (j, a) in p.0.iter().enumerate() {
        if a.cmp0() == Equal {
            continue;
        }
        let coef = rat_formula(a)?;
        let term = match j {
            0 => coef,
            1 => format!("Mul({coef},{u})"),
            _ => format!("Mul({coef},Pow({u},{j}))"),
        };
        out = Some(match out {
            None => term,
            Some(s) => format!("Add({s},{term})"),
        });
    }
    Some(out.unwrap_or_else(|| "0".into()))
}

/// `(x−r)`, r = c + o, as typed and as a formula.
fn x_minus_text(c: &Rational, o: &Rational) -> Option<(String, String)> {
    let r = Rational::from(c + o);
    if r.cmp0() == Equal {
        return Some(("x".into(), "x".into()));
    }
    let mut t = String::from("(x");
    for part in [c, o] {
        if part.cmp0() == Equal {
            continue;
        }
        let s = rat_text(&Rational::from(part.abs_ref()))?;
        t.push(if part.cmp0() == Greater { '-' } else { '+' });
        t.push_str(&s);
    }
    t.push(')');
    Some((t, format!("Sub(x,{})", rat_formula(&r)?)))
}

fn make_fun(rng: &mut Rng) -> Option<Fun> {
    // The centre: a double, or a point no double holds.
    let centres: [Rational; 8] = [
        qi(1),
        qi(1),
        qi(0),
        qi(-2),
        Rational::from((1, 2)),
        qi(3),
        Rational::from((1, 3)),
        Rational::from((5, 7)),
    ];
    let c = rng.pick(&centres).clone();
    let up = Rational::from(&qd(doubles_out(floor_double(&c), 1)) - &c);
    let down = Rational::from(&c - &qd(doubles_out(ceil_double(&c), -1)));
    // Offsets from the centre: the doubles beside it, points between
    // doubles, and others well apart.
    let mut palette: Vec<Rational> = vec![
        qi(0),
        qi(0),
        up.clone(),
        Rational::from(-&down),
        Rational::from(&up * 2u32),
        Rational::from(&up / 2u32),
        Rational::from(-&down) / 2u32,
        Rational::from(&up * 3u32),
        Rational::from(-&down) * 2u32,
        Rational::from((1, 3)),
        Rational::from((-1, 2)),
        qi(1),
        qi(-1),
        qi(2),
        Rational::from((5, 4)),
        Rational::from((-7, 3)),
    ];
    if c == 0 {
        // About 0 the doubles are the subnormals: keep to a few.
        let tiny = Rational::from((1, 1 << 20));
        palette.retain(|o| o.cmp0() == Equal || Rational::from(o.abs_ref()) >= tiny);
        palette.push(qd(f64::from_bits(1)));
        palette.push(qd(-f64::from_bits(1)));
    }
    if rng.chance(0.05) {
        // A constant (a plateau everywhere).
        let v = *rng.pick(&[0i64, 2, -3]);
        let mut fun = Fun {
            text: v.to_string(),
            evaluated: Vec::new(),
            f: Poly::constant(qi(v)),
            dom: Dom::Line,
            planted: vec![c.clone()],
            expanded: Vec::new(),
        };
        modifier(rng, &mut fun, &c, &palette)?;
        return Some(fun);
    }
    let k = *rng.pick(&[0usize, 0, 1, 1, 1, 2, 2]);
    let mut offsets: Vec<Rational> = Vec::new();
    if rng.chance(0.3) {
        // Two at adjacent doubles (1 and 1 + 2⁻⁵²).
        offsets.push(qi(0));
        offsets.push(if rng.chance(0.5) {
            up.clone()
        } else {
            Rational::from(-&down)
        });
    }
    let n = (1 + rng.below(if k == 0 { 4 } else { 3 })).max(offsets.len());
    while offsets.len() < n {
        let o = rng.pick(&palette).clone();
        if !offsets.contains(&o) {
            offsets.push(o);
        }
    }
    let mults: Vec<u32> = offsets
        .iter()
        .map(|_| *rng.pick(&[1u32, 1, 1, 1, 2, 2, 3, 4]))
        .collect();
    let scale = rng
        .pick(&[qi(1), qi(-1), qi(2), qi(-3), Rational::from((1, 2))])
        .clone();
    let degree: u32 = mults.iter().sum::<u32>() + k as u32;
    if degree > 7 {
        return None;
    }
    // P(u), u = x − c.
    let mut pu = Poly::constant(scale.clone());
    for (o, m) in offsets.iter().zip(&mults) {
        pu = pu.mul(&Poly::x_minus(o).pow(*m));
    }
    let mut fu = pu.clone();
    for _ in 0..k {
        fu = fu.integral();
        if rng.chance(0.4) {
            let k0 = rng
                .pick(&[qi(1), qi(-1), Rational::from((1, 3)), up.clone()])
                .clone();
            fu = fu.add(&Poly::constant(k0));
        }
    }
    let planted: Vec<Rational> = offsets.iter().map(|o| Rational::from(&c + o)).collect();
    // The text: P itself factored, or f in powers of u.
    let factored = |lead: &Rational| -> Option<(String, String)> {
        let mut t = rat_text(lead)?;
        let mut fm = rat_formula(lead)?;
        for (o, m) in offsets.iter().zip(&mults) {
            let (xt, xf) = x_minus_text(&c, o)?;
            if *m == 1 {
                t = format!("{t}*{xt}");
                fm = format!("Mul({fm},{xf})");
            } else {
                t = format!("{t}*{xt}^{m}");
                fm = format!("Mul({fm},Pow({xf},{m}))");
            }
        }
        Some((t, fm))
    };
    let text = if k == 0 && rng.chance(0.6) {
        factored(&scale)?.0
    } else {
        let (u, _) = x_minus_text(&c, &qi(0))?;
        let mut t = String::new();
        for (j, a) in fu.0.iter().enumerate().rev() {
            if a.cmp0() == Equal {
                continue;
            }
            let s = rat_text(&Rational::from(a.abs_ref()))?;
            let term = match j {
                0 => s,
                1 => format!("{s}*{u}"),
                _ => format!("{s}*{u}^{j}"),
            };
            let minus = a.cmp0() == Less;
            if t.is_empty() {
                t = if minus { format!("-{term}") } else { term };
            } else {
                t = format!("{t}{}{term}", if minus { "-" } else { "+" });
            }
        }
        if t.is_empty() { "0".into() } else { t }
    };
    let mut evaluated = Vec::new();
    if k > 0 && rng.chance(0.5) {
        let name = if k == 1 { "f′" } else { "f″" };
        evaluated.push(format!("{name} = {}", factored(&scale)?.1));
    }
    let d1 = fu.deriv();
    let expanded = vec![
        format!("f′ = {}", poly_formula(&d1, &c)?),
        format!("f″ = {}", poly_formula(&d1.deriv(), &c)?),
    ];
    let mut fun = Fun {
        text,
        evaluated,
        f: fu.at_shift(&c),
        dom: Dom::Line,
        planted,
        expanded,
    };
    modifier(rng, &mut fun, &c, &palette)?;
    Some(fun)
}

/// A closed domain end or a hole, sometimes, at one of the points the
/// function was built about.
fn modifier(rng: &mut Rng, fun: &mut Fun, c: &Rational, palette: &[Rational]) -> Option<()> {
    if !rng.chance(0.35) {
        return Some(());
    }
    let at = if rng.chance(0.6) && !fun.planted.is_empty() {
        rng.pick(&fun.planted).clone()
    } else {
        Rational::from(c + rng.pick(palette))
    };
    let t = rat_text(&at)?;
    let (term, dom) = match rng.below(3) {
        0 => (format!("+0*sqrt(x-({t}))"), Dom::From(at)),
        1 => (format!("+0*sqrt({t}-x)"), Dom::Upto(at)),
        _ => (format!("+0/(x-({t}))"), Dom::Hole(at)),
    };
    fun.text.push_str(&term);
    fun.dom = dom;
    // (Its derivatives' trees, which the replay checks are f's exactly.)
    for t in fun.expanded.clone() {
        let name = &t[..t.find(" = ").expect("named")];
        if !fun.evaluated.iter().any(|e| e.starts_with(name)) {
            fun.evaluated.push(t);
        }
    }
    Some(())
}

// ------------------------------------------------------------ the truth

/// A point of the line: a rational, or a root of a polynomial.
#[derive(Clone, Debug)]
enum Place {
    At(Rational),
    Root(Root),
}

impl Place {
    fn cmp(&self, q: &Rational) -> Ordering {
        match self {
            Place::At(x) => x.cmp(q),
            Place::Root(r) => r.cmp(q),
        }
    }

    fn within(&self, a: f64, b: f64) -> bool {
        (!a.is_finite() || self.cmp(&qd(a)) != Less)
            && (!b.is_finite() || self.cmp(&qd(b)) != Greater)
    }

    fn sign_of(&mut self, g: &Poly) -> Ordering {
        match self {
            Place::At(x) => g.sign(x),
            Place::Root(r) => r.sign_of(g),
        }
    }

    /// The place's enclosure: its double, or the two beside it.
    fn enc(&mut self) -> (f64, f64) {
        match self {
            Place::At(x) => (floor_double(x), ceil_double(x)),
            Place::Root(r) => r.doubles(),
        }
    }

    /// f's value here lies in [lo, hi].
    fn value_within(&mut self, f: &Poly, lo: f64, hi: f64) -> bool {
        self.sign_of(&f.sub(&Poly::constant(qd(lo)))) != Less
            && self.sign_of(&f.sub(&Poly::constant(qd(hi)))) != Greater
    }

    fn approx(&self) -> f64 {
        match self {
            Place::At(x) => x.to_f64(),
            Place::Root(r) => (Rational::from(&r.lo + &r.hi) / 2u32).to_f64(),
        }
    }
}

/// Two places in order, exactly.
fn pcmp(a: &Place, b: &Place) -> Ordering {
    match (a, b) {
        (Place::At(x), Place::At(y)) => x.cmp(y),
        (Place::Root(r), Place::At(y)) => r.cmp(y),
        (Place::At(x), Place::Root(r)) => r.cmp(x).reverse(),
        (Place::Root(r), Place::Root(s)) => {
            if let Some(x) = r.exact() {
                return s.cmp(x).reverse();
            }
            if let Some(y) = s.exact() {
                return r.cmp(y);
            }
            let (mut r, mut s) = (r.clone(), s.clone());
            for _ in 0..400 {
                if r.hi <= s.lo {
                    return Less;
                }
                if s.hi <= r.lo {
                    return Greater;
                }
                // (Isolating intervals of one square-free polynomial that
                // meet hold one root.)
                if r.p == s.p {
                    return Equal;
                }
                r.bisect();
                s.bisect();
                if let Some(x) = r.exact().cloned() {
                    return s.cmp(&x).reverse();
                }
                if let Some(y) = s.exact().cloned() {
                    return r.cmp(&y);
                }
            }
            // One root of both: their common factor's.
            let g = gcd(&r.p, &s.p);
            let lo = (&r.lo).max(&s.lo).clone();
            let hi = (&r.hi).min(&s.hi).clone();
            assert!(
                g.degree() >= 1 && Sturm::new(&g).closed(&lo, &hi) > 0,
                "two roots not told apart"
            );
            Equal
        }
    }
}

/// A zero, turn or inflection: where, and (a turn) whether a maximum.
#[derive(Clone, Debug)]
struct Feature {
    at: Place,
    max: Option<bool>,
}

struct Truth {
    /// f, f′, f″, f‴.
    d: [Poly; 4],
    dom: Dom,
    /// The distinct real roots of f, f′, f″, f‴ (anywhere).
    roots: [Vec<Root>; 4],
    zeros: Vec<Feature>,
    extrema: Vec<Feature>,
    inflections: Vec<Feature>,
    /// Where f′ changes sign inside the domain, and the domain's end or
    /// hole: where monotone pieces end.
    breaks: Vec<Place>,
}

impl Truth {
    fn new(f: &Poly, dom: &Dom) -> Truth {
        let d1 = f.deriv();
        let d2 = d1.deriv();
        let d3 = d2.deriv();
        let d = [f.clone(), d1, d2, d3];
        let roots = [0, 1, 2, 3].map(|k| {
            if d[k].is_zero() {
                Vec::new()
            } else {
                real_roots(&d[k])
            }
        });
        let changes = |k: usize| -> Vec<Feature> {
            roots[k]
                .iter()
                .filter(|r| dom.inside(&Place::Root((*r).clone())))
                .filter_map(|r| {
                    let (l, rr) = sides(&d[k], r);
                    (l != rr && l != Equal && rr != Equal).then(|| Feature {
                        at: Place::Root(r.clone()),
                        max: Some(l == Greater),
                    })
                })
                .collect()
        };
        let zeros = roots[0]
            .iter()
            .filter(|r| dom.holds(&Place::Root((*r).clone())))
            .map(|r| Feature {
                at: Place::Root(r.clone()),
                max: None,
            })
            .collect();
        let mut extrema = changes(1);
        if f.degree() > 0 {
            let end = match dom {
                Dom::From(c) => Some((c, true)),
                Dom::Upto(c) => Some((c, false)),
                _ => None,
            };
            if let Some((c, right)) = end {
                let g = f.sub(&Poly::constant(f.eval(c)));
                let s = sign_beside(&g, c, right);
                extrema.push(Feature {
                    at: Place::At(c.clone()),
                    max: Some(s == Less),
                });
            }
        }
        let mut inflections = changes(2);
        for i in &mut inflections {
            i.max = None;
        }
        let mut breaks: Vec<Place> = changes(1).into_iter().map(|t| t.at).collect();
        if let Some(c) = dom.point() {
            breaks.push(Place::At(c.clone()));
        }
        breaks.sort_by(|a, b| a.approx().total_cmp(&b.approx()));
        Truth {
            d,
            dom: dom.clone(),
            roots,
            zeros,
            extrema,
            inflections,
            breaks,
        }
    }

    fn constant(&self) -> bool {
        self.d[1].is_zero()
    }

    /// How many distinct roots f⁽ᵏ⁾ has in [a, b] (≡ 0: all).
    fn roots_in(&self, k: usize, a: f64, b: f64) -> usize {
        if self.d[k].is_zero() {
            return usize::MAX;
        }
        self.roots[k]
            .iter()
            .filter(|r| Place::Root((*r).clone()).within(a, b))
            .count()
    }

    /// The roots of f⁽ᵏ⁾ in [a, b].
    fn roots_within(&self, k: usize, a: f64, b: f64) -> Vec<Root> {
        self.roots[k]
            .iter()
            .filter(|r| Place::Root((*r).clone()).within(a, b))
            .cloned()
            .collect()
    }

    /// f⁽ᵏ⁾'s strict sign over [a, b] (closed, ends may be infinite), if
    /// it has one.
    fn sign_over(&self, k: usize, a: f64, b: f64) -> Option<bool> {
        if self.d[k].is_zero() || self.roots_in(k, a, b) > 0 {
            return None;
        }
        let m = match (a.is_finite(), b.is_finite()) {
            (true, true) => Rational::from(&qd(a) + &qd(b)) / 2u32,
            (true, false) => Rational::from(&qd(a) + 1u32),
            (false, true) => Rational::from(&qd(b) - 1u32),
            (false, false) => qi(0),
        };
        Some(self.d[k].sign(&m) == Greater)
    }

    /// The value of f⁽ᵏ⁾ at a double.
    fn at(&self, k: usize, x: f64) -> Rational {
        self.d[k].eval(&qd(x))
    }

    /// The sign of f⁽ᵏ⁾ at a double.
    fn sign_at(&self, k: usize, x: f64) -> Ordering {
        self.d[k].sign(&qd(x))
    }

    /// The roots of f⁽ᵏ⁾ strictly inside (a, b) where f is defined.
    fn roots_strictly_inside(&self, k: usize, a: f64, b: f64) -> usize {
        self.roots[k]
            .iter()
            .filter(|r| {
                let p = Place::Root((*r).clone());
                (!a.is_finite() || p.cmp(&qd(a)) == Greater)
                    && (!b.is_finite() || p.cmp(&qd(b)) == Less)
                    && self.dom.holds(&p)
            })
            .count()
    }

    /// Whether a claim is true (None: a kind this truth doesn't read).
    /// f⁽ᵏ⁾ is the polynomial's own derivative throughout the domain, at a
    /// closed end too, one-sided (as the replay reads `0*sqrt(u)`).
    fn claim_true(&self, c: &Claim) -> Option<bool> {
        let order = |of: &Subject| match of {
            Subject::F(k) if *k <= 2 => Some(*k),
            _ => None,
        };
        let ok_box = |x: &(f64, f64)| x.0 <= x.1 && self.dom.holds_box(x.0, x.1);
        let sign_is = |k: usize, a: f64, b: f64, above: bool| {
            ok_box(&(a, b)) && self.sign_over(k, a, b) == Some(above)
        };
        let tail = |side: Side, from: f64| match side {
            Side::Left => (f64::NEG_INFINITY, from),
            Side::Right => (from, f64::INFINITY),
        };
        Some(match c {
            Claim::Defined(x) | Claim::Continuous(x) | Claim::Kink(x) => ok_box(x),
            Claim::Undefined(x) => x.0 <= x.1 && self.dom.misses_box(x.0, x.1),
            Claim::Beyond { x, of, c, above }
            | Claim::NoCross { x, of, c, above }
            | Claim::Factors { x, of, c, above } => {
                let k = order(of)?;
                if *c != 0.0 {
                    return None;
                }
                sign_is(k, x.0, x.1, *above)
            }
            Claim::TailBeyond {
                side,
                from,
                of,
                c,
                above,
            }
            | Claim::TailChain {
                side,
                from,
                of,
                c,
                above,
                ..
            } => {
                let k = order(of)?;
                if *c != 0.0 {
                    return None;
                }
                let (a, b) = tail(*side, *from);
                sign_is(k, a, b, *above)
            }
            Claim::OneCross { x, of, c } => {
                let k = order(of)?;
                if *c != 0.0 || self.d[k].is_zero() {
                    return None;
                }
                let (a, b) = (self.sign_at(k, x.0), self.sign_at(k, x.1));
                ok_box(x) && a != Equal && b != Equal && a != b && self.roots_in(k, x.0, x.1) == 1
            }
            Claim::ExactAt { x, of, c, at } => {
                let k = order(of)?;
                if *c != 0.0 || self.d[k].is_zero() {
                    return None;
                }
                ok_box(x)
                    && x.0 <= *at
                    && *at <= x.1
                    && self.sign_at(k, *at) == Equal
                    && self.roots_in(k, x.0, x.1) == 1
            }
            Claim::Touch {
                x,
                of,
                c,
                at,
                above,
                ..
            } => {
                let k = order(of)?;
                if *c != 0.0 || self.d[k].is_zero() {
                    return None;
                }
                let other = if *at == x.0 { x.1 } else { x.0 };
                ok_box(x)
                    && (*at == x.0 || *at == x.1)
                    && self.sign_at(k, *at) == Equal
                    && self.roots_in(k, x.0, x.1) == 1
                    && (x.0 == x.1 || self.sign_at(k, other) == if *above { Greater } else { Less })
            }
            Claim::Value { x, of, lo, hi } => {
                let k = order(of)?;
                ok_box(x)
                    && lo.is_finite()
                    && hi.is_finite()
                    && x.0.is_finite()
                    && x.1.is_finite()
                    && values_within(&self.d[k], &qd(x.0), &qd(x.1), &qd(*lo), &qd(*hi))
            }
            Claim::Gap(x) => x.0 <= x.1,
            Claim::GapClear { x, of } => {
                let k = order(of)?;
                self.d[k].is_zero() || self.roots_strictly_inside(k, x.0, x.1) == 0
            }
            Claim::KinkSides {
                x,
                at,
                order: k,
                left,
                right,
                ..
            } => {
                if *k > 2 || *k == 0 || !ok_box(x) || x.0 >= x.1 {
                    return Some(false);
                }
                let side = |s: &KinkSign, a: f64, b: f64, open_a: bool, open_b: bool| {
                    // f⁽ᵏ⁾'s sign on the side, its open ends left out.
                    if self.d[*k].is_zero() {
                        return *s == KinkSign::Zero;
                    }
                    let want = match s {
                        KinkSign::Zero => return false,
                        KinkSign::Pos => Greater,
                        KinkSign::Neg => Less,
                    };
                    let inside = self.roots[*k].iter().any(|r| {
                        let p = Place::Root(r.clone());
                        let lo = p.cmp(&qd(a));
                        let hi = p.cmp(&qd(b));
                        (lo == Greater || (lo == Equal && !open_a))
                            && (hi == Less || (hi == Equal && !open_b))
                    });
                    let m = Rational::from(&qd(a) + &qd(b)) / 2u32;
                    !inside && self.d[*k].sign(&m) == want
                };
                match at {
                    Some(p) => {
                        x.0 < *p
                            && *p < x.1
                            && side(left, x.0, *p, false, true)
                            && side(right, *p, x.1, true, false)
                    }
                    None => {
                        // Some c strictly inside with these signs on
                        // either side of it.
                        let rs = self.roots_within(*k, x.0, x.1);
                        if self.d[*k].is_zero() {
                            *left == KinkSign::Zero && *right == KinkSign::Zero
                        } else if rs.is_empty() {
                            left == right && side(left, x.0, x.1, false, false)
                        } else if rs.len() == 1 {
                            let mut r = Place::Root(rs[0].clone());
                            let inner = r.cmp(&qd(x.0)) == Greater && r.cmp(&qd(x.1)) == Less;
                            let (l, rr) = sides(&self.d[*k], &rs[0]);
                            let want = |s: &KinkSign| match s {
                                KinkSign::Pos => Greater,
                                KinkSign::Neg => Less,
                                KinkSign::Zero => Equal,
                            };
                            let _ = r.enc();
                            inner && l == want(left) && rr == want(right)
                        } else {
                            false
                        }
                    }
                }
            }
            Claim::KinkAt {
                x, left, right, at, ..
            } => {
                ok_box(x)
                    && x.0 < *at
                    && *at < x.1
                    && self.sign_over(1, x.0, *at) == Some(*left)
                    && self.sign_over(1, *at, x.1) == Some(*right)
            }
            _ => return None,
        })
    }
}

// ------------------------------------------------------------ claims as JSON

/// A double as the certificate writes it.
fn num(d: f64) -> Value {
    if d == f64::INFINITY {
        json!("inf")
    } else if d == f64::NEG_INFINITY {
        json!("-inf")
    } else {
        json!(d)
    }
}

fn enc(lo: f64, hi: f64) -> Value {
    json!({"lo": num(lo), "hi": num(hi)})
}

fn bx(a: f64, b: f64) -> Value {
    json!({"a": num(a), "b": num(b)})
}

fn of(k: usize) -> Value {
    json!({"F": format!("F{k}")})
}

fn c_sign(kind: &str, k: usize, a: f64, b: f64, above: bool) -> Value {
    json!({kind: {"x": bx(a, b), "of": of(k), "c": 0.0, "above": above}})
}

fn c_value(k: usize, a: f64, b: f64, lo: f64, hi: f64) -> Value {
    json!({"Value": {"x": bx(a, b), "of": of(k), "lo": num(lo), "hi": num(hi)}})
}

fn c_exact(k: usize, a: f64, b: f64, at: f64) -> Value {
    json!({"ExactAt": {"x": bx(a, b), "of": of(k), "c": 0.0, "at": num(at)}})
}

fn c_cross(k: usize, a: f64, b: f64) -> Value {
    json!({"OneCross": {"x": bx(a, b), "of": of(k), "c": 0.0}})
}

fn c_touch(k: usize, a: f64, b: f64, at: f64, above: bool) -> Value {
    json!({"Touch": {"x": bx(a, b), "of": of(k), "c": 0.0, "at": num(at), "order": 0,
        "above": above}})
}

fn c_box(kind: &str, a: f64, b: f64) -> Value {
    json!({kind: {"x": bx(a, b)}})
}

fn c_clear(k: usize, a: f64, b: f64) -> Value {
    json!({"GapClear": {"x": bx(a, b), "of": of(k)}})
}

fn c_tail(k: usize, right: bool, from: f64, above: bool) -> Value {
    json!({"TailBeyond": {"side": if right { "Right" } else { "Left" }, "from": num(from),
        "of": of(k), "c": 0.0, "above": above}})
}

fn kink_sign(s: Ordering) -> &'static str {
    match s {
        Greater => "Pos",
        Less => "Neg",
        Equal => "Zero",
    }
}

fn c_sides(k: usize, a: f64, b: f64, at: Option<f64>, l: Ordering, r: Ordering) -> Value {
    json!({"KinkSides": {"x": bx(a, b), "at": at.map_or(Value::Null, num), "order": k,
        "left": kink_sign(l), "right": kink_sign(r), "joined": k == 2 && at.is_some()}})
}

// ------------------------------------------------------------ claim sets

/// Makes up true claims about f⁽ᵏ⁾ over stretches of x, aimed at the
/// shapes that bear on row inference: boxes ending on a feature or a
/// double off it, one-ulp gaps, points at and beside zeros.
struct Gen<'a> {
    t: &'a Truth,
    k: usize,
    rng: &'a mut Rng,
    /// The doubles boxes start and end at.
    atoms: &'a [f64],
    out: Vec<Value>,
}

impl Gen<'_> {
    fn push(&mut self, c: Value) {
        self.out.push(c);
    }

    fn above(&self, x: f64) -> Option<bool> {
        match self.t.sign_at(self.k, x) {
            Equal => None,
            s => Some(s == Greater),
        }
    }

    /// f⁽ᵏ⁾ strictly monotone over [a, b] (f⁽ᵏ⁺¹⁾ without a root there).
    fn monotone(&self, a: f64, b: f64) -> bool {
        self.t.roots_in(self.k + 1, a, b) == 0
    }

    /// Claims about the point x.
    fn point(&mut self, x: f64) {
        let k = self.k;
        if self.t.dom.misses_box(x, x) {
            if self.rng.chance(0.5) {
                self.push(c_box("Undefined", x, x));
            }
            return;
        }
        if !self.t.dom.holds_box(x, x) {
            return;
        }
        match self.above(x) {
            None if self.t.d[k].is_zero() => {
                if k > 0 && self.rng.chance(0.4) {
                    self.push(c_box("Kink", x, x));
                } else {
                    self.push(c_value(k, x, x, 0.0, 0.0));
                }
            }
            None => match self.rng.below(5) {
                0 | 1 if k > 0 => self.push(c_box("Kink", x, x)),
                2 if self.monotone(x, x) => self.push(c_exact(k, x, x, x)),
                _ => self.push(c_value(k, x, x, 0.0, 0.0)),
            },
            Some(up) => match self.rng.below(4) {
                0 if k > 0 => self.push(c_box("Kink", x, x)),
                1 => {
                    let v = self.t.at(k, x);
                    self.push(c_value(k, x, x, floor_double(&v), ceil_double(&v)));
                }
                _ => self.push(c_sign("Beyond", k, x, x, up)),
            },
        }
        if self.rng.chance(0.1) {
            let kind = *self.rng.pick(&["Defined", "Continuous"]);
            self.push(c_box(kind, x, x));
        }
    }

    /// A gap over (a, b), cleared of f⁽ᵏ⁾'s zeros where it is.
    fn gap(&mut self, a: f64, b: f64) {
        if !narrow(a, b) {
            return;
        }
        self.push(c_box("Gap", a, b));
        if self.t.d[self.k].is_zero() || self.t.roots_strictly_inside(self.k, a, b) == 0 {
            if self.rng.chance(0.9) {
                self.push(c_clear(self.k, a, b));
            }
        } else if self.rng.chance(0.2) {
            // (A false clearness, which the claim check must refuse.)
            self.push(c_clear(self.k, a, b));
        }
    }

    /// Claims covering [a, b] (a < b, both atoms).
    fn stretch(&mut self, a: f64, b: f64, depth: u32) {
        let k = self.k;
        if a >= b || depth > 40 {
            return;
        }
        if self.t.dom.misses_box(a, b) {
            if self.rng.chance(0.4) {
                self.push(c_box("Undefined", a, b));
            }
            return;
        }
        let inner: Vec<f64> = self
            .atoms
            .iter()
            .copied()
            .filter(|g| a < *g && *g < b)
            .collect();
        let split = |g: &mut Gen, inner: &[f64]| {
            // At every atom inside, each piece then elementary.
            let mut last = a;
            for &m in inner.iter().chain(std::iter::once(&b)) {
                g.stretch(last, m, depth + 1);
                if m < b && g.rng.chance(0.2) {
                    g.point(m);
                }
                last = m;
            }
        };
        if !self.t.dom.holds_box(a, b) {
            // A domain end or the hole inside: elementary pieces, and a gap
            // over the one that holds it.
            if inner.is_empty() {
                self.gap(a, b);
            } else {
                split(self, &inner);
            }
            return;
        }
        if self.t.d[k].is_zero() {
            match self.rng.below(4) {
                0 if k > 0 => self.push(c_box("Kink", a, b)),
                1 if k > 0 => self.push(c_sides(k, a, b, None, Equal, Equal)),
                2 if narrow(a, b) => self.gap(a, b),
                _ => self.push(c_value(k, a, b, 0.0, 0.0)),
            }
            return;
        }
        let rs = self.t.roots_within(k, a, b);
        match rs.len() {
            0 => {
                let up = self.t.sign_over(k, a, b).expect("no root, a sign");
                if narrow(a, b) && self.rng.chance(0.3) {
                    self.gap(a, b);
                    return;
                }
                for n in 0..2 {
                    if n == 1 && !self.rng.chance(0.3) {
                        break;
                    }
                    match self.rng.below(10) {
                        0 => self.push(c_sign("NoCross", k, a, b, up)),
                        1 if k == 0 => self.push(c_sign("Factors", k, a, b, up)),
                        2 | 3 if k > 0 => self.push(c_box("Kink", a, b)),
                        4 if k > 0 => {
                            let s = if up { Greater } else { Less };
                            self.push(c_sides(k, a, b, None, s, s));
                        }
                        _ => self.push(c_sign("Beyond", k, a, b, up)),
                    }
                }
            }
            1 => {
                let mut r = rs[0].clone();
                let (rl, rh) = r.doubles();
                let (sl, sr) = sides(&self.t.d[k], &r);
                let change = sl != sr;
                if rl == rh {
                    let e = rl;
                    if a < e && e < b {
                        match self.rng.below(5) {
                            0 | 1 if self.monotone(a, b) => self.push(c_exact(k, a, b, e)),
                            2 if k > 0 => {
                                self.push(c_sides(k, a, b, Some(e), sl, sr));
                            }
                            _ => {
                                self.stretch(a, e, depth + 1);
                                if self.rng.chance(0.6) {
                                    self.point(e);
                                }
                                self.stretch(e, b, depth + 1);
                            }
                        }
                    } else {
                        // The root at an end of the box.
                        let other = if e == a { b } else { a };
                        let s = self.above(other);
                        match self.rng.below(4) {
                            0 if self.monotone(a, b) => self.push(c_exact(k, a, b, e)),
                            1 | 2 if s.is_some() => {
                                self.push(c_touch(k, a, b, e, s.expect("signed")));
                            }
                            _ if !inner.is_empty() => split(self, &inner),
                            _ => self.gap(a, b),
                        }
                    }
                } else if a < rl && rh < b {
                    // Between two doubles inside: a box about it.
                    self.stretch(a, rl, depth + 1);
                    self.stretch(rl, rh, depth + 1);
                    self.stretch(rh, b, depth + 1);
                } else {
                    // The box is [rl, rh] (or within it).
                    let (pa, pb) = (self.above(a), self.above(b));
                    match self.rng.below(4) {
                        0 | 1 if change && pa.is_some() && pb.is_some() => {
                            self.push(c_cross(k, a, b));
                        }
                        2 if k > 0 && pa.is_some() && pb.is_some() => {
                            self.push(c_sides(k, a, b, None, sl, sr));
                        }
                        _ => self.gap(a, b),
                    }
                }
            }
            _ => {
                if inner.is_empty() {
                    self.gap(a, b);
                } else {
                    split(self, &inner);
                }
            }
        }
    }

    /// Claims covering [lo, hi] (either end may be infinite: a tail).
    fn cover(&mut self, lo: f64, hi: f64) {
        let k = self.k;
        let mut cuts: Vec<f64> = self
            .atoms
            .iter()
            .copied()
            .filter(|g| lo <= *g && *g <= hi)
            .filter(|g| *g == lo || *g == hi || self.rng.chance(0.55))
            .collect();
        // (A tail from the outermost atom: past every root of f⁽ᵏ⁾ and
        // its derivatives.)
        if lo == f64::NEG_INFINITY && self.rng.chance(0.9) {
            cuts.push(self.atoms[0]);
        }
        if hi == f64::INFINITY && self.rng.chance(0.9) {
            cuts.push(*self.atoms.last().expect("atoms"));
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        if cuts.is_empty() {
            return;
        }
        let deg = self.t.d[k].degree().max(1);
        for (inf, from, right) in [
            (lo, cuts[0], false),
            (hi, *cuts.last().expect("cuts"), true),
        ] {
            if inf.is_finite() {
                continue;
            }
            let (a, b) = if right {
                (from, f64::INFINITY)
            } else {
                (f64::NEG_INFINITY, from)
            };
            if let Some(up) = self.t.sign_over(k, a, b)
                && self.t.dom.holds_box(a, b)
            {
                let n = self.rng.below(10);
                if n < 8 {
                    self.push(c_tail(k, right, from, up));
                }
                if n >= 6 {
                    let mut c = c_tail(k, right, from, up);
                    let body = c["TailBeyond"].take();
                    c = json!({"TailChain": body});
                    c["TailChain"]["order"] = json!(deg);
                    self.push(c);
                }
            }
        }
        if cuts.len() == 1 {
            self.point(cuts[0]);
        }
        for w in cuts.windows(2) {
            self.stretch(w[0], w[1], 0);
            if self.rng.chance(0.15) {
                self.point(w[1]);
            }
        }
        if self.rng.chance(0.3) {
            self.point(cuts[0]);
        }
    }

    /// Two crossing boxes about one change of f⁽ᵏ⁾'s sign, overlapping,
    /// neither holding the other: f⁽ᵏ⁾ strictly monotone on both is so on
    /// their union, so they hold one zero.
    fn crossings(&mut self) {
        let k = self.k;
        let changes: Vec<Root> = self.t.roots[k]
            .iter()
            .filter(|r| {
                let (l, rr) = sides(&self.t.d[k], r);
                l != rr
            })
            .cloned()
            .collect();
        if changes.is_empty() {
            return;
        }
        let mut r = self.rng.pick(&changes).clone();
        let (lo, hi) = r.doubles();
        let (a1, a2) = if lo == hi {
            (doubles_out(lo, -2), doubles_out(lo, -1))
        } else {
            (doubles_out(lo, -1), lo)
        };
        let (b1, b2) = if lo == hi {
            (doubles_out(hi, 1), doubles_out(hi, 2))
        } else {
            (hi, doubles_out(hi, 1))
        };
        let wide = if self.rng.chance(0.5) { 3 } else { 0 };
        self.push(c_cross(k, doubles_out(a1, -wide), b1));
        self.push(c_cross(k, a2, doubles_out(b2, wide)));
    }

    /// Claims at and about the zeros of f⁽ᵏ⁾: a point, a one-ulp gap and
    /// a point beside it, a sign from a double off it.
    fn noise(&mut self) {
        let k = self.k;
        let near: Vec<f64> = self.t.roots[k]
            .iter()
            .flat_map(|r| {
                let (lo, hi) = r.clone().doubles();
                [-2, -1, 0, 1, 2].map(|i| {
                    if i <= 0 {
                        doubles_out(lo, i)
                    } else {
                        doubles_out(hi, i)
                    }
                })
            })
            .chain(self.t.dom.point().iter().flat_map(|c| {
                let (lo, hi) = (floor_double(c), ceil_double(c));
                [doubles_out(lo, -1), lo, hi, doubles_out(hi, 1)]
            }))
            .collect();
        if near.is_empty() {
            return;
        }
        for _ in 0..self.rng.below(4) {
            let x = *self.rng.pick(&near);
            match self.rng.below(6) {
                0 | 1 if k > 0 => self.push(c_box("Kink", x, x)),
                2 => {
                    let y = doubles_out(x, if self.rng.chance(0.5) { 1 } else { -1 });
                    self.gap(x.min(y), x.max(y));
                }
                3 => self.point(x),
                4 => {
                    let y = *self.rng.pick(self.atoms);
                    if y != x {
                        self.stretch(x.min(y), x.max(y), 30);
                    }
                }
                _ => {
                    let y = *self.rng.pick(self.atoms);
                    let (a, b) = (x.min(y), x.max(y));
                    let kind = *self.rng.pick(&["Kink", "Continuous", "Defined"]);
                    self.push(c_box(kind, a, b));
                }
            }
        }
    }
}

// ------------------------------------------------------------ rows

#[derive(Clone, Debug, PartialEq)]
enum Scope {
    Line,
    Window(f64, f64),
    Points,
}

impl Scope {
    fn covers(&self) -> Value {
        match self {
            Scope::Line => json!("Line"),
            Scope::Window(a, b) => json!({"Window": {"a": num(*a), "b": num(*b)}}),
            Scope::Points => json!("Points"),
        }
    }

    fn status(&self) -> &'static str {
        if *self == Scope::Line {
            "Certified"
        } else {
            "Partial"
        }
    }

    fn complete(&self) -> bool {
        *self != Scope::Points
    }

    fn holds(&self, p: &Place) -> bool {
        match self {
            Scope::Line => true,
            Scope::Window(a, b) => p.within(*a, *b),
            Scope::Points => false,
        }
    }
}

const ROWS: [(&str, usize); 4] = [
    ("x_intercepts", 0),
    ("extrema", 1),
    ("inflections", 2),
    ("monotonicity", 1),
];

/// A verdict of the truth on a row.
#[derive(Clone, Debug, PartialEq)]
enum Verdict {
    True,
    False(String),
}

/// A listed item: where, f's value there and (an extremum) its kind.
struct Item {
    x: (f64, f64),
    y: Option<(f64, f64)>,
    max: Option<bool>,
    family: bool,
}

fn enc_of(v: &Value) -> Option<(f64, f64)> {
    Some((replay::r(v.get("lo")?).ok()?, replay::r(v.get("hi")?).ok()?))
}

fn items_of(row: &str, value: &Value) -> Option<Vec<Item>> {
    value
        .as_array()?
        .iter()
        .map(|it| {
            if row == "x_intercepts" {
                if let Some(a) = it.get("At") {
                    return Some(Item {
                        x: enc_of(a)?,
                        y: None,
                        max: None,
                        family: false,
                    });
                }
                return Some(Item {
                    x: enc_of(it.get("Every")?.get("x0")?)?,
                    y: None,
                    max: None,
                    family: true,
                });
            }
            Some(Item {
                x: enc_of(it.get("x")?)?,
                y: enc_of(it.get("y")?),
                max: it.get("kind").and_then(Value::as_str).map(|k| k == "Max"),
                family: it.get("every").is_some_and(|e| !e.is_null()),
            })
        })
        .collect()
}

/// A bipartite matching that gives each of `left` an item of its own.
fn matched(options: &[Vec<usize>]) -> Option<usize> {
    fn assign(
        p: usize,
        options: &[Vec<usize>],
        owner: &mut BTreeMap<usize, usize>,
        seen: &mut Vec<bool>,
    ) -> bool {
        for &o in &options[p] {
            if seen[o] {
                continue;
            }
            seen[o] = true;
            let free = match owner.get(&o).copied() {
                None => true,
                Some(q) => assign(q, options, owner, seen),
            };
            if free {
                owner.insert(o, p);
                return true;
            }
        }
        false
    }
    let n = options.iter().flatten().max().map_or(0, |m| m + 1);
    let mut owner = BTreeMap::new();
    for p in 0..options.len() {
        let mut seen = vec![false; n];
        if !assign(p, options, &mut owner, &mut seen) {
            return Some(p);
        }
    }
    None
}

/// A monotone piece's end: where (an enclosure), whether included; None:
/// infinite.
type End = Option<((f64, f64), bool)>;

fn end_of(v: &Value) -> Option<End> {
    if v.as_str().is_some() {
        return Some(None);
    }
    let at = v.get("At")?;
    Some(Some((enc_of(at.get("x")?)?, at.get("closed")?.as_bool()?)))
}

impl Truth {
    fn features(&self, row: &str) -> Vec<Feature> {
        match row {
            "x_intercepts" => self.zeros.clone(),
            "extrema" => self.extrema.clone(),
            _ => self.inflections.clone(),
        }
    }

    /// The truth of a list row (x-intercepts, extrema, inflections).
    fn judge_list(&self, row: &str, scope: &Scope, value: &Value) -> Verdict {
        let Some(items) = items_of(row, value) else {
            return Verdict::False("unreadable".into());
        };
        if row == "x_intercepts" && self.d[0].is_zero() {
            // f ≡ 0: every point a zero, so an item is one where it meets
            // the domain; a complete list only over a window of one point.
            if items.iter().any(|i| self.dom.misses_box(i.x.0, i.x.1)) {
                return Verdict::False("an item outside the domain".into());
            }
            return match scope {
                Scope::Points => Verdict::True,
                // (No point of the domain in the window: no zero.)
                Scope::Window(a, b) if self.dom.misses_box(*a, *b) => Verdict::True,
                Scope::Window(a, b)
                    if a == b
                        || matches!(&self.dom, Dom::From(c) if qd(*b) == *c)
                        || matches!(&self.dom, Dom::Upto(c) if qd(*a) == *c) =>
                {
                    // One point of the domain in the window: its zero.
                    let p = if matches!(&self.dom, Dom::From(_)) {
                        *b
                    } else {
                        *a
                    };
                    if !self.dom.holds_q(&qd(p)) || items.iter().any(|i| i.x.0 <= p && p <= i.x.1) {
                        Verdict::True
                    } else {
                        Verdict::False(format!("f ≡ 0, and its zero at {p:e} is not listed"))
                    }
                }
                _ => Verdict::False("f ≡ 0: its zeros are no list".into()),
            };
        }
        let mut feats = self.features(row);
        let ok = |it: &Item, f: &mut Feature| -> bool {
            f.at.within(it.x.0, it.x.1)
                && (it.max.is_none() || it.max == f.max)
                && it
                    .y
                    .is_none_or(|(lo, hi)| f.at.value_within(&self.d[0], lo, hi))
        };
        let mut options: Vec<Vec<usize>> = vec![Vec::new(); feats.len()];
        for (j, it) in items.iter().enumerate() {
            if it.family {
                return Verdict::False(format!("a family listed at {:e}", it.x.0));
            }
            let mut any = false;
            for (i, f) in feats.iter_mut().enumerate() {
                if ok(it, f) {
                    options[i].push(j);
                    any = true;
                }
            }
            if !any {
                return Verdict::False(format!(
                    "the item at [{:e}, {:e}] is no {row} item",
                    it.x.0, it.x.1
                ));
            }
        }
        // A list of features names each once: two items for one zero,
        // turn or inflection say there are two.
        let mut held: Vec<Vec<usize>> = vec![Vec::new(); items.len()];
        for (i, o) in options.iter().enumerate() {
            for j in o {
                held[*j].push(i);
            }
        }
        if let Some(j) = matched(&held) {
            return Verdict::False(format!(
                "the item at [{:e}, {:e}] holds no {row} feature of its own",
                items[j].x.0, items[j].x.1
            ));
        }
        if !scope.complete() {
            return Verdict::True;
        }
        let inside: Vec<usize> = (0..feats.len())
            .filter(|i| scope.holds(&feats[*i].at))
            .collect();
        let opts: Vec<Vec<usize>> = inside.iter().map(|i| options[*i].clone()).collect();
        match matched(&opts) {
            None => Verdict::True,
            Some(p) => {
                let f = &mut feats[inside[p]];
                let (lo, hi) = f.at.enc();
                Verdict::False(format!(
                    "the {row} item at [{lo:e}, {hi:e}] in {scope:?} is not listed (or shares \
                     an item)"
                ))
            }
        }
    }

    /// f's maximal monotone stretches on its domain: (from, to, each end
    /// included, direction; None: constant), an end None where infinite.
    #[allow(clippy::type_complexity)]
    fn stretches(&self) -> Vec<(Option<Place>, bool, Option<Place>, bool, Option<bool>)> {
        let mut ends: Vec<(Option<Place>, bool)> = Vec::new();
        ends.push(match &self.dom {
            Dom::From(c) => (Some(Place::At(c.clone())), true),
            _ => (None, false),
        });
        let mut inner: Vec<(Place, bool)> = Vec::new();
        for b in &self.breaks {
            match (&self.dom, b) {
                (Dom::From(c) | Dom::Upto(c), Place::At(x)) if x == c => {}
                (Dom::Hole(c), Place::At(x)) if x == c => inner.push((b.clone(), false)),
                _ => inner.push((b.clone(), true)),
            }
        }
        // (In order, exactly: a rational against a root by the root's
        // own comparison; two turns are roots of one square-free f′, apart.)
        inner.sort_by(|(a, _), (b, _)| pcmp(a, b));
        ends.extend(inner.into_iter().map(|(p, c)| (Some(p), c)));
        ends.push(match &self.dom {
            Dom::Upto(c) => (Some(Place::At(c.clone())), true),
            _ => (None, false),
        });
        let d1 = &self.d[1];
        ends.windows(2)
            .map(|w| {
                let dir = if d1.is_zero() {
                    None
                } else {
                    // f′'s sign just inside the stretch.
                    let s = match (&w[0].0, &w[1].0) {
                        (Some(Place::At(x)), _) => sign_beside(d1, x, true),
                        (Some(Place::Root(r)), _) => sides(d1, r).1,
                        (None, Some(Place::At(x))) => sign_beside(d1, x, false),
                        (None, Some(Place::Root(r))) => sides(d1, r).0,
                        (None, None) => sign_beside(d1, &qi(0), true),
                    };
                    Some(s == Greater)
                };
                (w[0].0.clone(), w[0].1, w[1].0.clone(), w[1].1, dir)
            })
            .collect()
    }

    /// The piece from `lo` to `hi` is part of a monotone stretch of f in
    /// its direction: some p in lo's enclosure and q > p in hi's lie in
    /// one stretch (p the stretch's open end only where lo is open).
    fn piece_true(&self, lo: &End, hi: &End, dir: Option<bool>) -> Result<(), String> {
        let le = |p: &Option<Place>, x: f64, low: bool| -> bool {
            // The stretch end p (None: −∞ for its low end, +∞ for its high
            // one) at or below x.
            match p {
                None => low,
                Some(p) => p.cmp(&qd(x)) != Greater,
            }
        };
        let lt = |p: &Option<Place>, x: f64, low: bool| -> bool {
            match p {
                None => low,
                Some(p) => p.cmp(&qd(x)) == Less,
            }
        };
        let eq = |p: &Option<Place>, x: f64| p.as_ref().is_some_and(|p| p.cmp(&qd(x)) == Equal);
        let mut why = "no stretch of f holds it".to_string();
        for (b0, c0, b1, c1, d) in self.stretches() {
            if d != dir {
                continue;
            }
            // p in lo ∩ [b0, b1], q in hi ∩ [b0, b1], p < q.
            let lo_ok = match lo {
                None => b0.is_none(),
                Some((x, closed)) => {
                    le(&b0, x.1, true) && !le(&b1, x.0, false) && !(*closed && !c0 && eq(&b0, x.1))
                }
            };
            let hi_ok = match hi {
                None => b1.is_none(),
                Some((x, closed)) => {
                    !lt(&b1, x.0, false) && lt(&b0, x.1, true) && !(*closed && !c1 && eq(&b1, x.0))
                }
            };
            let order = match (lo, hi) {
                (Some((x, _)), Some((y, _))) => x.0 < y.1,
                _ => true,
            };
            if lo_ok && hi_ok && order {
                return Ok(());
            }
            if lo_ok || hi_ok {
                why = "it runs past a turn, an end of the domain or the hole".into();
            }
        }
        Err(why)
    }

    fn judge_monotone(&self, scope: &Scope, value: &Value) -> Verdict {
        let Some(list) = value.as_array() else {
            return Verdict::False("unreadable".into());
        };
        let mut pieces: Vec<(End, End, Option<bool>)> = Vec::new();
        for it in list {
            let on = &it["on"];
            let (Some(lo), Some(hi)) = (end_of(&on["lo"]), end_of(&on["hi"])) else {
                return Verdict::False("unreadable".into());
            };
            let dir = match it["dir"].as_str() {
                Some("Increasing") => Some(true),
                Some("Decreasing") => Some(false),
                _ => None,
            };
            pieces.push((lo, hi, dir));
        }
        let mut extents: Vec<(f64, f64)> = Vec::new();
        for (lo, hi, dir) in &pieces {
            if let Err(why) = self.piece_true(lo, hi, *dir) {
                return Verdict::False(format!("the piece {lo:?} to {hi:?} {dir:?}: {why}"));
            }
            extents.push((
                lo.map_or(f64::NEG_INFINITY, |(x, _)| x.0),
                hi.map_or(f64::INFINITY, |(x, _)| x.1),
            ));
        }
        if !scope.complete() {
            return Verdict::True;
        }
        // Every point of the scope where f is defined lies in a piece:
        // tried at every end and between each two (exactly). (Where that
        // is one point, there is no piece to list: f is monotone on no
        // stretch of it.)
        let (sa, sb) = match scope {
            Scope::Window(a, b) => (*a, *b),
            _ => (f64::NEG_INFINITY, f64::INFINITY),
        };
        let one_point = sa == sb
            || match &self.dom {
                Dom::From(c) => sb.is_finite() && qd(sb) == *c,
                Dom::Upto(c) => sa.is_finite() && qd(sa) == *c,
                _ => false,
            };
        if one_point {
            // (Each piece listed is true: checked above.)
            return Verdict::True;
        }
        let mut pts: Vec<Rational> = Vec::new();
        for (a, b) in extents.iter().chain([(sa, sb)].iter()) {
            for e in [a, b] {
                if e.is_finite() {
                    pts.push(qd(*e));
                }
            }
        }
        if let Some(c) = self.dom.point() {
            pts.push(c.clone());
        }
        pts.sort();
        pts.dedup();
        let mut tries = pts.clone();
        for w in pts.windows(2) {
            tries.push(Rational::from(&w[0] + &w[1]) / 2u32);
        }
        if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
            tries.push(Rational::from(first - 1u32));
            tries.push(Rational::from(last + 1u32));
        } else {
            tries.push(qi(0));
        }
        for t in &tries {
            let in_scope = (!sa.is_finite() || *t >= qd(sa)) && (!sb.is_finite() || *t <= qd(sb));
            if !in_scope || !self.dom.holds_q(t) {
                continue;
            }
            let covered = extents
                .iter()
                .any(|(a, b)| (!a.is_finite() || qd(*a) <= *t) && (!b.is_finite() || *t <= qd(*b)));
            if !covered {
                return Verdict::False(format!(
                    "the pieces leave {:e} of {scope:?} out",
                    t.to_f64()
                ));
            }
        }
        // f's pieces, each end where one ends: a zero of f′, the domain's
        // end or hole, the window's edge; and between two such ends, f
        // monotone the way listed. (Only some point of each enclosure
        // was asked above: two overlapping enclosures of one turn bound
        // a piece of no length.)
        for (lo, hi, dir) in &pieces {
            if let Err(why) = self.piece_anchored(lo, hi, *dir, scope) {
                return Verdict::False(format!("the piece {lo:?} to {hi:?} {dir:?}: {why}"));
            }
        }
        Verdict::True
    }

    /// Where a piece of a complete row may end, within the enclosure x:
    /// a zero of f′ in the domain, the domain's end or hole, an edge of
    /// the window.
    fn anchors_in(&self, x: (f64, f64), scope: &Scope) -> Vec<Place> {
        // (f constant: f′ is 0 everywhere, and a constant piece may be
        // cut anywhere in the domain.)
        if self.constant() {
            let mut out: Vec<Place> = [x.0, x.1]
                .iter()
                .filter(|e| e.is_finite() && self.dom.holds_q(&qd(**e)))
                .map(|e| Place::At(qd(*e)))
                .collect();
            // (And the domain's end or hole, where a piece ends too.)
            if let Some(c) = self.dom.point()
                && x.0.is_finite()
                && x.1.is_finite()
                && qd(x.0) <= *c
                && *c <= qd(x.1)
            {
                out.push(Place::At(c.clone()));
            }
            return out;
        }
        let mut out: Vec<Place> = self.roots[1]
            .iter()
            .map(|r| Place::Root(r.clone()))
            .filter(|p| p.within(x.0, x.1) && self.dom.holds(p))
            .collect();
        if let Some(c) = self.dom.point()
            && qd(x.0) <= *c
            && *c <= qd(x.1)
        {
            out.push(Place::At(c.clone()));
        }
        if let Scope::Window(a, b) = scope {
            for e in [*a, *b] {
                if x.0 <= e && e <= x.1 {
                    out.push(Place::At(qd(e)));
                }
            }
        }
        out
    }

    /// A complete row's piece: from an anchor in its low end's enclosure
    /// to a later one in its high end's, within one stretch of f in its
    /// direction.
    fn piece_anchored(
        &self,
        lo: &End,
        hi: &End,
        dir: Option<bool>,
        scope: &Scope,
    ) -> Result<(), String> {
        let ps: Option<Vec<Place>> = lo.map(|(x, _)| self.anchors_in(x, scope));
        let qs: Option<Vec<Place>> = hi.map(|(x, _)| self.anchors_in(x, scope));
        if ps.as_ref().is_some_and(Vec::is_empty) || qs.as_ref().is_some_and(Vec::is_empty) {
            return Err("an end where no piece of f ends".into());
        }
        let below = |a: &Option<Place>, b: &Place| a.as_ref().is_none_or(|a| pcmp(a, b) != Greater);
        let above = |a: &Option<Place>, b: &Place| a.as_ref().is_none_or(|a| pcmp(a, b) != Less);
        for (b0, c0, b1, c1, d) in self.stretches() {
            if d != dir {
                continue;
            }
            let starts: Vec<Option<Place>> = match &ps {
                None if b0.is_none() => vec![None],
                None => continue,
                Some(ps) => ps
                    .iter()
                    .filter(|p| below(&b0, p) && above(&b1, p))
                    .filter(|p| {
                        // (A closed end at the stretch's open end: no.)
                        !(lo.is_some_and(|(_, closed)| closed)
                            && !c0
                            && b0.as_ref().is_some_and(|b| pcmp(b, p) == Equal))
                    })
                    .map(|p| Some(p.clone()))
                    .collect(),
            };
            let ends: Vec<Option<Place>> = match &qs {
                None if b1.is_none() => vec![None],
                None => continue,
                Some(qs) => qs
                    .iter()
                    .filter(|q| below(&b0, q) && above(&b1, q))
                    .filter(|q| {
                        !(hi.is_some_and(|(_, closed)| closed)
                            && !c1
                            && b1.as_ref().is_some_and(|b| pcmp(b, q) == Equal))
                    })
                    .map(|q| Some(q.clone()))
                    .collect(),
            };
            let ordered = starts.iter().any(|p| {
                ends.iter().any(|q| match (p, q) {
                    (Some(p), Some(q)) => pcmp(p, q) == Less,
                    _ => true,
                })
            });
            if ordered {
                return Ok(());
            }
        }
        Err("no stretch of f between two of its anchors".into())
    }

    fn judge(&self, row: &str, scope: &Scope, value: &Value) -> Verdict {
        match row {
            "monotonicity" => self.judge_monotone(scope, value),
            "range" => self.judge_range(value),
            _ => self.judge_list(row, scope, value),
        }
    }

    /// f's range on its domain, as its components in order, each end
    /// infinite or the least (greatest) of f's values at some places.
    fn range_parts(&self) -> Vec<(REnd, REnd)> {
        let f = &self.d[0];
        let inside_point = || -> Place {
            Place::At(match &self.dom {
                Dom::From(c) => Rational::from(c + 1u32),
                Dom::Upto(c) => Rational::from(c - 1u32),
                Dom::Hole(c) => Rational::from(c + 1u32),
                Dom::Line => qi(0),
            })
        };
        if f.degree() == 0 {
            let v = REnd::Val {
                at: vec![inside_point()],
                attained: true,
            };
            return vec![(v.clone(), v)];
        }
        // Where f may be least or greatest: its critical points inside
        // the domain, a closed end.
        let mut cands: Vec<Place> = self.roots[1]
            .iter()
            .map(|r| Place::Root(r.clone()))
            .filter(|p| self.dom.inside(p))
            .collect();
        if let Dom::From(c) | Dom::Upto(c) = &self.dom {
            cands.push(Place::At(c.clone()));
        }
        // Which way f goes at each infinite end of its domain.
        let lead = f.lead().cmp0() == Greater;
        let odd = f.degree() % 2 == 1;
        let right = lead;
        let left = if odd { !lead } else { lead };
        let (up_unbounded, down_unbounded) = match &self.dom {
            Dom::From(_) => (right, !right),
            Dom::Upto(_) => (left, !left),
            _ => (right || left, !right || !left),
        };
        let lower = if down_unbounded {
            REnd::Inf
        } else {
            REnd::Val {
                at: cands.clone(),
                attained: true,
            }
        };
        let upper = if up_unbounded {
            REnd::Inf
        } else {
            REnd::Val {
                at: cands.clone(),
                attained: true,
            }
        };
        let Dom::Hole(c) = &self.dom else {
            return vec![(lower, upper)];
        };
        // A hole: its value is taken elsewhere, or left out.
        let v = f.eval(c);
        let g = f.sub(&Poly::constant(v.clone()));
        if real_roots(&g).iter().any(|r| r.cmp(c) != Equal) {
            return vec![(lower, upper)];
        }
        let at_c = REnd::Val {
            at: vec![Place::At(c.clone())],
            attained: false,
        };
        let mut all_cands = cands.clone();
        all_cands.push(Place::At(c.clone()));
        // (f's least value on the line is at c just when no other
        // critical value is below f(c).)
        let below = cands.iter().any(|p| p.clone().sign_of(&g) == Less);
        let above = cands.iter().any(|p| p.clone().sign_of(&g) == Greater);
        let lower = match lower {
            REnd::Inf => REnd::Inf,
            _ => REnd::Val {
                at: all_cands.clone(),
                attained: true,
            },
        };
        let upper = match upper {
            REnd::Inf => REnd::Inf,
            _ => REnd::Val {
                at: all_cands,
                attained: true,
            },
        };
        if !matches!(lower, REnd::Inf) && !below {
            // f(c) the least value, taken only at c.
            return vec![(at_c, upper)];
        }
        if !matches!(upper, REnd::Inf) && !above {
            return vec![(lower, at_c)];
        }
        vec![(lower, at_c.clone()), (at_c, upper)]
    }

    /// An end of the range, against a listed bound: the same infinity, or
    /// the value in the listed enclosure and closed just when taken.
    fn end_holds(&self, e: &REnd, b: &End, low: bool) -> Result<(), String> {
        match (e, b) {
            (REnd::Inf, None) => Ok(()),
            (REnd::Inf, Some(_)) => Err("a finite end where f is unbounded".into()),
            (REnd::Val { .. }, None) => Err("an infinite end where f is bounded".into()),
            (REnd::Val { at, attained }, Some((x, closed))) => {
                if closed != attained {
                    return Err(format!(
                        "an end listed {} where f {} it",
                        if *closed { "closed" } else { "open" },
                        if *attained { "takes" } else { "only tends to" }
                    ));
                }
                // The least (greatest) of f at `at` is in [x.0, x.1]: none
                // beyond it on the far side, one within reach.
                let f = &self.d[0];
                let lo = f.sub(&Poly::constant(qd(x.0)));
                let hi = f.sub(&Poly::constant(qd(x.1)));
                let ok = if low {
                    at.iter().all(|p| p.clone().sign_of(&lo) != Less)
                        && at.iter().any(|p| p.clone().sign_of(&hi) != Greater)
                } else {
                    at.iter().all(|p| p.clone().sign_of(&hi) != Greater)
                        && at.iter().any(|p| p.clone().sign_of(&lo) != Less)
                };
                if ok {
                    Ok(())
                } else {
                    Err(format!("the end [{:e}, {:e}] doesn't hold f's", x.0, x.1))
                }
            }
        }
    }

    fn judge_range(&self, value: &Value) -> Verdict {
        let Some(list) = value.as_array() else {
            return Verdict::False("unreadable".into());
        };
        let mut pieces: Vec<(End, End)> = Vec::new();
        for p in list {
            let (Some(lo), Some(hi)) = (end_of(&p["lo"]), end_of(&p["hi"])) else {
                return Verdict::False("unreadable".into());
            };
            pieces.push((lo, hi));
        }
        pieces.sort_by(|a, b| {
            let k = |e: &End| e.map_or(f64::NEG_INFINITY, |(x, _)| x.0);
            k(&a.0).total_cmp(&k(&b.0))
        });
        let parts = self.range_parts();
        if parts.len() != pieces.len() {
            return Verdict::False(format!(
                "{} pieces listed, f's range has {}",
                pieces.len(),
                parts.len()
            ));
        }
        for ((lo, hi), (pl, ph)) in parts.iter().zip(&pieces) {
            if let Err(why) = self.end_holds(lo, pl, true) {
                return Verdict::False(format!("low end: {why}"));
            }
            if let Err(why) = self.end_holds(hi, ph, false) {
                return Verdict::False(format!("high end: {why}"));
            }
        }
        Verdict::True
    }
}

/// An end of f's range: infinite, or the least (greatest) of f's values at
/// the places `at`, taken there or only a limit.
#[derive(Clone, Debug)]
enum REnd {
    Inf,
    Val { at: Vec<Place>, attained: bool },
}

// ------------------------------------------------------------ one function

/// A function with its genuine certificate and the replay's session.
struct Ctx {
    fun: Fun,
    truth: Truth,
    /// The genuine certificate, every row withheld but the domain's.
    base: Value,
    genuine: Value,
    fx: replay::Fx,
    session: replay::Session,
    /// The doubles boxes and windows start and end at.
    atoms: Vec<f64>,
    /// The atoms at and beside the features.
    near: Vec<f64>,
}

const LISTS: [&str; 12] = [
    "domain",
    "x_intercepts",
    "y_intercept",
    "parity",
    "period",
    "extrema",
    "inflections",
    "monotonicity",
    "range",
    "vertical",
    "horizontal",
    "oblique",
];

/// The domain row says what the function was built with.
fn domain_as_built(row: &Value, dom: &Dom) -> bool {
    let Some(v) = row.get("Certified").map(|b| &b["value"]) else {
        return false;
    };
    if v["excluded"].as_array().is_none_or(|e| !e.is_empty()) {
        return false;
    }
    let Some(pieces) = v["pieces"].as_array() else {
        return false;
    };
    let at = |b: &Value, c: &Rational, closed: bool| -> bool {
        end_of(b).is_some_and(|e| {
            e.is_some_and(|(x, cl)| cl == closed && qd(x.0) <= *c && *c <= qd(x.1))
        })
    };
    let inf = |b: &Value, s: &str| b.as_str() == Some(s);
    match (dom, pieces.as_slice()) {
        (Dom::Line, [p]) => inf(&p["lo"], "NegInf") && inf(&p["hi"], "PosInf"),
        (Dom::From(c), [p]) => at(&p["lo"], c, true) && inf(&p["hi"], "PosInf"),
        (Dom::Upto(c), [p]) => inf(&p["lo"], "NegInf") && at(&p["hi"], c, true),
        (Dom::Hole(c), [p, q]) => {
            inf(&p["lo"], "NegInf")
                && at(&p["hi"], c, false)
                && at(&q["lo"], c, false)
                && inf(&q["hi"], "PosInf")
        }
        _ => false,
    }
}

fn context(fun: Fun) -> Result<Ctx, String> {
    let a = certify_text(&fun.text, CompileOptions::default(), DEFAULT_BUDGET, None)
        .map_err(|e| format!("not certified: {e:?}"))?;
    let mut genuine: Value =
        serde_json::from_str(&serde_json::to_string(&a).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if !fun.evaluated.is_empty() {
        let mut parts: Vec<String> = genuine["evaluated"]
            .as_str()
            .map(|s| {
                s.split("; ")
                    .filter(|p| p.starts_with("f = "))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        parts.extend(fun.evaluated.iter().cloned());
        genuine["evaluated"] = json!(parts.join("; "));
    }
    if !domain_as_built(&genuine["domain"], &fun.dom) {
        return Err(format!(
            "the domain row is not as built: {}",
            genuine["domain"]
        ));
    }
    let fx = replay::function(&genuine)?;
    let truth = Truth::new(&fun.f, &fun.dom);
    let mut near: Vec<f64> = Vec::new();
    let mut about = |lo: f64, hi: f64, n: i32| {
        for i in 0..=n {
            near.push(doubles_out(lo, -i));
            near.push(doubles_out(hi, i));
        }
    };
    for k in 0..3 {
        for r in &truth.roots[k] {
            let (lo, hi) = r.clone().doubles();
            about(lo, hi, 2);
        }
    }
    for p in fun.planted.iter().chain(fun.dom.point()) {
        about(floor_double(p), ceil_double(p), 1);
    }
    near.retain(|x| x.is_finite());
    near.sort_by(f64::total_cmp);
    near.dedup();
    let mut atoms = near.clone();
    let (lo, hi) = match (near.first(), near.last()) {
        (Some(a), Some(b)) => (*a, *b),
        _ => (-1.0, 1.0),
    };
    // Past every root of f, f′, f″, f‴ and beyond (Cauchy's bound).
    let mut far = 1.0f64;
    for p in &truth.d {
        if p.degree() >= 1 {
            let lead = p.lead();
            for c in &p.0 {
                far = far.max(1.0 + Rational::from(c / &lead).abs().to_f64());
            }
        }
    }
    atoms.extend([
        lo - 1.0,
        (lo - 8.0).min(-far - 1.0),
        hi + 1.0,
        (hi + 8.0).max(far + 1.0),
    ]);
    let mut gaps = Vec::new();
    for w in near.windows(2) {
        if w[1] - w[0] > 1e-6 * (w[0].abs() + w[1].abs()).max(1e-300) {
            gaps.push(w[0] / 2.0 + w[1] / 2.0);
        }
    }
    atoms.extend(gaps);
    atoms.retain(|x| x.is_finite());
    atoms.sort_by(f64::total_cmp);
    atoms.dedup();
    let mut base = genuine.clone();
    for row in LISTS {
        if row != "domain" {
            base[row] = json!({"Unknown": {"reason": "withheld"}});
        }
    }
    Ok(Ctx {
        fun,
        truth,
        base,
        genuine,
        fx,
        session: replay::Session::default(),
        atoms,
        near,
    })
}

/// One made-up row with its claims.
#[derive(Clone, Debug)]
struct Case {
    row: &'static str,
    scope: Scope,
    value: Value,
    claims: Vec<Value>,
    /// The range's `range_ends` (where each end comes from).
    ends: Value,
}

impl Case {
    fn json(&self, base: &Value) -> Value {
        let mut v = base.clone();
        v[self.row] = json!({self.scope.status(): {"value": self.value.clone(),
            "cert": {"covers": self.scope.covers(), "claims": self.claims.clone()}}});
        if self.row == "range" {
            v["range_ends"] = self.ends.clone();
        }
        v
    }
}

/// p's values over [lo, hi] (rationals), by Horner's rule on intervals: an
/// enclosure.
fn poly_range(p: &Poly, lo: &Rational, hi: &Rational) -> (Rational, Rational) {
    let mut acc = (Rational::new(), Rational::new());
    for c in p.0.iter().rev() {
        let prods = [
            Rational::from(&acc.0 * lo),
            Rational::from(&acc.0 * hi),
            Rational::from(&acc.1 * lo),
            Rational::from(&acc.1 * hi),
        ];
        let mn = prods.iter().min().expect("four").clone();
        let mx = prods.iter().max().expect("four").clone();
        acc = (mn + c, mx + c);
    }
    acc
}

/// f's value at the place, enclosed in doubles.
fn value_enc(f: &Poly, p: &Place) -> (f64, f64) {
    match p {
        Place::At(x) => {
            let v = f.eval(x);
            (floor_double(&v), ceil_double(&v))
        }
        Place::Root(r) => {
            let mut r = r.clone();
            for _ in 0..400 {
                if r.exact().is_some()
                    || Rational::from(&r.hi - &r.lo) < Rational::from((1, Integer::from(1) << 300))
                {
                    break;
                }
                r.bisect();
            }
            if let Some(x) = r.exact() {
                let v = f.eval(x);
                return (floor_double(&v), ceil_double(&v));
            }
            let (lo, hi) = poly_range(f, &r.lo, &r.hi);
            (floor_double(&lo), ceil_double(&hi))
        }
    }
}

impl Ctx {
    /// f's values over [a, b], by the replay's own enclosure (exactly at a
    /// point).
    fn y_of(&self, a: f64, b: f64) -> Option<(f64, f64)> {
        if !self.truth.dom.holds_box(a, b) {
            return None;
        }
        if a == b {
            let v = self.truth.at(0, a);
            return Some((floor_double(&v), ceil_double(&v)));
        }
        replay::iv::set_prec(160);
        let s = self.fx.series(&self.fx.f, a, b, 0);
        let v = &s[0];
        (!v.empty && v.def && v.bounded()).then(|| replay::claims::outward(v))
    }

    fn window(&self, rng: &mut Rng, point: bool) -> Scope {
        let pick = |rng: &mut Rng| {
            if rng.chance(0.75) && !self.near.is_empty() {
                *rng.pick(&self.near)
            } else {
                *rng.pick(&self.atoms)
            }
        };
        let a = pick(rng);
        if point {
            return Scope::Window(a, a);
        }
        let b = pick(rng);
        if a == b {
            Scope::Window(a, doubles_out(a, 1 + rng.below(3) as i32))
        } else {
            Scope::Window(a.min(b), a.max(b))
        }
    }

    /// A case about one place: a zero of f⁽ᵏ⁾ (or the domain's end or
    /// hole) at a double or between two, the window on it or from it, and
    /// a few true claims about the doubles at and beside it, the reals
    /// between them and stretches from them: where a row's inference may
    /// take a fact at a point for one about the reals beside it.
    fn make_local_cases(&self, rng: &mut Rng, variants: usize) -> Vec<Case> {
        let (row, k) = *rng.pick(&[
            ROWS[0], ROWS[1], ROWS[1], ROWS[1], ROWS[2], ROWS[2], ROWS[3],
        ]);
        let mut spots: Vec<f64> = Vec::new();
        for r in &self.truth.roots[k] {
            let (lo, hi) = r.clone().doubles();
            spots.push(lo);
            if hi != lo {
                spots.push(hi);
            }
        }
        if let Some(c) = self.truth.dom.point() {
            spots.push(floor_double(c));
            spots.push(ceil_double(c));
        }
        spots.retain(|x| x.is_finite());
        if spots.is_empty() {
            spots = self.atoms.clone();
        }
        // Mostly one of the row's own features.
        let mut primary: Vec<f64> = Vec::new();
        let kind = if row == "monotonicity" {
            "extrema"
        } else {
            row
        };
        for mut f in self.truth.features(kind) {
            let (lo, hi) = f.at.enc();
            primary.push(lo);
            primary.push(hi);
        }
        primary.retain(|x| x.is_finite());
        // (Above all, one with another zero of f⁽ᵏ⁾ a double or two away:
        // a point fact there is a fact about the reals between.)
        let crowded: Vec<f64> = primary
            .iter()
            .copied()
            .filter(|p| {
                spots
                    .iter()
                    .any(|q| q != p && doubles_out(*p, -2) <= *q && *q <= doubles_out(*p, 2))
            })
            .collect();
        let r = if !crowded.is_empty() && rng.chance(0.5) {
            *rng.pick(&crowded)
        } else if !primary.is_empty() && rng.chance(0.7) {
            *rng.pick(&primary)
        } else {
            *rng.pick(&spots)
        };
        let u = |n: i32| doubles_out(r, n);
        let lefts: Vec<f64> = self.atoms.iter().copied().filter(|a| *a < u(-3)).collect();
        let rights: Vec<f64> = self.atoms.iter().copied().filter(|a| *a > u(3)).collect();
        let left = if lefts.is_empty() || rng.chance(0.2) {
            u(-3)
        } else {
            *rng.pick(&lefts)
        };
        let right = if rights.is_empty() || rng.chance(0.2) {
            u(3)
        } else {
            *rng.pick(&rights)
        };
        let mut g = Gen {
            t: &self.truth,
            k,
            rng,
            atoms: &self.atoms,
            out: Vec::new(),
        };
        // Groups of claims about the place: what decides the reals on each
        // side of it, the place itself and the doubles beside it. Each
        // variant takes a few, chosen at random, so that a shape of a few
        // claims (a point alone, a gap and a point beside it) comes up.
        let groups: [&dyn Fn(&mut Gen); 24] = [
            // (A side decided up to the place, a gap to the next double
            // and a point there; a gap from the place and a stretch past
            // it: each side.)
            &|g| {
                g.stretch(left, r, 30);
                g.gap(r, u(1));
                g.point(u(1));
            },
            &|g| {
                g.stretch(r, right, 30);
                g.gap(u(-1), r);
                g.point(u(-1));
            },
            &|g| {
                g.gap(r, u(1));
                g.stretch(u(1), right, 30);
            },
            &|g| {
                g.gap(u(-1), r);
                g.stretch(left, u(-1), 30);
            },
            &|g| g.crossings(),
            &|g| g.stretch(left, r, 30),
            &|g| g.stretch(r, right, 30),
            &|g| g.stretch(left, u(-1), 30),
            &|g| g.stretch(u(1), right, 30),
            &|g| g.gap(u(-1), r),
            &|g| g.gap(r, u(1)),
            &|g| g.stretch(u(-1), r, 30),
            &|g| g.stretch(r, u(1), 30),
            &|g| g.point(r),
            &|g| {
                if g.k > 0 {
                    g.push(c_box("Kink", r, r));
                } else {
                    g.point(r);
                }
            },
            &|g| g.point(u(-1)),
            &|g| g.point(u(1)),
            &|g| g.push(c_box("Kink", u(-1), u(-1))),
            &|g| g.push(c_box("Kink", u(1), u(1))),
            &|g| g.stretch(u(-1), u(1), 30),
            &|g| g.gap(u(-2), u(-1)),
            &|g| g.gap(u(1), u(2)),
            &|g| g.point(u(-2)),
            &|g| g.point(u(2)),
        ];
        let made: Vec<Vec<Value>> = groups
            .iter()
            .map(|f| {
                f(&mut g);
                std::mem::take(&mut g.out)
            })
            .collect();
        let mut out = Vec::new();
        for _ in 0..variants {
            let scope = match g.rng.below(10) {
                0..=4 if row != "monotonicity" => Scope::Window(r, r),
                0..=5 => Scope::Window(r, right),
                6 | 7 => Scope::Window(left, r),
                8 => Scope::Window(u(-1), u(1)),
                _ => {
                    if g.rng.chance(0.5) {
                        Scope::Line
                    } else {
                        Scope::Points
                    }
                }
            };
            let n = *g.rng.pick(&[1usize, 1, 1, 2, 2, 2, 3, 3, 4, 5, 7]);
            let mut chosen: Vec<usize> = Vec::new();
            while chosen.len() < n {
                let i = g.rng.below(groups.len());
                if !chosen.contains(&i) {
                    chosen.push(i);
                }
            }
            let mut claims: Vec<Value> = chosen.iter().flat_map(|i| made[*i].clone()).collect();
            if g.rng.chance(0.2) {
                match &scope {
                    Scope::Window(a, b) => g.cover(*a, *b),
                    Scope::Line => g.cover(f64::NEG_INFINITY, f64::INFINITY),
                    Scope::Points => {}
                }
            }
            if g.rng.chance(0.15) {
                g.noise();
            }
            claims.append(&mut g.out);
            let mut value = if row == "monotonicity" {
                self.mono_row(g.rng, &scope)
            } else {
                self.list_row(g.rng, row, &scope, &mut claims)
            };
            if g.rng.chance(0.5)
                && let Some(items) = value.as_array_mut()
                && !items.is_empty()
            {
                let i = g.rng.below(items.len());
                items.remove(i);
            }
            let mut seen = std::collections::HashSet::new();
            claims.retain(|c| seen.insert(c.to_string()));
            out.push(Case {
                row,
                scope,
                value,
                claims,
                ends: Value::Null,
            });
        }
        out
    }

    fn make_case(&self, rng: &mut Rng) -> Case {
        let (row, k) = *rng.pick(&[
            ROWS[0], ROWS[1], ROWS[1], ROWS[1], ROWS[2], ROWS[2], ROWS[3],
        ]);
        let scope = match rng.below(10) {
            0..=2 => Scope::Line,
            3..=8 => {
                let point = row != "monotonicity" && rng.chance(0.3);
                self.window(rng, point)
            }
            _ => Scope::Points,
        };
        let mut g = Gen {
            t: &self.truth,
            k,
            rng,
            atoms: &self.atoms,
            out: Vec::new(),
        };
        match &scope {
            Scope::Line => g.cover(f64::NEG_INFINITY, f64::INFINITY),
            Scope::Window(a, b) => {
                let (mut lo, mut hi) = (*a, *b);
                if g.rng.chance(0.3) {
                    lo = *g.rng.pick(&self.atoms);
                    lo = lo.min(*a);
                }
                if g.rng.chance(0.3) {
                    hi = *g.rng.pick(&self.atoms);
                    hi = hi.max(*b);
                }
                g.cover(lo, hi);
            }
            Scope::Points => {
                if g.rng.chance(0.7)
                    && let Scope::Window(a, b) = self.window(g.rng, false)
                {
                    g.cover(a, b);
                }
            }
        }
        if g.rng.chance(0.6) {
            g.noise();
        }
        if g.rng.chance(0.15) {
            g.crossings();
        }
        let mut claims = std::mem::take(&mut g.out);
        let drop = *rng.pick(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.02, 0.05]);
        claims.retain(|_| !rng.chance(drop));
        let value = if row == "monotonicity" {
            self.mono_row(rng, &scope)
        } else {
            self.list_row(rng, row, &scope, &mut claims)
        };
        let mut seen = std::collections::HashSet::new();
        claims.retain(|c| seen.insert(c.to_string()));
        Case {
            row,
            scope,
            value,
            claims,
            ends: Value::Null,
        }
    }

    /// A range row over the line: f′'s walk, the limits at the domain's
    /// infinite ends, and each finite end from the places f may be least
    /// or greatest at (a value claim there, or the hole's limit); then
    /// changed: an end moved a double, closed or opened, two pieces
    /// joined, one dropped.
    fn make_range_case(&self, rng: &mut Rng) -> Case {
        let mut g = Gen {
            t: &self.truth,
            k: 1,
            rng,
            atoms: &self.atoms,
            out: Vec::new(),
        };
        g.cover(f64::NEG_INFINITY, f64::INFINITY);
        if g.rng.chance(0.6) {
            // f′'s sign on every stretch between atoms with no zero of
            // it, in the domain: the walk covered but where it turns.
            for w in self.atoms.windows(2) {
                if self.truth.dom.holds_box(w[0], w[1])
                    && let Some(up) = self.truth.sign_over(1, w[0], w[1])
                {
                    g.push(c_sign("Beyond", 1, w[0], w[1], up));
                }
            }
        }
        if g.rng.chance(0.4) {
            g.noise();
        }
        let mut claims = std::mem::take(&mut g.out);
        // f's limits at the domain's infinite ends (by the tree's
        // structure).
        let f = &self.truth.d[0];
        if f.degree() >= 1 {
            let lead = f.lead().cmp0() == Greater;
            let odd = f.degree() % 2 == 1;
            for (right, up) in [(true, lead), (false, lead != odd)] {
                let reaches = match &self.truth.dom {
                    Dom::From(_) => right,
                    Dom::Upto(_) => !right,
                    _ => true,
                };
                if reaches {
                    claims.push(
                        json!({"Limit": {"at": if right { "PosInf" } else { "NegInf" },
                        "over_x": false, "to": if up { "PosInf" } else { "NegInf" }}}),
                    );
                }
            }
        }
        let mut pieces: Vec<[Value; 2]> = Vec::new();
        let mut sources: Vec<[Value; 2]> = Vec::new();
        for (lo, hi) in self.truth.range_parts() {
            let (bl, sl) = self.range_bound(&lo, true, &mut claims);
            let (bh, sh) = self.range_bound(&hi, false, &mut claims);
            pieces.push([bl, bh]);
            sources.push([sl, sh]);
        }
        let n = pieces.len();
        let finite: Vec<(usize, usize)> = (0..n)
            .flat_map(|i| [(i, 0), (i, 1)])
            .filter(|(i, j)| pieces[*i][*j].get("At").is_some())
            .collect();
        match rng.below(10) {
            4 if !finite.is_empty() => {
                // An end moved a double.
                let (i, j) = *rng.pick(&finite);
                let s = if rng.chance(0.5) { 1 } else { -1 };
                let x = &mut pieces[i][j]["At"]["x"];
                if let Some((lo, hi)) = enc_of(x) {
                    *x = enc(doubles_out(lo, s), doubles_out(hi, s));
                }
            }
            5 if !finite.is_empty() => {
                let (i, j) = *rng.pick(&finite);
                let c = pieces[i][j]["At"]["closed"].as_bool().unwrap_or(true);
                pieces[i][j]["At"]["closed"] = json!(!c);
            }
            6 if n >= 2 => {
                let i = rng.below(n - 1);
                let hi = pieces[i + 1][1].clone();
                let sh = sources[i + 1][1].clone();
                pieces[i][1] = hi;
                sources[i][1] = sh;
                pieces.remove(i + 1);
                sources.remove(i + 1);
            }
            7 if n >= 2 => {
                let i = rng.below(n);
                pieces.remove(i);
                sources.remove(i);
            }
            8 if !finite.is_empty() => {
                // An end narrowed to one of its doubles.
                let (i, j) = *rng.pick(&finite);
                let x = &mut pieces[i][j]["At"]["x"];
                if let Some((lo, hi)) = enc_of(x) {
                    let p = if rng.chance(0.5) { lo } else { hi };
                    *x = enc(p, p);
                }
            }
            _ => {}
        }
        let value = Value::Array(
            pieces
                .iter()
                .map(|[lo, hi]| json!({"lo": lo, "hi": hi}))
                .collect(),
        );
        let ends = Value::Array(sources.iter().map(|[a, b]| json!([a, b])).collect());
        let mut seen = std::collections::HashSet::new();
        claims.retain(|c| seen.insert(c.to_string()));
        Case {
            row: "range",
            scope: Scope::Line,
            value,
            claims,
            ends,
        }
    }

    /// A range end as listed, with its sources (and their claims).
    fn range_bound(&self, e: &REnd, low: bool, claims: &mut Vec<Value>) -> (Value, Value) {
        let REnd::Val { at, attained } = e else {
            return (
                json!(if low { "NegInf" } else { "PosInf" }),
                json!(["Other"]),
            );
        };
        let f = &self.truth.d[0];
        // Each place's enclosure, and f's values there: the replay's own
        // where f is defined over it (a value claim's), else exactly.
        // (Where, f's values there, from a value claim.)
        type Valued = ((f64, f64), (f64, f64), bool);
        let mut vals: Vec<Valued> = Vec::new();
        for p in at {
            let mut p = p.clone();
            let x = p.enc();
            match self.y_of(x.0, x.1).filter(|_| *attained) {
                Some(y) => vals.push((x, y, true)),
                None => vals.push((x, value_enc(f, &p), false)),
            }
        }
        // Those that may be the least (greatest).
        let group: Vec<&Valued> = if low {
            let m = vals.iter().map(|v| v.1.1).fold(f64::INFINITY, f64::min);
            vals.iter().filter(|v| v.1.0 <= m).collect()
        } else {
            let m = vals.iter().map(|v| v.1.0).fold(f64::NEG_INFINITY, f64::max);
            vals.iter().filter(|v| v.1.1 >= m).collect()
        };
        let lo = group.iter().map(|v| v.1.0).fold(f64::INFINITY, f64::min);
        let hi = group
            .iter()
            .map(|v| v.1.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let mut srcs = Vec::new();
        for (x, y, valued) in group {
            if *attained && *valued {
                claims.push(c_value(0, x.0, x.1, y.0, y.1));
                srcs.push(json!({"At": enc(x.0, x.1)}));
            } else if !*attained {
                let w = 1e-6 * x.0.abs().max(1.0);
                claims.push(json!({"Removable": {"near": bx(x.0 - w, x.1 + w),
                    "at": bx(x.0, x.1), "lo": num(y.0), "hi": num(y.1)}}));
                srcs.push(json!({"Hole": enc(x.0, x.1)}));
            } else {
                srcs.push(json!("Other"));
            }
        }
        (
            json!({"At": {"x": enc(lo, hi), "closed": attained}}),
            Value::Array(srcs),
        )
    }

    fn list_row(&self, rng: &mut Rng, row: &str, scope: &Scope, claims: &mut Vec<Value>) -> Value {
        let mut feats = self.truth.features(row);
        let mut chosen: Vec<((f64, f64), Option<bool>)> = Vec::new();
        // The boxes the claims place zeros in (each item may be listed as
        // one of those holding its feature, or as two).
        let zero_boxes: Vec<(f64, f64)> = claims
            .iter()
            .filter_map(|c| {
                let (kind, b) = c.as_object()?.iter().next()?;
                if !["OneCross", "ExactAt", "Touch", "KinkSides", "Gap"].contains(&kind.as_str()) {
                    return None;
                }
                let x = b.get("x")?;
                Some((replay::r(x.get("a")?).ok()?, replay::r(x.get("b")?).ok()?))
            })
            .collect();
        let as_boxes = rng.chance(0.3);
        for f in &mut feats {
            let take = match scope {
                Scope::Points => rng.chance(0.6),
                _ => scope.holds(&f.at) || rng.chance(0.15),
            };
            if !take {
                continue;
            }
            let holding: Vec<(f64, f64)> = zero_boxes
                .iter()
                .copied()
                .filter(|b| b.0.is_finite() && b.1.is_finite() && f.at.within(b.0, b.1))
                .collect();
            if as_boxes && !holding.is_empty() {
                chosen.push((*rng.pick(&holding), f.max));
                if holding.len() > 1 && rng.chance(0.5) {
                    chosen.push((*rng.pick(&holding), f.max));
                }
            } else {
                chosen.push((f.at.enc(), f.max));
            }
        }
        let ulp = |x: f64, n: i32| doubles_out(x, n);
        match rng.below(10) {
            3 if !chosen.is_empty() => {
                let i = rng.below(chosen.len());
                chosen.remove(i);
            }
            4 => {
                let x = *rng.pick(if self.near.is_empty() {
                    &self.atoms
                } else {
                    &self.near
                });
                chosen.push(((x, x), Some(rng.chance(0.5))));
            }
            5 if !chosen.is_empty() => {
                let i = rng.below(chosen.len());
                let n = if rng.chance(0.5) { 1 } else { -1 };
                chosen[i].0 = (ulp(chosen[i].0.0, n), ulp(chosen[i].0.1, n));
            }
            6 if chosen.len() >= 2 => {
                chosen.sort_by(|a, b| a.0.0.total_cmp(&b.0.0));
                let i = rng.below(chosen.len() - 1);
                let (a, b) = (chosen[i], chosen[i + 1]);
                chosen[i] = ((a.0.0.min(b.0.0), a.0.1.max(b.0.1)), a.1);
                chosen.remove(i + 1);
            }
            7 if !chosen.is_empty() => {
                let i = rng.below(chosen.len());
                chosen[i].1 = chosen[i].1.map(|m| !m);
            }
            8 if !chosen.is_empty() => {
                let i = rng.below(chosen.len());
                let n = 1 + rng.below(3) as i32;
                chosen[i].0 = (ulp(chosen[i].0.0, -n), ulp(chosen[i].0.1, n));
            }
            _ => {}
        }
        chosen.sort_by(|a, b| a.0.0.total_cmp(&b.0.0));
        let items: Vec<Value> = chosen
            .iter()
            .map(|&((lo, hi), max)| {
                if row == "x_intercepts" {
                    return json!({"At": enc(lo, hi)});
                }
                let y = self.y_of(lo, hi);
                if let Some((ylo, yhi)) = y {
                    claims.push(c_value(0, lo, hi, ylo, yhi));
                }
                let (ylo, yhi) = y.unwrap_or((0.0, 0.0));
                let mut item = json!({"x": enc(lo, hi), "y": enc(ylo, yhi), "every": null});
                if row == "extrema" {
                    item["kind"] = json!(if max.unwrap_or(false) { "Max" } else { "Min" });
                }
                item
            })
            .collect();
        Value::Array(items)
    }

    fn mono_row(&self, rng: &mut Rng, scope: &Scope) -> Value {
        // The pieces' ends in order: (enclosure, closed); None: infinite.
        let mut ends: Vec<End> = Vec::new();
        let (sa, sb) = match scope {
            Scope::Window(a, b) => (*a, *b),
            Scope::Line => (f64::NEG_INFINITY, f64::INFINITY),
            Scope::Points => {
                let Scope::Window(a, b) = self.window(rng, false) else {
                    unreachable!("a window")
                };
                (a, b)
            }
        };
        let dom = &self.truth.dom;
        let start: End = match dom {
            Dom::From(c) if !sa.is_finite() || qd(sa) < *c => {
                Some(((floor_double(c), ceil_double(c)), true))
            }
            _ if sa.is_finite() => Some(((sa, sa), true)),
            _ => None,
        };
        let end: End = match dom {
            Dom::Upto(c) if !sb.is_finite() || qd(sb) > *c => {
                Some(((floor_double(c), ceil_double(c)), true))
            }
            _ if sb.is_finite() => Some(((sb, sb), true)),
            _ => None,
        };
        let lo_x = start.map_or(f64::NEG_INFINITY, |(x, _)| x.1);
        let hi_x = end.map_or(f64::INFINITY, |(x, _)| x.0);
        ends.push(start);
        for b in &self.truth.breaks {
            let mut b = b.clone();
            let x = b.enc();
            if x.0 > lo_x && x.1 < hi_x {
                let hole = matches!(dom, Dom::Hole(c) if b.cmp(c) == Equal);
                ends.push(Some((x, !hole)));
            }
        }
        ends.push(end);
        // The direction between two ends.
        let dir = |a: &End, b: &End| -> &'static str {
            if self.truth.constant() {
                return "Constant";
            }
            let m = match (a, b) {
                (Some((x, _)), Some((y, _))) => Rational::from(&qd(x.1) + &qd(y.0)) / 2u32,
                (Some((x, _)), None) => Rational::from(&qd(x.1) + 1u32),
                (None, Some((y, _))) => Rational::from(&qd(y.0) - 1u32),
                (None, None) => qi(0),
            };
            let d1 = &self.truth.d[1];
            let s = if d1.sign(&m) == Equal {
                sign_beside(d1, &m, true)
            } else {
                d1.sign(&m)
            };
            if s == Greater {
                "Increasing"
            } else {
                "Decreasing"
            }
        };
        let mut pieces: Vec<(End, End, &'static str)> = ends
            .windows(2)
            .map(|w| (w[0], w[1], dir(&w[0], &w[1])))
            .collect();
        let flip = |d: &str| match d {
            "Increasing" => "Decreasing",
            "Decreasing" => "Increasing",
            _ => "Increasing",
        };
        let n = pieces.len();
        match rng.below(10) {
            4 if n > 0 => {
                let i = rng.below(n);
                pieces[i].2 = flip(pieces[i].2);
            }
            5 if n >= 2 => {
                let i = rng.below(n - 1);
                pieces[i].1 = pieces[i + 1].1;
                pieces.remove(i + 1);
            }
            6 if n > 0 => {
                // Cut at an atom inside.
                let i = rng.below(n);
                let (a, b, d) = pieces[i];
                let lo = a.map_or(f64::NEG_INFINITY, |(x, _)| x.1);
                let hi = b.map_or(f64::INFINITY, |(x, _)| x.0);
                let inside: Vec<f64> = self
                    .atoms
                    .iter()
                    .copied()
                    .filter(|g| lo < *g && *g < hi)
                    .collect();
                if !inside.is_empty() {
                    let g = *rng.pick(&inside);
                    pieces[i] = (a, Some(((g, g), true)), d);
                    pieces.insert(i + 1, (Some(((g, g), true)), b, d));
                }
            }
            7 if n > 0 => {
                // An end moved a double.
                let i = rng.below(n);
                let s = if rng.chance(0.5) { 1 } else { -1 };
                let moved =
                    |e: End| e.map(|((x, y), c)| ((doubles_out(x, s), doubles_out(y, s)), c));
                if rng.chance(0.5) {
                    pieces[i].0 = moved(pieces[i].0);
                } else {
                    pieces[i].1 = moved(pieces[i].1);
                }
            }
            8 if n > 1 => {
                let i = rng.below(n);
                pieces.remove(i);
            }
            9 if n > 0 => {
                let i = rng.below(n);
                let toggle = |e: End| e.map(|(x, c)| (x, !c));
                if rng.chance(0.5) {
                    pieces[i].0 = toggle(pieces[i].0);
                } else {
                    pieces[i].1 = toggle(pieces[i].1);
                }
            }
            _ => {}
        }
        let bound = |e: &End, low: bool| -> Value {
            match e {
                None => json!(if low { "NegInf" } else { "PosInf" }),
                Some((x, closed)) => json!({"At": {"x": enc(x.0, x.1), "closed": closed}}),
            }
        };
        Value::Array(
            pieces
                .iter()
                .map(|(a, b, d)| json!({"on": {"lo": bound(a, true), "hi": bound(b, false)}, "dir": d}))
                .collect(),
        )
    }

    /// The replay's report on the case's row, its premises cut to those it
    /// proves (strong or weak); None if it can't read it.
    fn replay(
        &mut self,
        case: &mut Case,
        dropped: &mut BTreeMap<String, u64>,
    ) -> Option<replay::Report> {
        for _ in 0..3 {
            let v = case.json(&self.base);
            let rep = replay::replay_in(&v, &mut self.session, Some(case.row)).ok()?;
            let open: std::collections::HashSet<String> = rep
                .claims
                .iter()
                .filter(|c| !matches!(c.outcome.class, Class::Strong | Class::Weak))
                .map(|c| {
                    *dropped
                        .entry(format!("{} {}", c.claim.kind(), c.outcome.class.name()))
                        .or_default() += 1;
                    replay::claim_key(&c.claim)
                })
                .collect();
            if open.is_empty() {
                return Some(rep);
            }
            case.claims.retain(|c| {
                replay::claim(c).is_ok_and(|cl| !open.contains(&replay::claim_key(&cl)))
            });
        }
        None
    }

    /// Whether the replay accepts the case's row from premises it proves
    /// (all of them), and the truth refuses it.
    fn accepted_false(&mut self, case: &Case) -> bool {
        let v = case.json(&self.base);
        let Ok(rep) = replay::replay_in(&v, &mut self.session, Some(case.row)) else {
            return false;
        };
        rep.claims
            .iter()
            .all(|c| matches!(c.outcome.class, Class::Strong | Class::Weak))
            && rep.rows.iter().all(|r| r.problems.is_empty())
            && matches!(
                self.truth.judge(case.row, &case.scope, &case.value),
                Verdict::False(_)
            )
    }

    /// The case cut down: claims, then items, dropped while the replay
    /// still accepts the false row.
    fn minimise(&mut self, case: &Case) -> Case {
        let mut c = case.clone();
        let mut i = 0;
        while i < c.claims.len() {
            let mut t = c.clone();
            t.claims.remove(i);
            if self.accepted_false(&t) {
                c = t;
            } else {
                i += 1;
            }
        }
        let mut i = 0;
        while c.value.as_array().is_some_and(|a| i < a.len()) {
            let mut t = c.clone();
            t.value.as_array_mut().expect("a list").remove(i);
            if self.accepted_false(&t) {
                c = t;
            } else {
                i += 1;
            }
        }
        c
    }

    fn show(&self, case: &Case, why: &str) -> String {
        format!(
            "y={}{}\n  {} {} over {}: {}\n  value: {}{}\n  claims:\n    {}\n  truth: {why}",
            self.fun.text,
            if self.fun.evaluated.is_empty() {
                String::new()
            } else {
                format!("  [{}]", self.fun.evaluated.join("; "))
            },
            case.row,
            case.scope.status(),
            case.scope.covers(),
            if case.scope.complete() {
                "complete"
            } else {
                "points"
            },
            case.value,
            if case.row == "range" {
                format!("\n  range_ends: {}", case.ends)
            } else {
                String::new()
            },
            case.claims
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n    "),
        )
    }
}

// ------------------------------------------------------------ the run

#[derive(Default)]
struct Tally {
    functions: u64,
    not_made: u64,
    not_certified: u64,
    domain_not_as_built: Vec<String>,
    cases: u64,
    unreadable: u64,
    /// (accepted, true) by row.
    by_row: BTreeMap<String, [u64; 4]>,
    kept: BTreeMap<&'static str, u64>,
    dropped: BTreeMap<String, u64>,
    false_premises: Vec<String>,
    /// Accepted false rows (minimised).
    failures: Vec<String>,
    /// All premises strong among those.
    failures_strong: u64,
    /// True rows the replay refuses: by its reason, and a few in full.
    refusals: BTreeMap<String, u64>,
    refused_true: Vec<String>,
    genuine_rows: u64,
    /// Genuine rows the truth refuses (the certifier's errors, or the
    /// truth's: each fails the run), and those of them the replay accepts
    /// too (the replay's as well).
    genuine_false: Vec<String>,
    genuine_false_accepted: Vec<String>,
    /// True genuine rows the replay refuses (counted, not failed: the
    /// replay may not see what the claims don't show).
    genuine_refused_true: Vec<String>,
}

impl Tally {
    fn absorb(&mut self, o: Tally) {
        self.functions += o.functions;
        self.not_made += o.not_made;
        self.not_certified += o.not_certified;
        self.domain_not_as_built.extend(o.domain_not_as_built);
        self.cases += o.cases;
        self.unreadable += o.unreadable;
        for (k, v) in o.by_row {
            let e = self.by_row.entry(k).or_default();
            for i in 0..4 {
                e[i] += v[i];
            }
        }
        for (k, v) in o.kept {
            *self.kept.entry(k).or_default() += v;
        }
        for (k, v) in o.dropped {
            *self.dropped.entry(k).or_default() += v;
        }
        self.false_premises.extend(o.false_premises);
        self.failures.extend(o.failures);
        self.failures_strong += o.failures_strong;
        for (k, v) in o.refusals {
            *self.refusals.entry(k).or_default() += v;
        }
        if self.refused_true.len() < 50 {
            self.refused_true.extend(o.refused_true);
        }
        self.genuine_rows += o.genuine_rows;
        self.genuine_false.extend(o.genuine_false);
        self.genuine_false_accepted.extend(o.genuine_false_accepted);
        self.genuine_refused_true.extend(o.genuine_refused_true);
    }

    fn print(&self, seed: u64) {
        println!(
            "seed {seed}: {} functions ({} not made, {} not certified, {} with another domain), \
             {} cases ({} unreadable)",
            self.functions,
            self.not_made,
            self.not_certified,
            self.domain_not_as_built.len(),
            self.cases,
            self.unreadable
        );
        println!("  row: accepted true / accepted FALSE / refused true / refused false");
        for (row, v) in &self.by_row {
            println!("  {row:>13}: {} / {} / {} / {}", v[0], v[1], v[2], v[3]);
        }
        println!(
            "  premises kept: {:?}; dropped: {:?}",
            self.kept, self.dropped
        );
        println!(
            "  accepted false rows: {} ({} from strong premises only); premises proved but \
             false: {}",
            self.failures.len(),
            self.failures_strong,
            self.false_premises.len()
        );
        println!(
            "  genuine certificates: {} rows, {} refused by the truth ({} of them accepted by the \
             replay), {} true ones refused by the replay",
            self.genuine_rows,
            self.genuine_false.len(),
            self.genuine_false_accepted.len(),
            self.genuine_refused_true.len()
        );
        let show = env_u64("REPLAY_FUZZ_SHOW").unwrap_or(3) as usize;
        if env_u64("REPLAY_FUZZ_REFUSALS").is_some() {
            let mut r: Vec<_> = self.refusals.iter().collect();
            r.sort_by(|a, b| b.1.cmp(a.1));
            for (why, n) in r.iter().take(40) {
                println!("  refused true {n}: {why}");
            }
            for s in self
                .refused_true
                .iter()
                .take(env_u64("REPLAY_FUZZ_REFUSALS").unwrap_or(0) as usize)
            {
                println!("REFUSED TRUE: {s}");
            }
        }
        for (what, list) in [
            ("ACCEPTED FALSE", &self.failures),
            ("PROVED FALSE PREMISE", &self.false_premises),
            ("GENUINE ROW THE TRUTH REFUSES", &self.genuine_false),
            (
                "true genuine row the replay refuses",
                &self.genuine_refused_true,
            ),
            ("domain not as built", &self.domain_not_as_built),
        ] {
            for s in list.iter().take(show) {
                println!("{what}: {s}");
            }
        }
    }
}

/// The genuine certificate's rows, held to the truth and replayed.
fn genuine(ctx: &mut Ctx, t: &mut Tally) {
    for row in [
        "x_intercepts",
        "extrema",
        "inflections",
        "monotonicity",
        "range",
    ] {
        let Some((status, body)) = ctx.genuine[row]
            .as_object()
            .and_then(|o| o.iter().next())
            .map(|(s, b)| (s.clone(), b.clone()))
        else {
            continue;
        };
        if status == "Unknown" {
            continue;
        }
        let covers = &body["cert"]["covers"];
        let scope = match covers.as_str() {
            Some("Line") => Scope::Line,
            Some("Points") => Scope::Points,
            _ => match covers.get("Window") {
                Some(w) => Scope::Window(
                    replay::r(&w["a"]).unwrap_or(f64::NAN),
                    replay::r(&w["b"]).unwrap_or(f64::NAN),
                ),
                None => continue,
            },
        };
        t.genuine_rows += 1;
        let rep = replay::replay_in(&ctx.genuine, &mut ctx.session, Some(row));
        let refused = match &rep {
            Err(e) => Some(e.clone()),
            Ok(r) => r
                .claims
                .iter()
                .find(|c| !matches!(c.outcome.class, Class::Strong | Class::Weak))
                .map(|c| format!("{} {:?}", c.outcome.class.name(), c.claim))
                .or_else(|| r.row_problems().into_iter().next()),
        };
        // A genuine row the truth refuses is the certifier's error (or the
        // truth's), the panel showing it, whether the replay refuses it or
        // not; the replay accepting it too is the replay's as well. A true
        // one the replay refuses is only counted.
        match ctx.truth.judge(row, &scope, &body["value"]) {
            Verdict::False(why) => {
                let text = format!(
                    "y={} {row} {status} over {covers}: {}: {why}; the replay {}",
                    ctx.fun.text,
                    body["value"],
                    refused
                        .as_ref()
                        .map_or("accepts it".to_string(), |w| format!("refuses it: {w}"))
                );
                if refused.is_none() {
                    t.genuine_false_accepted.push(text.clone());
                }
                t.genuine_false.push(text);
            }
            Verdict::True => {
                if let Some(why) = refused {
                    t.genuine_refused_true
                        .push(format!("y={} {row}: {why}", ctx.fun.text));
                }
            }
        }
    }
}

fn run_function(seed: u64, i: u64, cases: u64) -> Tally {
    let mut t = Tally {
        functions: 1,
        ..Tally::default()
    };
    let mut rng = Rng::new(seed, i, 0);
    let Some(fun) = (0..20).find_map(|_| make_fun(&mut rng)) else {
        t.not_made += 1;
        return t;
    };
    let mut ctx = match context(fun.clone()) {
        Ok(c) => c,
        Err(e) => {
            if e.starts_with("the domain row") {
                t.domain_not_as_built.push(format!("y={}: {e}", fun.text));
            } else {
                t.not_certified += 1;
            }
            return t;
        }
    };
    genuine(&mut ctx, &mut t);
    // `REPLAY_FUZZ_TRACE=i:j`: that case's claims as made, each with the
    // replay's outcome.
    let trace = std::env::var("REPLAY_FUZZ_TRACE").ok().and_then(|s| {
        let (a, b) = s.split_once(':')?;
        Some((a.parse::<u64>().ok()?, b.parse::<u64>().ok()?))
    });
    for j in 0..cases {
        if trace.is_some_and(|(a, b)| (a, b) != (i, j)) {
            continue;
        }
        let mut rng = Rng::new(seed, i, j + 1);
        let made = match rng.below(100) {
            0..=49 => ctx.make_local_cases(&mut rng, 6),
            50..=61 => vec![ctx.make_range_case(&mut rng)],
            _ => vec![ctx.make_case(&mut rng)],
        };
        for (v, mut case) in made.into_iter().enumerate() {
            let j = format!("{j}.{v}");
            if trace.is_some() {
                let v = case.json(&ctx.base);
                let rep = replay::replay_in(&v, &mut ctx.session, Some(case.row));
                println!("TRACE {}", ctx.show(&case, "(as made)"));
                for c in rep.iter().flat_map(|r| r.claims.iter()) {
                    println!(
                        "  {} {:?} {}",
                        c.outcome.class.name(),
                        c.claim,
                        c.outcome.note
                    );
                }
            }
            let Some(rep) = ctx.replay(&mut case, &mut t.dropped) else {
                t.unreadable += 1;
                continue;
            };
            t.cases += 1;
            for c in &rep.claims {
                let class = c.outcome.class;
                *t.kept.entry(class.name()).or_default() += 1;
                if ctx.truth.claim_true(&c.claim) == Some(false) {
                    t.false_premises.push(format!(
                        "seed {seed} function {i} case {j}: y={} [{}]: {} {:?} ({})",
                        ctx.fun.text,
                        ctx.fun.evaluated.join("; "),
                        class.name(),
                        c.claim,
                        c.outcome.note
                    ));
                }
            }
            let accepted = rep.rows.iter().all(|r| r.problems.is_empty());
            let verdict = ctx.truth.judge(case.row, &case.scope, &case.value);
            let slot = match (&verdict, accepted) {
                (Verdict::True, true) => 0,
                (Verdict::False(_), true) => 1,
                (Verdict::True, false) => 2,
                (Verdict::False(_), false) => 3,
            };
            t.by_row.entry(case.row.to_string()).or_default()[slot] += 1;
            if slot == 2 {
                let why = rep
                    .rows
                    .iter()
                    .flat_map(|r| r.problems.iter())
                    .next()
                    .cloned()
                    .unwrap_or_default();
                // (By the problem's words, numbers aside.)
                let key: String = why
                    .split_whitespace()
                    .filter(|w| !w.chars().any(|c| c.is_ascii_digit()))
                    .collect::<Vec<_>>()
                    .join(" ");
                *t.refusals
                    .entry(format!("{}: {key}", case.row))
                    .or_default() += 1;
                let wanted =
                    std::env::var("REPLAY_FUZZ_REFUSALS_MATCH").map_or(true, |m| why.contains(&m));
                if wanted && t.refused_true.len() < 50 {
                    t.refused_true.push(format!(
                        "function {i} case {j}: {}\n  replay: {why}",
                        ctx.show(&case, "true")
                    ));
                }
            }
            match verdict {
                Verdict::False(why) if accepted => {
                    let strong = rep.claims.iter().all(|c| c.outcome.class == Class::Strong);
                    if strong {
                        t.failures_strong += 1;
                    }
                    let small = ctx.minimise(&case);
                    let why = match ctx.truth.judge(small.row, &small.scope, &small.value) {
                        Verdict::False(w) => w,
                        _ => why,
                    };
                    t.failures.push(format!(
                        "seed {seed} function {i} case {j}{}:\n{}",
                        if strong { " (all premises strong)" } else { "" },
                        ctx.show(&small, &why)
                    ));
                }
                _ => {}
            }
        }
    }
    t
}

fn run(seed: u64, functions: u64, cases: u64, threads: u64) -> Tally {
    let total = Mutex::new(Tally::default());
    let threads = threads.max(1);
    std::thread::scope(|s| {
        for w in 0..threads {
            let total = &total;
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(s, move || {
                    replay::iv::init();
                    let mut i = w;
                    while i < functions {
                        let t = run_function(seed, i, cases);
                        total.lock().unwrap().absorb(t);
                        i += threads;
                    }
                })
                .expect("a fuzzing thread");
        }
    });
    let mut t = total.into_inner().unwrap();
    t.failures.sort();
    t.false_premises.sort();
    t
}

fn env_u64(name: &str) -> Option<u64> {
    let v = std::env::var(name).ok()?;
    Some(
        v.trim()
            .parse()
            .unwrap_or_else(|_| panic!("{name}={v} is not a number")),
    )
}

fn threads() -> u64 {
    env_u64("REPLAY_FUZZ_THREADS")
        .or_else(|| env_u64("RUST_TEST_THREADS"))
        .unwrap_or(2)
        .clamp(1, 16)
}

/// No false row accepted, no false premise proved, no genuine row the
/// truth refuses. A genuine row the truth refuses is a false row on the
/// panel whether the replay refuses it or not: the panel shows the
/// certifier's rows, and the replay is no gate before it. (Only one the
/// replay accepted too failed the run: the certifier's turn beside a
/// hole, four functions in two long runs, was printed and counted, and
/// the run passed.) A true genuine row the replay refuses is counted, not
/// failed.
fn assert_clean(t: &Tally) {
    assert!(
        t.failures.is_empty() && t.false_premises.is_empty() && t.genuine_false.is_empty(),
        "{} accepted false rows, {} proved false premises, {} genuine rows the truth refuses \
         ({} of them accepted by the replay); the first: {}",
        t.failures.len(),
        t.false_premises.len(),
        t.genuine_false.len(),
        t.genuine_false_accepted.len(),
        t.failures
            .first()
            .or(t.false_premises.first())
            .or(t.genuine_false.first())
            .map_or("", String::as_str)
    );
}

const SHORT_SEED: u64 = 20_261_024;
const SHORT_FUNCTIONS: u64 = 24;
const CASES: u64 = 24;

/// The short run: a fixed seed, seconds.
#[test]
fn short_run() {
    let t = run(SHORT_SEED, SHORT_FUNCTIONS, CASES, threads());
    t.print(SHORT_SEED);
    assert_clean(&t);
}

/// The long run: `REPLAY_FUZZ_SEED`, `REPLAY_FUZZ_FUNCTIONS`,
/// `REPLAY_FUZZ_CASES`, `REPLAY_FUZZ_THREADS` (release).
#[test]
#[ignore = "the long run: minutes"]
fn long_run() {
    let seed = env_u64("REPLAY_FUZZ_SEED").unwrap_or(SHORT_SEED);
    let functions = env_u64("REPLAY_FUZZ_FUNCTIONS").unwrap_or(2000);
    let cases = env_u64("REPLAY_FUZZ_CASES").unwrap_or(CASES);
    let started = std::time::Instant::now();
    let t = run(seed, functions, cases, threads());
    t.print(seed);
    println!("{:.1} s", started.elapsed().as_secs_f64());
    assert_clean(&t);
}

/// The truth's own arithmetic against what was planted: Sturm counts each
/// distinct root once, wherever it is (a double, the next double, a point
/// between two doubles); the roots where p changes sign are those of odd
/// multiplicity (Yun's square-free parts, an independent route); and the
/// doubles about a root hold it exactly.
#[test]
fn the_truth_knows_its_polynomials() {
    let mut rng = Rng::new(7, 0, 0);
    let one = qi(1);
    let ulp = qd(2f64.powi(-52));
    let half = Rational::from(&ulp / 2u32);
    let two = Rational::from(&ulp * 2u32);
    let palette = [
        one.clone(),
        Rational::from(&one + &ulp),
        Rational::from(&one - &half),
        Rational::from(&one + &half),
        Rational::from(&one + &two),
        Rational::from((1, 3)),
        qi(-2),
        qi(0),
        qd(f64::from_bits(1)),
    ];
    for _ in 0..400 {
        let mut planted: Vec<(Rational, u32)> = Vec::new();
        for _ in 0..1 + rng.below(4) {
            let r = rng.pick(&palette).clone();
            if planted.iter().all(|(q, _)| *q != r) {
                planted.push((r, 1 + rng.below(4) as u32));
            }
        }
        let p = planted.iter().fold(Poly::constant(qi(-3)), |acc, (r, m)| {
            acc.mul(&Poly::x_minus(r).pow(*m))
        });
        let roots = real_roots(&p);
        assert_eq!(roots.len(), planted.len(), "{planted:?}");
        let odd = real_roots(&odd_part(&p));
        for (r, m) in &planted {
            let mut found = roots
                .iter()
                .find(|x| x.cmp(r) == Equal)
                .cloned()
                .unwrap_or_else(|| panic!("{r} not found among {roots:?}"));
            let (l, rr) = sides(&p, &found);
            assert_eq!(l != rr, m % 2 == 1, "{r}^{m}: sides {l:?} {rr:?}");
            assert_eq!(odd.iter().any(|x| x.cmp(r) == Equal), m % 2 == 1, "{r}^{m}");
            assert_eq!(found.doubles(), (floor_double(r), ceil_double(r)), "{r}");
            // The sign of a polynomial at the root: exactly 0 for p's
            // factors, and that of a constant shift otherwise.
            assert_eq!(found.sign_of(&Poly::x_minus(r)), Equal);
            assert_eq!(
                found.sign_of(&Poly::x_minus(&Rational::from(r - 1u32))),
                Greater
            );
        }
    }
}

/// The reviewers' rows, built by hand (review 22, R22-M-01; review 23,
/// R23-M-01 and R23-M-02): the truth refuses each, and the replay now
/// does too, every premise proved.
#[test]
fn the_reviewers_rows_are_false_and_refused() {
    replay::iv::init();
    let d = 2f64.powi(-52);
    let cubic = |t: &str, dom: Dom| {
        // f = 3δu² − 2u³ (u = x − 1): f′ = 6u(δ − u), a minimum at 1, f′
        // zero again at the next double.
        let du = qd(d);
        let f = Poly::new(vec![qi(0), qi(0), Rational::from(&du * 3u32), qi(-2)]).at_shift(&qi(1));
        Fun {
            text: t.into(),
            evaluated: vec!["f′ = Mul(Mul(6,Sub(x,1)),Sub(Pow(2,Neg(52)),Sub(x,1)))".into()],
            f,
            dom,
            planted: vec![qi(1)],
            expanded: Vec::new(),
        }
    };
    let kink = |a: f64| json!({"Kink": {"x": bx(a, a)}});
    let cases: Vec<(Fun, &'static str, Scope, Vec<Value>)> = vec![
        // R22-M-01: x² − 1, "no extrema for 0 ≤ x ≤ 2", a gap from 0.
        (
            Fun {
                text: "x^2-1".into(),
                evaluated: Vec::new(),
                f: Poly::new(vec![qi(-1), qi(0), qi(1)]),
                dom: Dom::Line,
                planted: vec![qi(0)],
                expanded: Vec::new(),
            },
            "extrema",
            Scope::Window(0.0, 2.0),
            vec![
                c_box("Gap", 0.0, f64::from_bits(1)),
                c_clear(1, 0.0, f64::from_bits(1)),
                c_sign("Beyond", 1, f64::from_bits(1), 2.0, true),
            ],
        ),
        // R23-M-01: a flat point at the next double.
        (
            cubic("3*2^(-52)*(x-1)^2-2*(x-1)^3", Dom::Line),
            "extrema",
            Scope::Window(1.0, 1.0),
            vec![
                c_exact(1, 1.0 - d / 2.0, 1.0, 1.0),
                c_box("Gap", 1.0, 1.0 + d),
                c_clear(1, 1.0, 1.0 + d),
                kink(1.0 + d),
            ],
        ),
        // ... and at a closed domain end.
        (
            cubic("3*2^(-52)*(x-1)^2-2*(x-1)^3+0*sqrt(x-1)", Dom::From(qi(1))),
            "extrema",
            Scope::Window(1.0, 1.0),
            vec![
                c_box("Defined", 1.0, 1.0),
                c_box("Gap", 1.0, 1.0 + d),
                c_clear(1, 1.0, 1.0 + d),
                kink(1.0 + d),
            ],
        ),
        // R23-M-02: a continuity point at the turn, alone.
        (
            Fun {
                text: "(x-1)^2".into(),
                evaluated: Vec::new(),
                f: Poly::x_minus(&qi(1)).pow(2),
                dom: Dom::Line,
                planted: vec![qi(1)],
                expanded: Vec::new(),
            },
            "extrema",
            Scope::Window(1.0, 1.0),
            vec![kink(1.0)],
        ),
        (
            Fun {
                text: "(x-1)^3".into(),
                evaluated: Vec::new(),
                f: Poly::x_minus(&qi(1)).pow(3),
                dom: Dom::Line,
                planted: vec![qi(1)],
                expanded: Vec::new(),
            },
            "inflections",
            Scope::Window(1.0, 1.0),
            vec![kink(1.0)],
        ),
    ];
    for (fun, row, scope, claims) in cases {
        let text = fun.text.clone();
        let mut ctx = context(fun).unwrap_or_else(|e| panic!("{text}: {e}"));
        let case = Case {
            row,
            scope,
            value: json!([]),
            claims,
            ends: Value::Null,
        };
        assert!(
            matches!(
                ctx.truth.judge(row, &case.scope, &case.value),
                Verdict::False(_)
            ),
            "{text}: the truth takes an empty {row} row over {:?} for true",
            case.scope
        );
        let v = case.json(&ctx.base);
        let rep = replay::replay_in(&v, &mut ctx.session, Some(row)).expect("replayed");
        assert!(
            rep.claims.iter().all(|c| c.outcome.class == Class::Strong),
            "{text}: {:?}",
            rep.claims
        );
        assert!(
            rep.rows.iter().any(|r| !r.problems.is_empty()),
            "{text}: the replay accepts a false empty {row} row over {:?}",
            case.scope
        );
        // The same claims with the feature listed: true.
        let mut listed = case.clone();
        let feats = ctx.truth.features(row);
        let mut items = Vec::new();
        for mut f in feats {
            if listed.scope.holds(&f.at) {
                let (lo, hi) = f.at.enc();
                let (ylo, yhi) = ctx.y_of(lo, hi).expect("a value");
                let mut it = json!({"x": enc(lo, hi), "y": enc(ylo, yhi), "every": null});
                if row == "extrema" {
                    it["kind"] = json!(if f.max == Some(true) { "Max" } else { "Min" });
                }
                items.push(it);
            }
        }
        listed.value = Value::Array(items);
        assert_eq!(
            ctx.truth.judge(row, &listed.scope, &listed.value),
            Verdict::True,
            "{text}: {}",
            listed.value
        );
    }
}

/// The genuine rows the long runs found false (seeds 20261025 and
/// 20261026), held to the truth again: a turn or an inflection at the
/// double beside a hole, its side across the gap to the hole decided by
/// no claim, certified away ("no extrema", "no inflection points"). The
/// certifier now leaves such a row incomplete. Each function is made
/// again by its seed and index, and its genuine certificate judged as the
/// run judges it.
#[test]
fn the_genuine_rows_beside_a_hole_are_true() {
    let found: [(u64, &str); 4] = [
        (
            20_261_025,
            "1/3*(x+2)^3-2^(-53)*(x+2)^2+1/3+0/(x-(-9007199254740991*2^(-52)))",
        ),
        (
            20_261_026,
            "1/6*(x-1/2)^3+2^(-56)*(x-1/2)^2+1/3+0/(x-(1/2))",
        ),
        (
            20_261_026,
            "-1*(x-1/2)^3-3*2^(-55)*(x-1/2)^2+1+0/(x-(9007199254740991*2^(-54)))",
        ),
        (
            20_261_026,
            "1/24*(x-3)^4-2^(-53)/3*(x-3)^3+2^(-51)*(x-3)-1+0/(x-(3))",
        ),
    ];
    let bad = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            replay::iv::init();
            let mut bad = Vec::new();
            for (seed, text) in found {
                // (The function the run makes as its i-th.)
                let i = (0..2000)
                    .find(|&i| {
                        let mut rng = Rng::new(seed, i, 0);
                        (0..20)
                            .find_map(|_| make_fun(&mut rng))
                            .is_some_and(|f| f.text == text)
                    })
                    .unwrap_or_else(|| panic!("{text}: not made by seed {seed}"));
                let t = run_function(seed, i, 0);
                assert!(t.genuine_rows > 0, "{text}: no genuine row judged");
                bad.extend(t.genuine_false);
            }
            bad
        })
        .expect("a thread")
        .join()
        .expect("held to the truth");
    assert!(bad.is_empty(), "{bad:#?}");
}
