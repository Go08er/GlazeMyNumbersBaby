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
//! **Functions with corners** (`make_kinked`): |P| (times a constant,
//! plus one), P + a·|x − q|, P + a·|x − q₁| + b·|x − q₂|, |x − q|·P, the
//! max or min of P and P − R, and of a quadratic and one or two lines,
//! their corners at doubles, adjacent doubles, between doubles, at or
//! beside a turn of P, with a closed end or a hole at a corner or beside
//! one. The truth is worked piece by piece (`Pw`): f⁽ᵏ⁾ has no value at
//! a cut where f′, …, f⁽ᵏ⁾'s two sides differ, a turn may be at a corner,
//! a monotone stretch runs through a corner where f′ keeps its sign, an
//! inflection is at a corner only where f′ is continuous (|x − q|·P at a
//! root of P), and across a corner where f″ changes sign no complete row
//! of inflections is true (the certifier's and the replay's rule). A root
//! of even multiplicity of |P|'s P (or of R) is no corner, though the
//! certifier asks about a kink there. Claims of the certifier's shapes
//! about them: `Kink` on the box about a corner, `KinkAt` (f′'s one-sided
//! signs, the corner at a double), `KinkSides` for f′ and f″ at a double
//! or between doubles, joined or not, a flat side among them.
//!
//! **Periodic functions** (`make_trig`): A·T(a·x + b) + C, T sin, cos, tan
//! or cot, C/A one of T's values at whole multiples of 30° (45°), so every
//! zero, turn, inflection and pole is at a rational number of half-turns
//! s = θ/180°. Mostly in degrees, where that is a rational x and the
//! truth stays exact; at times in radians, x = −b/a + (s/a)·π, each
//! comparison with a double decided by MPFR, the precision raised until
//! it decides (its values too, enclosed and rounded out; never sampled).
//! Rows over one period with families (Certified), over windows across
//! periods and poles, over points; the domain's pole families; the
//! period row (P halved, doubled, moved a double, a prime's witness
//! dropped, the simplifier's fact halved); a row over one period without
//! the fact of its period, or over half of one.
//!
//! **Exhaustive mode** (`exhaustive_slice`, `exhaustive_full`): on fixed
//! functions (the reviewers' rows, the turn beside a hole, zeros at
//! adjacent doubles and between them, corners of each kind, sin, tan and
//! a shifted sine in degrees), about each place a row's features sit at,
//! every set of up to two (CI) or three claims from a pool of proven true
//! ones (its doubles and two either side, gaps between, boxes from far
//! off and from two doubles off ending on each, crossing and kink boxes),
//! crossed with every changed row (an item dropped, moved a double,
//! made up, of the other kind; a piece flipped, dropped, joined, an end
//! moved), over the place, windows from and to it, points and one period.
//!
//! What these found in the replay, each fixed with a regression in
//! `tests/certify_replay.rs`: an item listed alone at a member of a family
//! the row lists too passed for a number of its own (−3·tan(3x + 45)°
//! with −75 alone beside every 60° from −15;
//! `an_item_alone_at_a_familys_member_is_listed_twice`); and a monotone
//! piece ending at a double inside a hole's enclosure, past the hole, lay
//! "in" the domain piece before it (x − 5/7 − 2⁻⁵³/7 − ½|x − q| +
//! 0/(x + 2/7), long run seed 9103;
//! `a_monotone_piece_does_not_run_past_a_hole_in_its_enclosure`). No
//! genuine row of these functions was false.
//!
//! **The truth, audited.** Review 24 found it calling four false things
//! true, each with rows or claims built by hand: two items at one zero
//! of f ≡ 0, crossing claims without monotonicity, a periodic list's
//! turn count overflowing, a family of f ≡ 0 read as one point. The
//! audit that followed held every claim kind it reads to what the claim
//! promises (its doc in `certify/cert.rs` and what the replay's rows
//! take from it) and every list and row branch to what the row says. It
//! also called true a complete monotonicity row whose pieces met at one
//! enclosure holding two turns, the piece between left out (each end
//! was asked of some place on its own); a `TailChain` whose chain was
//! false; a periodic f's `GapClear` over an infinite box; an item, end
//! or period of a NaN enclosure, a low end "PosInf", a direction or
//! kind of no known word, a range over a window; a plateau's pieces,
//! range and zeros. Each is fixed, with a regression that fails on the
//! truth before. What it doesn't model is Unknown, never true: a row so
//! (a zero function's repeating list, a plateau, a periodic item over a
//! tail) fails the run; a premise so (a side expression's, a limit, a
//! bound, the simplifier's fact, a family) is not held to it. Review 25
//! found it counting a periodic f's cycles in doubles (a zero far out, or
//! past a large phase, never visited), taking a family's period for the
//! first multiple in its enclosure, and misreading an end enclosed from
//! −∞; the sweep that followed, the same rounding in the domain's poles
//! (tan(x − 10²⁰ + 8100)°'s all missed near 0), in the repeats of a
//! period's monotone pieces and in the order of a range's pieces, and a
//! scope from −∞ sent to `qd`. Every count of periods is exact now
//! (rational floors in degrees, MPFR's directed bounds in radians), each
//! fix with a regression.
//!
//! **The refuter** (`Ctx::refute_claim`, `Ctx::refute_row`), a second
//! check on the truth: every claim and row it calls true is tried against
//! f's own tree, evaluated by the replay's MPFR series (`replay::iv`,
//! written from the language's specification; of the truth's model only
//! where to look is read): on a grid over the claim's box or the row's
//! scope, at every double within two of each of the truth's zeros of f to
//! f‴, cuts and poles and within three of the domain's end. It refutes
//! only what an enclosure shows (a strict sign or monotone order broken,
//! a listed zero, turn or inflection where f⁽ᵏ⁾ isn't 0, two items at one
//! double, a change of sign between two samples that no item of a
//! complete row meets, a point in no monotone piece of the direction f′
//! shows, a value of f outside the range, f(x + P) ≠ f(x), the truth's
//! domain against f's definedness) and confirms nothing; a refutation
//! fails the run as a truth bug. Of what the truth before review 24's
//! fixes called true, it refutes R24-L-02's crossing claims (on that very
//! truth: each refuted), R24-L-01's two items at one double,
//! R24-L-03's sparse families and R24-L-04's family through −1
//! (`the_refuter_catches_review_24s_examples`); not R24-L-01's items
//! clipped to one domain end (only the end's exact place shows that one
//! point), nor a gap clear out to infinity, nor a range that holds every
//! sampled value.
//!
//! **Not covered**: plateaus (a piece where f is constant, max(P, c): the
//! truth says Unknown), steps (floor, round, sign, mod), fractional
//! powers, poles other than a hole and tan's and cot's, sums of sinusoids
//! with transcendental turns, and functions whose features no exact truth
//! here reaches. The rows of the y-intercept, parity and the asymptotes
//! are the mutation tester's and the corpus's; the domain row is held to
//! the domain as built (a Certified one otherwise fails the run).
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
//! after −2 not decided. Four such functions, turns and inflections, in
//! the long runs of seeds 20261025 and 20261026. The certifier now takes a
//! zero at a box's end for its bound only at a closed end of the domain
//! at that double, and otherwise leaves the row incomplete:
//! `certify::rows::side_unasked`, and the four held to the truth again in
//! [`the_genuine_rows_beside_a_hole_are_true`].) A true genuine row the
//! replay refuses is counted and printed, not failed.
//!
//! ```text
//! cargo test -p graphing --features mpfr-oracle --test replay_fuzz          # fixed seed, seconds
//! cargo test --release -p graphing --features mpfr-oracle --test replay_fuzz -- \
//!     --ignored long_run --nocapture
//! ```
//!
//! The long run takes `REPLAY_FUZZ_SEED`, `REPLAY_FUZZ_FUNCTIONS` (cases
//! come `REPLAY_FUZZ_CASES` to a function, 24 unless set),
//! `REPLAY_FUZZ_KIND` (poly, kink, trig, or mixed: half polynomials,
//! three in ten with corners, one in five periodic, each drawn from a
//! stream of its own) and `REPLAY_FUZZ_THREADS`; `REPLAY_FUZZ_SHOW=n`
//! prints the first n failures in full, `REPLAY_FUZZ_TIMES` each
//! function's time. The short runs (`short_run`, `short_run_kinked`,
//! `short_run_periodic`) and the exhaustive slice run in CI;
//!
//! ```text
//! cargo test --release -p graphing --features mpfr-oracle --test replay_fuzz -- \
//!     --ignored exhaustive_full --nocapture                 # REPLAY_FUZZ_SUBSETS, 3 unless set
//! ```
#![cfg(feature = "mpfr-oracle")]

mod replay;

use graphing::certify::{DEFAULT_BUDGET, certify_text};
use graphing::compile::CompileOptions;
use graphing::functions::TrigUnit;
use replay::{Claim, Class, KinkSign, Side, Subject};
use rug::float::{Constant, Round};
use rug::{Float, Integer, Rational};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::cmp::Ordering::{self, Equal, Greater, Less};
use std::collections::{BTreeMap, HashMap};
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

/// f as polynomials on pieces: `polys[i]` on the closed stretch from
/// `cuts[i − 1]` to `cuts[i]` (from −∞ before the first cut, to +∞ after
/// the last), f continuous at each cut. A polynomial is one piece; |p|,
/// p + a·|x − c|, max and min of two are several, cut where they kink.
#[derive(Clone, Debug, PartialEq)]
struct Pw {
    cuts: Vec<Rational>,
    polys: Vec<Poly>,
}

impl Pw {
    fn one(p: Poly) -> Pw {
        Pw {
            cuts: Vec::new(),
            polys: vec![p],
        }
    }

    fn deriv(&self) -> Pw {
        Pw {
            cuts: self.cuts.clone(),
            polys: self.polys.iter().map(Poly::deriv).collect(),
        }
    }

    fn sub_const(&self, v: &Rational) -> Pw {
        Pw {
            cuts: self.cuts.clone(),
            polys: self
                .polys
                .iter()
                .map(|p| p.sub(&Poly::constant(v.clone())))
                .collect(),
        }
    }

    fn is_zero(&self) -> bool {
        self.polys.iter().all(Poly::is_zero)
    }

    /// f on its domain: no cut at a closed end or past it (f there is
    /// the piece inside, its derivatives one-sided at the end).
    fn within(&self, dom: &Dom) -> Pw {
        let keep = |c: &Rational| match dom {
            Dom::From(a) => c > a,
            Dom::Upto(a) => c < a,
            _ => true,
        };
        let mut out = Pw {
            cuts: Vec::new(),
            polys: vec![self.polys[0].clone()],
        };
        for (j, c) in self.cuts.iter().enumerate() {
            if keep(c) {
                out.cuts.push(c.clone());
                out.polys.push(self.polys[j + 1].clone());
            } else if matches!(dom, Dom::From(_)) {
                // (Left of the end: the piece after it stands.)
                *out.polys.last_mut().expect("a piece") = self.polys[j + 1].clone();
            }
        }
        out
    }

    /// The pieces whose closed stretch holds x: one, or the two that meet
    /// at a cut.
    fn at(&self, x: &Rational) -> (usize, usize) {
        let i = self.cuts.partition_point(|c| c < x);
        if self.cuts.get(i) == Some(x) {
            (i, i + 1)
        } else {
            (i, i)
        }
    }

    /// The same for a place.
    fn at_place(&self, p: &Place) -> (usize, usize) {
        if let Place::At(x) = p {
            return self.at(x);
        }
        let mut i = 0;
        while i < self.cuts.len() && p.cmp(&self.cuts[i]) == Greater {
            i += 1;
        }
        if i < self.cuts.len() && p.cmp(&self.cuts[i]) == Equal {
            (i, i + 1)
        } else {
            (i, i)
        }
    }

    /// Piece i's stretch (None: an infinite end).
    fn ends(&self, i: usize) -> (Option<&Rational>, Option<&Rational>) {
        (i.checked_sub(1).map(|j| &self.cuts[j]), self.cuts.get(i))
    }

    /// The piece just right (or left) of x.
    fn beside(&self, x: &Rational, right: bool) -> usize {
        let (l, r) = self.at(x);
        if right { r } else { l }
    }

    /// The real roots of each piece's polynomial strictly inside its
    /// stretch, in order (a piece ≡ 0 left out).
    fn inner_roots(&self) -> Vec<Place> {
        let mut out = Vec::new();
        for (i, p) in self.polys.iter().enumerate() {
            if p.is_zero() {
                continue;
            }
            let (lo, hi) = self.ends(i);
            for r in real_roots(p) {
                let inside =
                    lo.is_none_or(|c| r.cmp(c) == Greater) && hi.is_none_or(|c| r.cmp(c) == Less);
                if inside {
                    out.push(Place::Root(r));
                }
            }
        }
        out
    }
}

// ------------------------------------------------------------ reals by MPFR

/// A closed interval of MPFR floats about a real, its ends rounded
/// outward (MPFR's own directed rounding; nothing of the replay's).
#[derive(Clone, Debug)]
struct Fi {
    lo: Float,
    hi: Float,
}

impl Fi {
    fn rat(q: &Rational, p: u32) -> Fi {
        Fi {
            lo: Float::with_val_round(p, q, Round::Down).0,
            hi: Float::with_val_round(p, q, Round::Up).0,
        }
    }

    fn pi(p: u32) -> Fi {
        Fi {
            lo: Float::with_val_round(p, Constant::Pi, Round::Down).0,
            hi: Float::with_val_round(p, Constant::Pi, Round::Up).0,
        }
    }

    fn prec(&self) -> u32 {
        self.lo.prec()
    }

    /// No bound at all.
    fn whole(p: u32) -> Fi {
        Fi {
            lo: Float::with_val(p, f64::NEG_INFINITY),
            hi: Float::with_val(p, f64::INFINITY),
        }
    }

    fn add(&self, o: &Fi) -> Fi {
        let p = self.prec();
        Fi {
            lo: Float::with_val_round(p, &self.lo + &o.lo, Round::Down).0,
            hi: Float::with_val_round(p, &self.hi + &o.hi, Round::Up).0,
        }
    }

    fn neg(&self) -> Fi {
        let p = self.prec();
        Fi {
            lo: Float::with_val(p, -&self.hi),
            hi: Float::with_val(p, -&self.lo),
        }
    }

    fn mul(&self, o: &Fi) -> Fi {
        let p = self.prec();
        let ends = [
            (&self.lo, &o.lo),
            (&self.lo, &o.hi),
            (&self.hi, &o.lo),
            (&self.hi, &o.hi),
        ];
        let down = ends.map(|(a, b)| Float::with_val_round(p, a * b, Round::Down).0);
        let up = ends.map(|(a, b)| Float::with_val_round(p, a * b, Round::Up).0);
        Fi {
            lo: down.into_iter().reduce(|a, b| a.min(&b)).expect("four"),
            hi: up.into_iter().reduce(|a, b| a.max(&b)).expect("four"),
        }
    }

    /// 1/o (None: o may be 0).
    fn recip(&self) -> Option<Fi> {
        let p = self.prec();
        if !(self.lo > 0 || self.hi < 0) {
            return None;
        }
        Some(Fi {
            lo: Float::with_val_round(p, 1 / &self.hi, Round::Down).0,
            hi: Float::with_val_round(p, 1 / &self.lo, Round::Up).0,
        })
    }

    /// sin over the interval, by its value at the low end widened by the
    /// interval's width (|sin′| ≤ 1).
    fn sin(&self) -> Fi {
        self.trig(true)
    }

    fn cos(&self) -> Fi {
        self.trig(false)
    }

    fn trig(&self, sin: bool) -> Fi {
        let p = self.prec();
        let w = Float::with_val_round(p, &self.hi - &self.lo, Round::Up).0;
        let mut d = Float::with_val(p, &self.lo);
        let mut u = Float::with_val(p, &self.lo);
        if sin {
            d.sin_round(Round::Down);
            u.sin_round(Round::Up);
        } else {
            d.cos_round(Round::Down);
            u.cos_round(Round::Up);
        }
        Fi {
            lo: Float::with_val_round(p, &d - &w, Round::Down).0,
            hi: Float::with_val_round(p, &u + &w, Round::Up).0,
        }
    }

    /// The real against q, if the interval decides it.
    fn cmp_rat(&self, q: &Rational) -> Option<Ordering> {
        if self.lo > *q {
            Some(Greater)
        } else if self.hi < *q {
            Some(Less)
        } else {
            None
        }
    }
}

/// A double's place in the order of all doubles, as a whole number.
fn ord(d: f64) -> i64 {
    let b = d.to_bits() as i64;
    if b < 0 { b ^ i64::MAX } else { b }
}

fn unord(o: i64) -> f64 {
    f64::from_bits((if o < 0 { o ^ i64::MAX } else { o }) as u64)
}

/// The doubles at or about a real enclosed by `fi`, `cmp(d)` the real
/// against the double d: bisected over the doubles in order. (Walked a
/// double at a time from the enclosure's end, a value near 0 was 2⁶⁰
/// doubles from an end 2⁻⁹⁶ off: the periodic run hung on −cos(5x − 60)°.)
fn doubles_about(cmp: impl Fn(f64) -> Ordering, fi: &Fi) -> (f64, f64) {
    let mut lo = ord(fi.lo.to_f64_round(Round::Down).max(f64::MIN));
    let mut hi = ord(fi.hi.to_f64_round(Round::Up).min(f64::MAX));
    debug_assert!(cmp(unord(lo)) != Less);
    while lo < hi {
        // (In i128: the whole line's doubles span more than i64 holds.)
        let mid = ((i128::from(lo) + i128::from(hi) + 1).div_euclid(2)) as i64;
        if cmp(unord(mid)) != Less {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let d = unord(lo);
    if cmp(d) == Equal {
        (d, d)
    } else {
        (d, d.next_up())
    }
}

/// A real (enclosed by `enc` at each precision) against q, the precision
/// raised until the enclosure decides it. The real must not be q (each
/// caller decides an exact equality exactly first).
fn decide(enc: impl Fn(u32) -> Fi, q: &Rational) -> Ordering {
    let mut p = 96;
    loop {
        if let Some(o) = enc(p).cmp_rat(q) {
            return o;
        }
        assert!(p < 1 << 16, "a real not told from {q} at {p} bits");
        p *= 2;
    }
}

/// floor(q/π), with an exact rational numerator. A nonzero rational
/// divided by π is not an integer, so directed MPFR bounds eventually
/// have the same floor. Never round a cycle count through a double.
fn floor_over_pi(q: &Rational) -> Integer {
    if q.cmp0() == Equal {
        return Integer::from(0);
    }
    let mut p = 128;
    loop {
        let v = Fi::rat(q, p).mul(&Fi::pi(p).recip().expect("π > 0"));
        let lo = v.lo.to_integer_round(Round::Down).expect("finite").0;
        let hi = v.hi.to_integer_round(Round::Down).expect("finite").0;
        if lo == hi {
            return lo;
        }
        assert!(p < 1 << 16, "a cycle boundary not decided at {p} bits");
        p *= 2;
    }
}

/// ⌊d/P⌋ for d = d0 + d1·π and a period P = p0 + p1·π > 0 (one part 0;
/// d rational where P is), exactly. In radians d/P = d0/(p1·π) + d1/p1 is
/// a whole number only where d0 = 0, so directed bounds decide the rest
/// (as `floor_over_pi` does). Never a count of periods through a double.
fn cycles(d0: &Rational, d1: &Rational, per: &(Rational, Rational)) -> Integer {
    let (p0, p1) = per;
    if p1.cmp0() == Equal {
        debug_assert_eq!(d1.cmp0(), Equal);
        return Rational::from(d0 / p0).floor().numer().clone();
    }
    let s = Rational::from(d1 / p1);
    if d0.cmp0() == Equal {
        return s.floor().numer().clone();
    }
    let q = Rational::from(d0 / p1);
    let mut p = 128;
    loop {
        let v = Fi::rat(&q, p)
            .mul(&Fi::pi(p).recip().expect("π > 0"))
            .add(&Fi::rat(&s, p));
        let lo = v.lo.to_integer_round(Round::Down).expect("finite").0;
        let hi = v.hi.to_integer_round(Round::Down).expect("finite").0;
        if lo == hi {
            return lo;
        }
        assert!(p < 1 << 16, "a count of periods not decided at {p} bits");
        p *= 2;
    }
}

// ------------------------------------------------------------ periodic functions

/// The trigonometric function of a periodic f.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TrigFn {
    Sin,
    Cos,
    Tan,
    Cot,
}

/// f = A·T(θ) + C, θ = a·x + b in degrees (or radians), T sin, cos, tan
/// or cot, and C/A one of the values T takes at whole multiples of 30°
/// (or 45°): 0, ±½, ±1. In half-turns s = θ/180° (θ/π), every zero of f,
/// f′, f″, f‴ and every pole is at a rational s; in degrees that is a
/// rational x, so the truth stays exact; in radians x = −b/a + (s/a)·π,
/// and a comparison of one with a double is decided by MPFR, the
/// precision raised until it decides (π is no rational).
#[derive(Clone, Debug)]
struct Trig {
    func: TrigFn,
    a: Rational,
    b: Rational,
    amp: Rational,
    c: Rational,
    deg: bool,
}

/// sin(π·u) exactly, where it is rational (u a whole multiple of 1/6, mod
/// 2: Niven's).
fn sin_pi_exact(u: &Rational) -> Option<Rational> {
    let two = qi(2);
    let v = u - Rational::from(u / &two).floor() * &two;
    let table: [(Rational, Rational); 8] = [
        (qi(0), qi(0)),
        (Rational::from((1, 6)), Rational::from((1, 2))),
        (Rational::from((1, 2)), qi(1)),
        (Rational::from((5, 6)), Rational::from((1, 2))),
        (qi(1), qi(0)),
        (Rational::from((7, 6)), Rational::from((-1, 2))),
        (Rational::from((3, 2)), qi(-1)),
        (Rational::from((11, 6)), Rational::from((-1, 2))),
    ];
    table.into_iter().find(|(w, _)| *w == v).map(|(_, s)| s)
}

impl Trig {
    /// T's period in half-turns.
    fn tp(&self) -> Rational {
        match self.func {
            TrigFn::Sin | TrigFn::Cos => qi(2),
            TrigFn::Tan | TrigFn::Cot => qi(1),
        }
    }

    /// The half-turns of one unit of θ: 180 (degrees), or π.
    fn theta(&self, x: &Rational) -> Rational {
        Rational::from(&self.a * x) + &self.b
    }

    /// s at x, where it is rational: always in degrees; in radians where
    /// θ = 0.
    fn s_exact(&self, x: &Rational) -> Option<Rational> {
        let th = self.theta(x);
        if self.deg {
            Some(th / 180u32)
        } else {
            (th.cmp0() == Equal).then(qi0)
        }
    }

    /// The whole cycle containing x, without rounding a·x + b or the
    /// quotient. Far from 0 a double's cycle count can skip whole periods;
    /// large opposite a·x and b can also cancel after losing their phase.
    fn cycle_at(&self, x: f64) -> Integer {
        let q = self.theta(&qd(x)) / self.tp();
        if self.deg {
            (q / 180u32).floor().numer().clone()
        } else {
            floor_over_pi(&q)
        }
    }

    /// f's period P = Tp·180/|a| (or Tp·π/|a|), as q0 + q1·π.
    fn period(&self) -> (Rational, Rational) {
        let p = &self.tp() / Rational::from(self.a.abs_ref());
        if self.deg {
            (p * 180u32, qi0())
        } else {
            (qi0(), p)
        }
    }

    /// The place at half-turns s: x = (180·s − b)/a, or −b/a + (s/a)·π.
    fn place(&self, s: &Rational) -> Place {
        if self.deg {
            Place::At((Rational::from(s * 180u32) - &self.b) / &self.a)
        } else {
            Place::Pi(
                Rational::from(-&self.b) / &self.a,
                Rational::from(s / &self.a),
            )
        }
    }

    /// The half-turns of a place of f (its zeros, turns, poles: all at
    /// rational s).
    fn s_of(&self, p: &Place) -> Option<Rational> {
        match p {
            Place::At(x) => self.s_exact(x),
            Place::Pi(q0, q1) => {
                (self.theta(q0).cmp0() == Equal).then(|| Rational::from(&self.a * q1))
            }
            Place::Root(_) => None,
        }
    }

    /// f⁽ᵏ⁾ = coef·T⁽ᵏ⁾(θ) (+ C for k = 0), coef = A·(a·π/180)ᵏ, or A·aᵏ:
    /// as (q, m), coef = q·πᵐ.
    fn coef(&self, k: usize) -> (Rational, u32) {
        let mut q = self.amp.clone();
        for _ in 0..k {
            q *= &self.a;
            if self.deg {
                q /= 180u32;
            }
        }
        (q, if self.deg { k as u32 } else { 0 })
    }

    /// T⁽ᵏ⁾ at half-turns s, exactly where rational; Err at a pole.
    fn tk_exact(&self, k: usize, s: &Rational) -> Result<Option<Rational>, ()> {
        match self.func {
            TrigFn::Sin | TrigFn::Cos => {
                // T⁽ᵏ⁾(πs) = sin(π(s + k/2)) (+ ½ for cos).
                let shift = Rational::from((k as i64 + i64::from(self.func == TrigFn::Cos), 2));
                Ok(sin_pi_exact(&Rational::from(s + &shift)))
            }
            TrigFn::Tan | TrigFn::Cot => {
                let v = s - s.clone().floor();
                let tan = self.func == TrigFn::Tan;
                let pole = if tan { Rational::from((1, 2)) } else { qi0() };
                if v == pole {
                    return Err(());
                }
                let t = if v == 0 || v == Rational::from((1, 2)) {
                    qi0()
                } else if v == Rational::from((1, 4)) {
                    qi(1)
                } else if v == Rational::from((3, 4)) {
                    qi(-1)
                } else {
                    return Ok(None);
                };
                Ok(Some(Trig::tan_poly(k, tan, &t)))
            }
        }
    }

    /// tan⁽ᵏ⁾ (cot⁽ᵏ⁾) as a polynomial in t = tan (cot): t, ±(1 + t²),
    /// 2t(1 + t²), ±2(1 + t²)(1 + 3t²).
    fn tan_poly(k: usize, tan: bool, t: &Rational) -> Rational {
        let t2 = Rational::from(t * t);
        let one = Rational::from(&t2 + 1u32);
        let sg = if tan { qi(1) } else { qi(-1) };
        match k {
            0 => t.clone(),
            1 => sg * one,
            2 => Rational::from(t * 2u32) * one,
            _ => sg * one * 2u32 * (Rational::from(&t2 * 3u32) + 1u32),
        }
    }

    /// T⁽ᵏ⁾ over an interval of θ in radians (None near a pole).
    fn tk_fi(&self, k: usize, th: &Fi) -> Option<Fi> {
        let (s, c) = (th.sin(), th.cos());
        Some(match self.func {
            TrigFn::Sin => [s.clone(), c.clone(), s.neg(), c.neg()][k].clone(),
            TrigFn::Cos => [c.clone(), s.neg(), c.neg(), s][k].clone(),
            TrigFn::Tan | TrigFn::Cot => {
                let tan = self.func == TrigFn::Tan;
                let t = if tan {
                    s.mul(&c.recip()?)
                } else {
                    c.mul(&s.recip()?)
                };
                let p = th.prec();
                let one = Fi::rat(&qi(1), p);
                let t2 = t.mul(&t);
                let sec = t2.add(&one);
                let sg = Fi::rat(&qi(if tan { 1 } else { -1 }), p);
                match k {
                    0 => t,
                    1 => sg.mul(&sec),
                    2 => Fi::rat(&qi(2), p).mul(&t).mul(&sec),
                    _ => sg
                        .mul(&Fi::rat(&qi(2), p))
                        .mul(&sec)
                        .mul(&Fi::rat(&qi(3), p).mul(&t2).add(&one)),
                }
            }
        })
    }

    /// f⁽ᵏ⁾(x) enclosed at p bits (x no pole).
    fn val_fi(&self, k: usize, x: &Rational, p: u32) -> Fi {
        let th = self.theta(x);
        let arg = if self.deg {
            Fi::pi(p).mul(&Fi::rat(&(th / 180u32), p))
        } else {
            Fi::rat(&th, p)
        };
        // (Near a pole the denominator's enclosure may hold 0 at this
        // precision: no bound, and `decide` raises it.)
        let Some(t) = self.tk_fi(k, &arg) else {
            return Fi::whole(p);
        };
        let (q, m) = self.coef(k);
        let mut v = t.mul(&Fi::rat(&q, p));
        for _ in 0..m {
            v = v.mul(&Fi::pi(p));
        }
        if k == 0 {
            v = v.add(&Fi::rat(&self.c, p));
        }
        v
    }

    /// f⁽ᵏ⁾ at half-turns s against y, f⁽ᵏ⁾ exact there (T⁽ᵏ⁾ rational).
    fn cmp_exact(&self, k: usize, t: &Rational, y: &Rational) -> Ordering {
        let (q, m) = self.coef(k);
        let qt = Rational::from(&q * t);
        let add = if k == 0 { self.c.clone() } else { qi0() };
        if m == 0 || qt.cmp0() == Equal {
            return (qt + add).cmp(y);
        }
        // q·T·πᵐ (+ C): never a rational but 0.
        let y = Rational::from(y - &add);
        decide(
            |p| {
                let mut v = Fi::rat(&qt, p);
                for _ in 0..m {
                    v = v.mul(&Fi::pi(p));
                }
                v
            },
            &y,
        )
    }

    /// f⁽ᵏ⁾(x) against y (x no pole).
    fn val_cmp(&self, k: usize, x: &Rational, y: &Rational) -> Ordering {
        if let Some(s) = self.s_exact(x)
            && let Ok(Some(t)) = self.tk_exact(k, &s)
        {
            return self.cmp_exact(k, &t, y);
        }
        // T⁽ᵏ⁾ irrational there, so f⁽ᵏ⁾ is no rational: MPFR decides.
        decide(|p| self.val_fi(k, x, p), y)
    }

    /// x is a pole.
    fn pole_at(&self, x: &Rational) -> bool {
        matches!(self.func, TrigFn::Tan | TrigFn::Cot)
            && self
                .s_exact(x)
                .is_some_and(|s| self.tk_exact(0, &s).is_err())
    }

    /// f⁽ᵏ⁾ − v's breakpoints in one period of s, [0, Tp): its zeros
    /// (false) and the poles (true), in order. (v = −C for k = 0.)
    fn breakpoints(&self, k: usize) -> Vec<(Rational, bool)> {
        let tp = self.tp();
        let r = |n: i64, d: i64| Rational::from((n, d));
        let mut out: Vec<(Rational, bool)> = Vec::new();
        match self.func {
            TrigFn::Sin | TrigFn::Cos => {
                // sin(π(s + shift)) = v.
                let v = if k == 0 {
                    Rational::from(-&self.c) / &self.amp
                } else {
                    qi0()
                };
                let shift = r(k as i64 + i64::from(self.func == TrigFn::Cos), 2);
                let us: Vec<Rational> = if v == 0 {
                    vec![qi(0), qi(1)]
                } else if v == r(1, 2) {
                    vec![r(1, 6), r(5, 6)]
                } else if v == r(-1, 2) {
                    vec![r(7, 6), r(11, 6)]
                } else if v == 1 {
                    vec![r(1, 2)]
                } else if v == -1 {
                    vec![r(3, 2)]
                } else {
                    Vec::new()
                };
                for u in us {
                    let s = Rational::from(&u - &shift);
                    let s = &s - Rational::from(&s / &tp).floor() * &tp;
                    out.push((s, false));
                }
            }
            TrigFn::Tan | TrigFn::Cot => {
                let tan = self.func == TrigFn::Tan;
                out.push((if tan { r(1, 2) } else { qi(0) }, true));
                let zero_t = if tan { qi(0) } else { r(1, 2) };
                match k {
                    0 => {
                        let v = Rational::from(-&self.c) / &self.amp;
                        if v == 0 {
                            out.push((zero_t, false));
                        } else if v == 1 {
                            out.push((r(1, 4), false));
                        } else if v == -1 {
                            out.push((r(3, 4), false));
                        }
                    }
                    2 => out.push((zero_t, false)),
                    _ => {}
                }
            }
        }
        out.sort();
        out
    }
}

fn qi0() -> Rational {
    Rational::new()
}

/// The derivative whose zeros (or changes of sign) a list row lists.
fn row_order(row: &str) -> usize {
    match row {
        "x_intercepts" => 0,
        "extrema" | "monotonicity" => 1,
        _ => 2,
    }
}

/// A place of a periodic f as (q0, q1): q0 + q1·π.
fn pq(p: &Place) -> (Rational, Rational) {
    match p {
        Place::At(x) => (x.clone(), qi0()),
        Place::Pi(q0, q1) => (q0.clone(), q1.clone()),
        Place::Root(_) => panic!("a root of a polynomial as q0 + q1·π"),
    }
}

/// q is a whole number.
fn whole(q: &Rational) -> bool {
    *q.denom() == 1
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
    /// ℝ less the poles x0 + k·P of tan or cot (each q0 + q1·π).
    Poles {
        x0: (Rational, Rational),
        per: (Rational, Rational),
    },
}

/// q0 + q1·π as a place (a rational where q1 = 0).
fn pi_place(q0: Rational, q1: Rational) -> Place {
    if q1.cmp0() == Equal {
        Place::At(q0)
    } else {
        Place::Pi(q0, q1)
    }
}

impl Dom {
    /// The k-th pole (Poles only).
    fn member(&self, k: &Integer) -> Place {
        let Dom::Poles { x0, per } = self else {
            unreachable!("poles")
        };
        let k = Rational::from(k);
        pi_place(
            Rational::from(&k * &per.0) + &x0.0,
            Rational::from(&k * &per.1) + &x0.1,
        )
    }

    /// The poles in [a, b] (Poles only; finite ends).
    fn members(&self, a: f64, b: f64) -> Vec<Place> {
        let Dom::Poles { x0, per } = self else {
            return Vec::new();
        };
        // (From ⌊(a − x0)/P⌋, at most the first pole's k, to ⌊(b − x0)/P⌋,
        // the last's, exactly. Counted in doubles, k was dozens off far
        // out, and near 0 where x0 is far and rounded: tan(x − 10²⁰ +
        // 8100)° had no pole in [0, 20], at 10: review 25.)
        let from = |e: f64| cycles(&(qd(e) - &x0.0), &Rational::from(-&x0.1), per);
        let (mut k, last) = (from(a), from(b));
        let mut out = Vec::new();
        while k <= last {
            let m = self.member(&k);
            if m.within(a, b) {
                out.push(m);
            }
            k += 1;
        }
        out
    }

    /// The pole at the place, if one is (Poles only).
    fn pole(&self, p: &Place) -> bool {
        let Dom::Poles { x0, per } = self else {
            return false;
        };
        // (q0 − x0 − k·P) + (q1 − x1 − k·P1)·π = 0, both parts.
        let (q0, q1) = match p {
            Place::At(x) => (x.clone(), qi0()),
            Place::Pi(q0, q1) => (q0.clone(), q1.clone()),
            Place::Root(_) => return false,
        };
        let (d0, d1) = (Rational::from(&q0 - &x0.0), Rational::from(&q1 - &x0.1));
        let k = if per.1.cmp0() != Equal {
            Rational::from(&d1 / &per.1)
        } else {
            if d1.cmp0() != Equal {
                return false;
            }
            Rational::from(&d0 / &per.0)
        };
        *k.denom() == 1 && Rational::from(&k * &per.0) == d0 && Rational::from(&k * &per.1) == d1
    }

    fn holds_q(&self, x: &Rational) -> bool {
        match self {
            Dom::Line => true,
            Dom::From(c) => x >= c,
            Dom::Upto(c) => x <= c,
            Dom::Hole(c) => x != c,
            Dom::Poles { .. } => !self.pole(&Place::At(x.clone())),
        }
    }

    /// The closed box [a, b] lies in the domain.
    fn holds_box(&self, a: f64, b: f64) -> bool {
        match self {
            Dom::Line => true,
            Dom::From(c) => a.is_finite() && qd(a) >= *c,
            Dom::Upto(c) => b.is_finite() && qd(b) <= *c,
            Dom::Hole(c) => !((!a.is_finite() || qd(a) <= *c) && (!b.is_finite() || *c <= qd(b))),
            Dom::Poles { .. } => a.is_finite() && b.is_finite() && self.members(a, b).is_empty(),
        }
    }

    /// The closed box [a, b] misses the domain.
    fn misses_box(&self, a: f64, b: f64) -> bool {
        match self {
            Dom::Line => false,
            Dom::From(c) => b.is_finite() && qd(b) < *c,
            Dom::Upto(c) => a.is_finite() && qd(a) > *c,
            Dom::Hole(c) => a == b && a.is_finite() && qd(a) == *c,
            Dom::Poles { .. } => a == b && a.is_finite() && !self.holds_q(&qd(a)),
        }
    }

    fn holds(&self, p: &Place) -> bool {
        match self {
            Dom::Line => true,
            Dom::From(c) => p.cmp(c) != Less,
            Dom::Upto(c) => p.cmp(c) != Greater,
            Dom::Hole(c) => p.cmp(c) != Equal,
            Dom::Poles { .. } => !self.pole(p),
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
            Dom::Line | Dom::Poles { .. } => None,
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
    f: Pw,
    dom: Dom,
    /// The points it was built about (roots planted, the centre).
    planted: Vec<Rational>,
    /// f′ and f″ as formulas, multiplied out (given with a domain end or a
    /// hole, where the replay's own algebra doesn't read f as a rational
    /// function and needs a tree to factor).
    expanded: Vec<String>,
    /// A periodic f (`f` then unused): its truth is the trigonometric
    /// function's, certified in its angle unit.
    trig: Option<Trig>,
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

/// The centre a function is built about (a double, or a point no double
/// holds), the gaps to the doubles above and below it, and offsets from
/// it: the doubles beside it, points between doubles, and others well
/// apart.
fn centre_and_palette(rng: &mut Rng) -> (Rational, Rational, Rational, Vec<Rational>) {
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
    (c, up, down, palette)
}

/// f(u) in powers of u = x − c, as typed.
fn u_text(fu: &Poly, c: &Rational) -> Option<String> {
    let (u, _) = x_minus_text(c, &qi(0))?;
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
    Some(if t.is_empty() { "0".into() } else { t })
}

/// lead·Π(x − c − o)^m as typed and as a formula.
fn factored_text(
    lead: &Rational,
    c: &Rational,
    offsets: &[Rational],
    mults: &[u32],
) -> Option<(String, String)> {
    let mut t = rat_text(lead)?;
    let mut fm = rat_formula(lead)?;
    for (o, m) in offsets.iter().zip(mults) {
        let (xt, xf) = x_minus_text(c, o)?;
        if *m == 1 {
            t = format!("{t}*{xt}");
            fm = format!("Mul({fm},{xf})");
        } else {
            t = format!("{t}*{xt}^{m}");
            fm = format!("Mul({fm},Pow({xf},{m}))");
        }
    }
    Some((t, fm))
}

fn make_fun(rng: &mut Rng) -> Option<Fun> {
    let (c, up, down, palette) = centre_and_palette(rng);
    if rng.chance(0.05) {
        // A constant (a plateau everywhere).
        let v = *rng.pick(&[0i64, 2, -3]);
        let mut fun = Fun {
            text: v.to_string(),
            evaluated: Vec::new(),
            f: Pw::one(Poly::constant(qi(v))),
            dom: Dom::Line,
            planted: vec![c.clone()],
            expanded: Vec::new(),
            trig: None,
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
    let factored = |lead: &Rational| factored_text(lead, &c, &offsets, &mults);
    let text = if k == 0 && rng.chance(0.6) {
        factored(&scale)?.0
    } else {
        u_text(&fu, &c)?
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
        f: Pw::one(fu.at_shift(&c)),
        dom: Dom::Line,
        planted,
        expanded,
        trig: None,
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
    fun.f = fun.f.within(&fun.dom);
    // (Its derivatives' trees, which the replay checks are f's exactly.)
    for t in fun.expanded.clone() {
        let name = &t[..t.find(" = ").expect("named")];
        if !fun.evaluated.iter().any(|e| e.starts_with(name)) {
            fun.evaluated.push(t);
        }
    }
    Some(())
}

/// A periodic f: A·T(a·x + b) + C, T sin, cos, tan or cot, mostly in
/// degrees (each zero, turn, inflection and pole at a rational), at
/// times in radians (at rationals plus rational multiples of π, each
/// told from a double by MPFR); C/A one of T's values at whole multiples
/// of 30° (45°), so f's zeros sit at those too, or f touches 0 at a turn.
fn make_trig(rng: &mut Rng) -> Option<Fun> {
    let deg = rng.chance(0.8);
    let func = *rng.pick(&[
        TrigFn::Sin,
        TrigFn::Sin,
        TrigFn::Cos,
        TrigFn::Cos,
        TrigFn::Tan,
        TrigFn::Cot,
    ]);
    let a = rng
        .pick(&[
            qi(1),
            qi(1),
            qi(2),
            qi(-1),
            qi(3),
            Rational::from((1, 2)),
            Rational::from((1, 3)),
            qi(-2),
            Rational::from((3, 2)),
            qi(5),
        ])
        .clone();
    let b = if deg {
        rng.pick(&[
            qi(0),
            qi(0),
            qi(30),
            qi(45),
            qi(-60),
            qi(90),
            qi(15),
            qi(10),
            qi(-135),
            Rational::from((1, 2)),
        ])
        .clone()
    } else {
        rng.pick(&[
            qi(0),
            qi(0),
            qi(1),
            Rational::from((-1, 2)),
            Rational::from((1, 3)),
        ])
        .clone()
    };
    let amp = rng
        .pick(&[qi(1), qi(1), qi(2), qi(-1), Rational::from((1, 2)), qi(-3)])
        .clone();
    let v = match func {
        TrigFn::Sin | TrigFn::Cos => rng
            .pick(&[
                qi(0),
                qi(0),
                Rational::from((1, 2)),
                Rational::from((-1, 2)),
                qi(1),
                qi(-1),
            ])
            .clone(),
        TrigFn::Tan | TrigFn::Cot => rng.pick(&[qi(0), qi(0), qi(1), qi(-1)]).clone(),
    };
    let c = -Rational::from(&amp * &v);
    let name = match func {
        TrigFn::Sin => "sin",
        TrigFn::Cos => "cos",
        TrigFn::Tan => "tan",
        TrigFn::Cot => "cot",
    };
    let arg = format!("{}x{}", coef_text(&a)?, plus_text(&b)?);
    let text = format!("{}{name}({arg}){}", coef_text(&amp)?, plus_text(&c)?);
    Some(Fun {
        text,
        evaluated: Vec::new(),
        f: Pw::one(Poly::constant(qi(1))),
        dom: Dom::Line,
        planted: Vec::new(),
        expanded: Vec::new(),
        trig: Some(Trig {
            func,
            a,
            b,
            amp,
            c,
            deg,
        }),
    })
}

/// A point at or beside q: q itself, the double next above or below it,
/// or the point halfway between q's double and the next (no double holds
/// it).
fn beside_point(rng: &mut Rng, q: &Rational) -> Rational {
    let lo = floor_double(q);
    match rng.below(6) {
        0 => qd(doubles_out(lo, 1)),
        1 => qd(doubles_out(ceil_double(q), -1)),
        2 => Rational::from(&qd(lo) + &qd(doubles_out(lo, 1))) / 2u32,
        _ => q.clone(),
    }
}

/// The max (or min) of polynomials, as pieces: cut at `cands` (every
/// rational where two of them may cross), the greatest (least) on each
/// stretch between, two stretches of one polynomial joined.
fn envelope(args: &[Poly], max: bool, mut cands: Vec<Rational>) -> Pw {
    cands.sort();
    cands.dedup();
    let pick = |x: &Rational| -> usize {
        let vals: Vec<Rational> = args.iter().map(|g| g.eval(x)).collect();
        let mut best = 0;
        for (i, v) in vals.iter().enumerate() {
            if (max && *v > vals[best]) || (!max && *v < vals[best]) {
                best = i;
            }
        }
        best
    };
    let sample = |i: usize| -> Rational {
        match (i.checked_sub(1).map(|j| &cands[j]), cands.get(i)) {
            (None, None) => qi(0),
            (None, Some(b)) => Rational::from(b - 1u32),
            (Some(a), None) => Rational::from(a + 1u32),
            (Some(a), Some(b)) => Rational::from(a + b) / 2u32,
        }
    };
    let mut out = Pw {
        cuts: Vec::new(),
        polys: vec![args[pick(&sample(0))].clone()],
    };
    for i in 1..=cands.len() {
        let g = &args[pick(&sample(i))];
        if g != out.polys.last().expect("a piece") {
            out.cuts.push(cands[i - 1].clone());
            out.polys.push(g.clone());
        }
    }
    out
}

/// `{coef}*` as typed before a factor (nothing for 1, `-` for −1).
fn coef_text(a: &Rational) -> Option<String> {
    Some(if *a == 1 {
        String::new()
    } else if *a == -1 {
        "-".into()
    } else {
        format!("{}*", rat_text(a)?)
    })
}

/// `+t` or `-t` for a term added (nothing for 0).
fn plus_text(a: &Rational) -> Option<String> {
    Some(match a.cmp0() {
        Equal => String::new(),
        Greater => format!("+{}", rat_text(a)?),
        Less => format!("-{}", rat_text(&Rational::from(-a))?),
    })
}

/// A function with corners, its truth worked piece by piece: |P| (times
/// a constant, plus one), P + a·|x − k|, and the max or min of P and Q =
/// P − R, where P has its roots (or its turns) planted as `make_fun`'s
/// are, R its own, and the corners sit at rationals: at doubles, at
/// adjacent doubles, between doubles, at a turn of P or a double beside
/// one; then sometimes a closed domain end or a hole at a corner or
/// beside it. (A root of even multiplicity of |P|'s P, or of R, is no
/// corner, though the certifier asks about a kink there.)
fn make_kinked(rng: &mut Rng) -> Option<Fun> {
    let (c, up, down, palette) = centre_and_palette(rng);
    let shape = rng.below(14);
    // P about c: its roots planted (k = 0) or its turns (k = 1, P′'s).
    let k = if shape <= 2 {
        0
    } else {
        *rng.pick(&[0usize, 0, 1, 1])
    };
    let mut offsets: Vec<Rational> = Vec::new();
    if rng.chance(0.3) {
        offsets.push(qi(0));
        offsets.push(if rng.chance(0.5) {
            up.clone()
        } else {
            Rational::from(-&down)
        });
    }
    let n = (1 + rng.below(3)).max(offsets.len());
    while offsets.len() < n {
        let o = rng.pick(&palette).clone();
        if !offsets.contains(&o) {
            offsets.push(o);
        }
    }
    let mults: Vec<u32> = offsets
        .iter()
        .map(|_| *rng.pick(&[1u32, 1, 1, 2, 2, 3]))
        .collect();
    let scale = rng
        .pick(&[qi(1), qi(-1), qi(2), qi(-3), Rational::from((1, 2))])
        .clone();
    if mults.iter().sum::<u32>() + k as u32 > 6 {
        return None;
    }
    let mut fu = Poly::constant(scale.clone());
    for (o, m) in offsets.iter().zip(&mults) {
        fu = fu.mul(&Poly::x_minus(o).pow(*m));
    }
    for _ in 0..k {
        fu = fu.integral();
        if rng.chance(0.4) {
            let k0 = rng
                .pick(&[qi(1), qi(-1), Rational::from((1, 3)), up.clone()])
                .clone();
            fu = fu.add(&Poly::constant(k0));
        }
    }
    let p_text = if k == 0 && rng.chance(0.6) {
        factored_text(&scale, &c, &offsets, &mults)?.0
    } else {
        u_text(&fu, &c)?
    };
    let p = fu.at_shift(&c);
    let mut planted: Vec<Rational> = offsets.iter().map(|o| Rational::from(&c + o)).collect();
    // The sign of a polynomial on each piece between its roots `cuts`.
    let signs = |g: &Poly, cuts: &[Rational]| -> Vec<Ordering> {
        (0..=cuts.len())
            .map(|i| {
                if cuts.is_empty() {
                    sign_beside(g, &qi(0), true)
                } else if i == 0 {
                    sign_beside(g, &cuts[0], false)
                } else {
                    sign_beside(g, &cuts[i - 1], true)
                }
            })
            .collect()
    };
    let (text, f) = match shape {
        0..=2 => {
            // s·|P| + C: corners at P's roots of odd multiplicity.
            let s = rng
                .pick(&[qi(1), qi(1), qi(-1), qi(2), Rational::from((1, 2))])
                .clone();
            let c0 = rng
                .pick(&[qi(0), qi(0), qi(0), qi(1), Rational::from((-1, 2))])
                .clone();
            let mut cuts: Vec<Rational> = offsets
                .iter()
                .zip(&mults)
                .filter(|(_, m)| *m % 2 == 1)
                .map(|(o, _)| Rational::from(&c + o))
                .collect();
            cuts.sort();
            let polys = signs(&p, &cuts)
                .into_iter()
                .map(|sg| {
                    let sp = if sg == Less {
                        p.scale(&qi(-1))
                    } else {
                        p.clone()
                    };
                    sp.scale(&s).add(&Poly::constant(c0.clone()))
                })
                .collect();
            (
                format!("{}abs({p_text}){}", coef_text(&s)?, plus_text(&c0)?),
                Pw { cuts, polys },
            )
        }
        3..=6 => {
            // P + a·|x − q| or |x − q|·P, q at or beside one of P's
            // planted points (a root, or a turn), or near the centre. (At
            // a root of P, |x − q|·P has f′ continuous across q, 0 there,
            // and f″ changing sign: an inflection at a kink, x·|x|'s.)
            let base = if rng.chance(0.6) {
                rng.pick(&planted).clone()
            } else {
                Rational::from(&c + rng.pick(&palette))
            };
            let q = beside_point(rng, &base);
            let a = rng
                .pick(&[qi(1), qi(-1), qi(2), Rational::from((-1, 2)), qi(3)])
                .clone();
            let (xt, _) = x_minus_text(&q, &qi(0))?;
            let arg = if xt == "x" { "(x)".to_string() } else { xt };
            planted.push(q.clone());
            if shape == 6 {
                let g = p.mul(&Poly::x_minus(&q));
                (
                    format!("abs{arg}*({p_text})"),
                    Pw {
                        cuts: vec![q],
                        polys: vec![g.scale(&qi(-1)), g],
                    },
                )
            } else {
                let lin = Poly::x_minus(&q).scale(&a);
                let sign = if a.cmp0() == Less { "-" } else { "+" };
                let mag = coef_text(&Rational::from(a.abs_ref()))?;
                (
                    format!("{p_text}{sign}{mag}abs{arg}"),
                    Pw {
                        cuts: vec![q],
                        polys: vec![p.sub(&lin), p.add(&lin)],
                    },
                )
            }
        }
        10 | 11 => {
            // P + a·|x − q1| + b·|x − q2|: two corners, each kink
            // argument signed over the other's box.
            let mut qs: Vec<Rational> = Vec::new();
            while qs.len() < 2 {
                let base = if rng.chance(0.6) {
                    rng.pick(&planted).clone()
                } else {
                    Rational::from(&c + rng.pick(&palette))
                };
                let q = beside_point(rng, &base);
                if !qs.contains(&q) {
                    qs.push(q);
                }
            }
            qs.sort();
            let ab: Vec<Rational> = (0..2)
                .map(|_| {
                    rng.pick(&[qi(1), qi(-1), qi(2), Rational::from((-1, 2)), qi(3)])
                        .clone()
                })
                .collect();
            let mut text = p_text.clone();
            for (q, a) in qs.iter().zip(&ab) {
                let (xt, _) = x_minus_text(q, &qi(0))?;
                let arg = if xt == "x" { "(x)".to_string() } else { xt };
                let sign = if a.cmp0() == Less { "-" } else { "+" };
                text = format!(
                    "{text}{sign}{}abs{arg}",
                    coef_text(&Rational::from(a.abs_ref()))?
                );
            }
            planted.extend(qs.iter().cloned());
            // Left of both, between, right of both: each |x − q| as ∓.
            let lin = |i: usize, s: i64| Poly::x_minus(&qs[i]).scale(&ab[i]).scale(&qi(s));
            let polys = vec![
                p.add(&lin(0, -1)).add(&lin(1, -1)),
                p.add(&lin(0, 1)).add(&lin(1, -1)),
                p.add(&lin(0, 1)).add(&lin(1, 1)),
            ];
            (text, Pw { cuts: qs, polys })
        }
        12 | 13 => {
            // max or min of a quadratic P and one or two lines through
            // pairs of points at or beside P's, L = P − s·(x − q)(x − q′)
            // (a line: P's own square cancels): corners where they cross,
            // f″ ≡ 0 on a line's pieces, and with two lines a max of
            // three, its corners also where the lines cross.
            let pick_pt = |rng: &mut Rng, planted: &[Rational]| {
                let base = if rng.chance(0.6) {
                    rng.pick(planted).clone()
                } else {
                    Rational::from(&c + rng.pick(&palette))
                };
                beside_point(rng, &base)
            };
            let s2 = rng
                .pick(&[qi(1), qi(-1), qi(2), Rational::from((1, 2))])
                .clone();
            let (r1, r2) = (pick_pt(rng, &planted), pick_pt(rng, &planted));
            let quad = Poly::x_minus(&r1)
                .mul(&Poly::x_minus(&r2))
                .scale(&s2)
                .add(&Poly::constant(
                    rng.pick(&[qi(0), qi(0), qi(1), qi(-1)]).clone(),
                ));
            planted.extend([r1, r2]);
            let mut args = vec![quad.clone()];
            let mut cands: Vec<Rational> = Vec::new();
            for _ in 0..1 + rng.below(2) {
                let (q1, q2) = (pick_pt(rng, &planted), pick_pt(rng, &planted));
                let line = quad.sub(&Poly::x_minus(&q1).mul(&Poly::x_minus(&q2)).scale(&s2));
                cands.extend([q1.clone(), q2.clone()]);
                planted.extend([q1, q2]);
                args.push(line);
            }
            if args.len() == 3 {
                // (Where the two lines cross.)
                let d = args[1].sub(&args[2]);
                if d.degree() == 1 {
                    cands.push(Rational::from(-&d.0[0]) / &d.0[1]);
                }
            }
            let max = rng.chance(0.5);
            let texts: Vec<String> = args
                .iter()
                .map(|g| u_text(&g.at_shift(&Rational::from(-&c)), &c))
                .collect::<Option<_>>()?;
            let name = if max { "max" } else { "min" };
            (
                format!("{name}({})", texts.join(",")),
                envelope(&args, max, cands),
            )
        }
        _ => {
            // max or min of P and Q = P − R: corners at R's roots of odd
            // multiplicity, each at or beside a planted point of P.
            let nr = 1 + rng.below(2);
            let mut roffs: Vec<Rational> = Vec::new();
            for _ in 0..nr {
                let base = if rng.chance(0.5) {
                    rng.pick(&offsets).clone()
                } else {
                    rng.pick(&palette).clone()
                };
                let o = Rational::from(&beside_point(rng, &Rational::from(&c + &base)) - &c);
                if !roffs.contains(&o) {
                    roffs.push(o);
                }
            }
            let rmults: Vec<u32> = roffs.iter().map(|_| *rng.pick(&[1u32, 1, 2])).collect();
            let rs = rng
                .pick(&[qi(1), qi(-1), qi(2), Rational::from((1, 2))])
                .clone();
            let mut ru = Poly::constant(rs);
            for (o, m) in roffs.iter().zip(&rmults) {
                ru = ru.mul(&Poly::x_minus(o).pow(*m));
            }
            let qu = fu.sub(&ru);
            let q_text = u_text(&qu, &c)?;
            let (q, r) = (qu.at_shift(&c), ru.at_shift(&c));
            let mut cuts: Vec<Rational> = roffs
                .iter()
                .zip(&rmults)
                .filter(|(_, m)| *m % 2 == 1)
                .map(|(o, _)| Rational::from(&c + o))
                .collect();
            cuts.sort();
            planted.extend(roffs.iter().map(|o| Rational::from(&c + o)));
            let max = rng.chance(0.5);
            let polys = signs(&r, &cuts)
                .into_iter()
                .map(|sg| {
                    // (R > 0: P is the greater.)
                    if (sg == Greater) == max {
                        p.clone()
                    } else {
                        q.clone()
                    }
                })
                .collect();
            let name = if max { "max" } else { "min" };
            let text = if rng.chance(0.5) {
                format!("{name}({p_text},{q_text})")
            } else {
                format!("{name}({q_text},{p_text})")
            };
            (text, Pw { cuts, polys })
        }
    };
    // (No piece where f is constant: a plateau's turns are no list.)
    if f.polys.iter().any(|g| g.degree() == 0) || f.polys.iter().any(|g| g.degree() > 7) {
        return None;
    }
    planted.sort();
    planted.dedup();
    let mut fun = Fun {
        text,
        evaluated: Vec::new(),
        f,
        dom: Dom::Line,
        planted,
        expanded: Vec::new(),
        trig: None,
    };
    modifier(rng, &mut fun, &c, &palette)?;
    Some(fun)
}

// ------------------------------------------------------------ the truth

/// A point of the line: a rational, a root of a polynomial, or q0 + q1·π
/// (a feature of a periodic f in radians).
#[derive(Clone, Debug)]
enum Place {
    At(Rational),
    Root(Root),
    Pi(Rational, Rational),
}

impl Place {
    fn cmp(&self, q: &Rational) -> Ordering {
        match self {
            Place::At(x) => x.cmp(q),
            Place::Root(r) => r.cmp(q),
            Place::Pi(q0, q1) => pi_cmp(q0, q1, q),
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
            Place::Pi(..) => panic!("a polynomial's sign at q0 + q1·π"),
        }
    }

    /// The place's enclosure: its double, or the two beside it.
    fn enc(&mut self) -> (f64, f64) {
        match self {
            Place::At(x) => (floor_double(x), ceil_double(x)),
            Place::Root(r) => r.doubles(),
            Place::Pi(q0, q1) => {
                let fi = Fi::pi(128).mul(&Fi::rat(q1, 128)).add(&Fi::rat(q0, 128));
                doubles_about(|d| self.cmp(&qd(d)), &fi)
            }
        }
    }

    fn approx(&self) -> f64 {
        match self {
            Place::At(x) => x.to_f64(),
            Place::Root(r) => (Rational::from(&r.lo + &r.hi) / 2u32).to_f64(),
            Place::Pi(q0, q1) => {
                let v = Fi::pi(128).mul(&Fi::rat(q1, 128)).add(&Fi::rat(q0, 128));
                v.lo.to_f64()
            }
        }
    }
}

/// q0 + q1·π against q, exactly (by MPFR where q1 ≠ 0: π is no rational).
fn pi_cmp(q0: &Rational, q1: &Rational, q: &Rational) -> Ordering {
    if q1.cmp0() == Equal {
        return q0.cmp(q);
    }
    let y = Rational::from(q - q0);
    decide(|p| Fi::pi(p).mul(&Fi::rat(q1, p)), &y)
}

/// Two places in order, exactly.
fn pcmp(a: &Place, b: &Place) -> Ordering {
    match (a, b) {
        (Place::At(x), Place::At(y)) => x.cmp(y),
        (Place::Pi(..), Place::At(y)) => a.cmp(y),
        (Place::At(x), Place::Pi(..)) => b.cmp(x).reverse(),
        (Place::Pi(q0, q1), Place::Pi(r0, r1)) => {
            // (q0 − r0) + (q1 − r1)·π against 0.
            pi_cmp(&Rational::from(q0 - r0), &Rational::from(q1 - r1), &qi0())
        }
        (Place::Pi(..), Place::Root(_)) | (Place::Root(_), Place::Pi(..)) => {
            panic!("a root of a polynomial against q0 + q1·π")
        }
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
            for i in 0..8000 {
                // (Past 400 halvings, one root of both is their common
                // factor's; two apart may be 2⁻¹⁰⁷⁴ apart, a corner's piece
                // beside another's: halved on to tell them.)
                if i == 400 {
                    let g = gcd(&r.p, &s.p);
                    let lo = (&r.lo).max(&s.lo).clone();
                    let hi = (&r.hi).min(&s.hi).clone();
                    if g.degree() >= 1 && Sturm::new(&g).closed(&lo, &hi) > 0 {
                        return Equal;
                    }
                }
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

/// A closed stretch, an infinite end None.
type Stretch = (Option<Rational>, Option<Rational>);

/// The stretch meets [a, b] (closed; an infinite end of either reaches
/// out).
fn meets(s: &Stretch, a: f64, b: f64) -> bool {
    (!b.is_finite() || s.0.as_ref().is_none_or(|c| *c <= qd(b)))
        && (!a.is_finite() || s.1.as_ref().is_none_or(|c| qd(a) <= *c))
}

/// A periodic f's truth: f⁽ᵏ⁾'s breakpoints in one period of the
/// half-turns s (its zeros, false, and the poles, true) and its sign on
/// the arc after each, and the stretch of x its feature lists cover.
struct Periodic {
    trig: Trig,
    brk: [Vec<(Rational, bool)>; 4],
    arcs: [Vec<Ordering>; 4],
    span: (f64, f64),
}

impl Periodic {
    fn new(trig: Trig, span: (f64, f64)) -> Periodic {
        let brk = [0, 1, 2, 3].map(|k| trig.breakpoints(k));
        let mut per = Periodic {
            trig,
            brk,
            arcs: Default::default(),
            span,
        };
        let tp = per.trig.tp();
        per.arcs = [0, 1, 2, 3].map(|k| {
            let b = &per.brk[k];
            (0..b.len().max(1))
                .map(|j| {
                    let mid = match (b.get(j), b.get(j + 1)) {
                        (Some(x), Some(y)) => Rational::from(&x.0 + &y.0) / 2u32,
                        (Some(x), None) => (Rational::from(&x.0 + &b[0].0) + &tp) / 2u32,
                        _ => Rational::from((1, 3)),
                    };
                    per.cmp_s(k, &mid, &qi0())
                })
                .collect()
        });
        per
    }

    /// f⁽ᵏ⁾ at half-turns s against y (s no pole).
    fn cmp_s(&self, k: usize, s: &Rational, y: &Rational) -> Ordering {
        let t = &self.trig;
        match t.tk_exact(k, s) {
            Ok(Some(v)) => t.cmp_exact(k, &v, y),
            Ok(None) => decide(
                |p| {
                    let th = Fi::pi(p).mul(&Fi::rat(s, p));
                    let (q, m) = t.coef(k);
                    // (coef·πᵐ is (a/180)ᵏ·πᵏ in degrees, aᵏ in radians.)
                    let Some(tk) = t.tk_fi(k, &th) else {
                        return Fi::whole(p);
                    };
                    let mut v = tk.mul(&Fi::rat(&q, p));
                    for _ in 0..m {
                        v = v.mul(&Fi::pi(p));
                    }
                    if k == 0 {
                        v = v.add(&Fi::rat(&t.c, p));
                    }
                    v
                },
                y,
            ),
            Err(()) => panic!("f at a pole"),
        }
    }

    /// Where half-turns s lie among f⁽ᵏ⁾'s breakpoints: on brk[j]
    /// (Some(j)), or on the arc after brk[j].
    fn locate(&self, k: usize, s: &Rational) -> (Option<usize>, usize) {
        let tp = self.trig.tp();
        let u = s - Rational::from(s / &tp).floor() * &tp;
        let b = &self.brk[k];
        if let Some(j) = b.iter().position(|(r, _)| *r == u) {
            return (Some(j), j);
        }
        let j = b
            .iter()
            .rposition(|(r, _)| *r < u)
            .unwrap_or(b.len().saturating_sub(1));
        (None, j)
    }

    /// f⁽ᵏ⁾'s sign just left and right of half-turns s.
    fn sides_s(&self, k: usize, s: &Rational) -> (Ordering, Ordering) {
        let n = self.arcs[k].len();
        match self.locate(k, s) {
            (Some(j), _) => (self.arcs[k][(j + n - 1) % n], self.arcs[k][j]),
            (None, j) => (self.arcs[k][j], self.arcs[k][j]),
        }
    }

    /// The same in x: s falls as x rises where a < 0.
    fn sides_x(&self, k: usize, s: &Rational) -> (Ordering, Ordering) {
        let (l, r) = self.sides_s(k, s);
        if self.trig.a.cmp0() == Less {
            (r, l)
        } else {
            (l, r)
        }
    }

    /// A row's features in one period of s: (where, a maximum?). Its
    /// zeros for x-intercepts; f′'s and f″'s changes of sign for the
    /// others.
    fn feats_s(&self, row: &str) -> Vec<(Rational, Option<bool>)> {
        let k = row_order(row);
        self.brk[k]
            .iter()
            .filter(|(_, pole)| !pole)
            .filter_map(|(r, _)| {
                if k == 0 {
                    return Some((r.clone(), None));
                }
                let (l, rr) = self.sides_x(k, r);
                (l != rr && l != Equal && rr != Equal)
                    .then(|| (r.clone(), (k == 1).then_some(l == Greater)))
            })
            .collect()
    }

    /// s modulo T's period.
    fn residue(&self, s: &Rational) -> Rational {
        let tp = self.trig.tp();
        s - Rational::from(s / &tp).floor() * &tp
    }

    /// The least shift of s mapping a row's features (and their kinds)
    /// onto themselves: Tp/j. (sin's zeros repeat every half-turn,
    /// though f only every turn.)
    fn shift_s(&self, row: &str) -> Rational {
        let fs = self.feats_s(row);
        let tp = self.trig.tp();
        for j in (1..=fs.len().max(1)).rev() {
            let d = Rational::from(&tp / j as u32);
            let onto = fs.iter().all(|(r, m)| {
                let u = self.residue(&Rational::from(r + &d));
                fs.iter().any(|(r2, m2)| *r2 == u && m2 == m)
            });
            if onto {
                return d;
            }
        }
        tp
    }

    /// A row's features in [a, b] of x (finite): (where, its s, a
    /// maximum?).
    fn feats_in(&self, row: &str, a: f64, b: f64) -> Vec<(Place, Rational, Option<bool>)> {
        let fs = self.feats_s(row);
        self.between(row_order(row), a, b)
            .into_iter()
            .filter(|(_, pole)| !pole)
            .filter_map(|(p, _)| {
                let s = self.trig.s_of(&p)?;
                let u = self.residue(&s);
                let m = fs.iter().find(|(r, _)| *r == u)?.1;
                Some((p, s, m))
            })
            .collect()
    }

    /// A shift of s as one of x: (q0, q1), |Δx| = q0 + q1·π.
    fn dx(&self, ds: &Rational) -> (Rational, Rational) {
        let q = ds / Rational::from(self.trig.a.abs_ref());
        if self.trig.deg {
            (q * 180u32, qi0())
        } else {
            (qi0(), q)
        }
    }

    /// Every whole n ≥ 1 whose shift lies in [lo, hi]. The periods of
    /// different items must be chosen together with their first members:
    /// taking only the first n can make two otherwise disjoint orbits
    /// overlap. None means too many choices, not a false family.
    fn multiples_in(&self, ds: &Rational, lo: f64, hi: f64) -> Option<Vec<Integer>> {
        let (q0, q1) = self.dx(ds);
        if !(lo.is_finite() && hi.is_finite()) {
            return None;
        }
        let (mut from, to) = if q1.cmp0() == Equal {
            (
                (qd(lo) / &q0).ceil().numer().clone(),
                (qd(hi) / &q0).floor().numer().clone(),
            )
        } else {
            debug_assert_eq!(q0.cmp0(), Equal);
            (
                -floor_over_pi(&(-qd(lo) / &q1)),
                floor_over_pi(&(qd(hi) / &q1)),
            )
        };
        from = from.max(Integer::from(1));
        if Integer::from(&to - &from) >= 256 {
            return None;
        }
        let mut out = Vec::new();
        while from <= to {
            out.push(from.clone());
            from += 1;
        }
        Some(out)
    }

    /// The zeros of f⁽ᵏ⁾ and the poles (true) in [a, b] (finite), in
    /// order.
    fn between(&self, k: usize, a: f64, b: f64) -> Vec<(Place, bool)> {
        let t = &self.trig;
        let (ca, cb) = (t.cycle_at(a), t.cycle_at(b));
        let mut m = ca.clone().min(cb.clone()) - 1u32;
        let last = ca.max(cb) + 1u32;
        let mut out: Vec<(Place, bool)> = Vec::new();
        while m <= last {
            for (r, pole) in &self.brk[k] {
                let sv = r + Rational::from(&m) * t.tp();
                let pl = t.place(&sv);
                if pl.within(a, b) {
                    out.push((pl, *pole));
                }
            }
            m += 1;
        }
        out.sort_by(|x, y| pcmp(&x.0, &y.0));
        out
    }
}

struct Truth {
    /// f, f′, f″, f‴ (on f's pieces).
    d: [Pw; 4],
    dom: Dom,
    /// Where f⁽ᵏ⁾ is 0 where it exists, anywhere on the line: each piece's
    /// roots inside it, and the cuts where f⁽ᵏ⁾ exists and is 0 (a stretch
    /// where it is ≡ 0 is in `flats`). A polynomial's: its real roots.
    roots: [Vec<Place>; 4],
    /// Where f⁽ᵏ⁾ doesn't exist though f does (k ≥ 1): the cuts where
    /// f′, …, f⁽ᵏ⁾'s two sides differ. Those of f′ are f's corners.
    rough: [Vec<Rational>; 4],
    /// Both, in order (true: rough).
    events: [Vec<(Place, bool)>; 4],
    /// The stretches where f⁽ᵏ⁾ ≡ 0: a piece whose polynomial is 0.
    flats: [Vec<Stretch>; 4],
    zeros: Vec<Feature>,
    extrema: Vec<Feature>,
    inflections: Vec<Feature>,
    /// Where f″ changes sign with no inflection decided: across a corner
    /// (f′ jumps there) or across a stretch where f″ ≡ 0. A complete row
    /// of inflections whose scope meets one is not true (the replay's and
    /// the certifier's rule: no complete row passes a change of f″'s sign
    /// at a corner), nor is an item standing for one.
    blur: Vec<(Place, Place)>,
    /// Where f′ changes sign inside the domain (a corner of f among
    /// them), and the domain's end or hole: where monotone pieces end.
    breaks: Vec<Place>,
    /// A periodic f's (then `d` is unused, and the lists above are of
    /// the span's features alone).
    per: Option<Periodic>,
}

impl Truth {
    fn new(f: &Pw, dom: &Dom) -> Truth {
        let d1 = f.deriv();
        let d2 = d1.deriv();
        let d3 = d2.deriv();
        let d = [f.clone(), d1, d2, d3];
        let mut roots: [Vec<Place>; 4] = Default::default();
        let mut rough: [Vec<Rational>; 4] = Default::default();
        let mut flats: [Vec<Stretch>; 4] = Default::default();
        for k in 0..4 {
            roots[k] = d[k].inner_roots();
            for (i, p) in d[k].polys.iter().enumerate() {
                if p.is_zero() {
                    let (lo, hi) = d[k].ends(i);
                    flats[k].push((lo.cloned(), hi.cloned()));
                }
            }
            for (j, c) in f.cuts.iter().enumerate() {
                let exists = (1..=k).all(|m| d[m].polys[j].eval(c) == d[m].polys[j + 1].eval(c));
                if !exists {
                    rough[k].push(c.clone());
                } else if d[k].polys[j].sign(c) == Equal
                    && !(d[k].polys[j].is_zero() && d[k].polys[j + 1].is_zero())
                {
                    roots[k].push(Place::At(c.clone()));
                }
            }
            if !f.cuts.is_empty() {
                roots[k].sort_by(pcmp);
            }
        }
        let events = [0, 1, 2, 3].map(|k| {
            let mut e: Vec<(Place, bool)> = roots[k].iter().map(|p| (p.clone(), false)).collect();
            e.extend(rough[k].iter().map(|c| (Place::At(c.clone()), true)));
            if !rough[k].is_empty() {
                e.sort_by(|a, b| pcmp(&a.0, &b.0));
            }
            e
        });
        let mut t = Truth {
            d,
            dom: dom.clone(),
            roots,
            rough,
            events,
            flats,
            zeros: Vec::new(),
            extrema: Vec::new(),
            inflections: Vec::new(),
            blur: Vec::new(),
            breaks: Vec::new(),
            per: None,
        };
        t.zeros = t.roots[0]
            .iter()
            .filter(|p| dom.holds(p))
            .map(|p| Feature {
                at: p.clone(),
                max: None,
            })
            .collect();
        let mut extrema = t.changes(1);
        if !t.d[1].is_zero() {
            let end = match dom {
                Dom::From(c) => Some((c, true)),
                Dom::Upto(c) => Some((c, false)),
                _ => None,
            };
            if let Some((c, right)) = end {
                let i = t.d[0].beside(c, right);
                let g = t.d[0].polys[i].sub(&Poly::constant(t.d[0].polys[i].eval(c)));
                let s = sign_beside(&g, c, right);
                extrema.push(Feature {
                    at: Place::At(c.clone()),
                    max: Some(s == Less),
                });
            }
        }
        let mut inflections = t.changes(2);
        for i in &mut inflections {
            i.max = None;
        }
        // f″ changing sign across a corner or a flat stretch.
        let mut blur = Vec::new();
        for c in &t.rough[2] {
            let p = Place::At(c.clone());
            let (l, r) = t.sides(2, &p);
            if t.rough[1].contains(c) && dom.inside(&p) && l != r && l != Equal && r != Equal {
                blur.push((p.clone(), p));
            }
        }
        for (lo, hi) in &t.flats[2] {
            let (Some(lo), Some(hi)) = (lo, hi) else {
                continue;
            };
            let l = t.beside_sign(2, lo, false);
            let r = t.beside_sign(2, hi, true);
            if l != r && l != Equal && r != Equal {
                blur.push((Place::At(lo.clone()), Place::At(hi.clone())));
            }
        }
        let mut breaks: Vec<Place> = t.changes(1).into_iter().map(|t| t.at).collect();
        if let Some(c) = dom.point() {
            breaks.push(Place::At(c.clone()));
        }
        breaks.sort_by(|a, b| a.approx().total_cmp(&b.approx()));
        t.extrema = extrema;
        t.inflections = inflections;
        t.blur = blur;
        t.breaks = breaks;
        t
    }

    /// A periodic f's truth, its lists over `span`.
    fn new_trig(trig: &Trig, span: (f64, f64)) -> Truth {
        let per = Periodic::new(trig.clone(), span);
        let dom = match trig.func {
            TrigFn::Tan | TrigFn::Cot => {
                let pole = per.brk[0]
                    .iter()
                    .find(|(_, p)| *p)
                    .expect("a pole")
                    .0
                    .clone();
                let x0 = match trig.place(&pole) {
                    Place::At(x) => (x, qi0()),
                    Place::Pi(q0, q1) => (q0, q1),
                    Place::Root(_) => unreachable!("a pole at a root"),
                };
                Dom::Poles {
                    x0,
                    per: trig.period(),
                }
            }
            _ => Dom::Line,
        };
        let events = [0, 1, 2, 3].map(|k| {
            per.between(k, span.0, span.1)
                .into_iter()
                .filter(|(_, pole)| !pole)
                .map(|(p, _)| (p, false))
                .collect::<Vec<_>>()
        });
        let roots = [0, 1, 2, 3].map(|k| events[k].iter().map(|(p, _)| p.clone()).collect());
        let one = Pw::one(Poly::constant(qi(1)));
        let mut t = Truth {
            d: [one.clone(), one.clone(), one.clone(), one],
            dom,
            roots,
            rough: Default::default(),
            events,
            flats: Default::default(),
            zeros: Vec::new(),
            extrema: Vec::new(),
            inflections: Vec::new(),
            blur: Vec::new(),
            breaks: Vec::new(),
            per: Some(per),
        };
        t.zeros = t.roots[0]
            .iter()
            .map(|p| Feature {
                at: p.clone(),
                max: None,
            })
            .collect();
        t.extrema = t.changes(1);
        t.inflections = t.changes(2);
        for i in &mut t.inflections {
            i.max = None;
        }
        let mut breaks: Vec<Place> = t.extrema.iter().map(|f| f.at.clone()).collect();
        breaks.extend(t.dom.members(span.0, span.1));
        breaks.sort_by(pcmp);
        t.breaks = breaks;
        t
    }

    /// The zeros of f⁽ᵏ⁾ that may lie in [a, b]: a polynomial's all, a
    /// periodic f's there.
    fn zeros_near(&self, k: usize, a: f64, b: f64) -> Vec<Place> {
        match &self.per {
            Some(per) if a.is_finite() && b.is_finite() => per
                .between(k, a, b)
                .into_iter()
                .filter(|(_, pole)| !pole)
                .map(|(p, _)| p)
                .collect(),
            _ => self.roots[k].clone(),
        }
    }

    /// The value of f⁽ᵏ⁾ at a double, enclosed in the doubles about it
    /// (where it exists).
    fn val_enc(&self, k: usize, x: f64) -> Option<(f64, f64)> {
        let Some(per) = &self.per else {
            let v = self.at(k, x)?;
            return Some((floor_double(&v), ceil_double(&v)));
        };
        let q = qd(x);
        if per.trig.pole_at(&q) {
            return None;
        }
        Some(doubles_about(
            |d| per.trig.val_cmp(k, &q, &qd(d)),
            &per.trig.val_fi(k, &q, 96),
        ))
    }

    /// Where f⁽ᵏ⁾ changes sign strictly inside the domain: a zero, or a
    /// cut where it has no value (for f″, only where f′ is continuous: a
    /// corner is `blur`). (A maximum: from positive.)
    fn changes(&self, k: usize) -> Vec<Feature> {
        self.events[k]
            .iter()
            .filter(|(p, _)| self.dom.inside(p))
            .filter(|(p, rough)| {
                !(k == 2 && *rough && matches!(p, Place::At(c) if self.rough[1].contains(c)))
            })
            .filter_map(|(p, _)| {
                let (l, r) = self.sides(k, p);
                (l != r && l != Equal && r != Equal).then(|| Feature {
                    at: p.clone(),
                    max: Some(l == Greater),
                })
            })
            .collect()
    }

    fn constant(&self) -> bool {
        self.d[1].is_zero()
    }

    /// f constant on a stretch, not everywhere (max(P, c)): its turns,
    /// monotone pieces and range are not modelled (no generator makes
    /// one; `make_kinked` refuses a constant piece).
    fn plateau(&self) -> bool {
        !self.constant() && !self.flats[1].is_empty()
    }

    /// f⁽ᵏ⁾'s sign just right (or left) of x, x left out.
    fn beside_sign(&self, k: usize, x: &Rational, right: bool) -> Ordering {
        if let Some(per) = &self.per {
            return match per.trig.s_exact(x) {
                Some(s) => {
                    let (l, r) = per.sides_x(k, &s);
                    if right { r } else { l }
                }
                None => self.sign_q(k, x).expect("no pole"),
            };
        }
        let i = self.d[k].beside(x, right);
        sign_beside(&self.d[k].polys[i], x, right)
    }

    /// f⁽ᵏ⁾'s sign just left and just right of a zero or a cut.
    fn sides(&self, k: usize, p: &Place) -> (Ordering, Ordering) {
        if let Some(per) = &self.per {
            return per.sides_x(k, &per.trig.s_of(p).expect("a feature's half-turns"));
        }
        match p {
            Place::At(x) => (self.beside_sign(k, x, false), self.beside_sign(k, x, true)),
            Place::Root(r) => {
                let (i, _) = self.d[k].at_place(p);
                sides(&self.d[k].polys[i], r)
            }
            Place::Pi(..) => unreachable!("q0 + q1·π of a polynomial"),
        }
    }

    /// f⁽ᵏ⁾'s sign at x, None where it has no value (a cut where it
    /// doesn't exist).
    fn sign_q(&self, k: usize, x: &Rational) -> Option<Ordering> {
        if let Some(per) = &self.per {
            return (!per.trig.pole_at(x)).then(|| per.trig.val_cmp(k, x, &qi0()));
        }
        let (l, r) = self.d[k].at(x);
        if l != r && self.rough[k].contains(x) {
            return None;
        }
        Some(self.d[k].polys[l].sign(x))
    }

    /// f⁽ᵏ⁾(x), where it exists.
    fn value_q(&self, k: usize, x: &Rational) -> Option<Rational> {
        if self.per.is_some() {
            return None;
        }
        let (l, r) = self.d[k].at(x);
        if l != r && self.rough[k].contains(x) {
            return None;
        }
        Some(self.d[k].polys[l].eval(x))
    }

    /// f⁽ᵏ⁾ at a place against y (f⁽ᵏ⁾ exists there).
    fn cmp_value(&self, k: usize, p: &Place, y: &Rational) -> Ordering {
        if let Some(per) = &self.per {
            return match (per.trig.s_of(p), p) {
                (Some(s), _) => per.cmp_s(k, &s, y),
                (None, Place::At(x)) => per.trig.val_cmp(k, x, y),
                _ => unreachable!("a place of a periodic f"),
            };
        }
        let (i, _) = self.d[k].at_place(p);
        let g = self.d[k].polys[i].sub(&Poly::constant(y.clone()));
        p.clone().sign_of(&g)
    }

    /// f's value at the place lies in [lo, hi] (an infinite end no bound;
    /// a NaN end, or lo > hi, holds no value).
    fn value_within_at(&self, p: &Place, lo: f64, hi: f64) -> bool {
        if lo.is_nan() || hi.is_nan() || lo > hi || lo == f64::INFINITY || hi == f64::NEG_INFINITY {
            return false;
        }
        (!lo.is_finite() || self.cmp_value(0, p, &qd(lo)) != Less)
            && (!hi.is_finite() || self.cmp_value(0, p, &qd(hi)) != Greater)
    }

    /// The values of f⁽ᵏ⁾ over [a, b] lie in [lo, hi] (it exists
    /// throughout: no cut where it doesn't inside).
    fn values_within(
        &self,
        k: usize,
        a: &Rational,
        b: &Rational,
        lo: &Rational,
        hi: &Rational,
    ) -> bool {
        if let Some(per) = &self.per {
            // Its ends' values, and those at the turns of f⁽ᵏ⁾ between (no
            // pole there).
            let (af, bf) = (a.to_f64(), b.to_f64());
            if per.between(k, af, bf).iter().any(|(_, pole)| *pole) {
                return false;
            }
            let inside = |c: Ordering| c != Less;
            let within = |c_lo: Ordering, c_hi: Ordering| inside(c_lo) && c_hi != Greater;
            let ends = [a, b]
                .iter()
                .all(|x| within(per.trig.val_cmp(k, x, lo), per.trig.val_cmp(k, x, hi)));
            return ends
                && per
                    .between(k + 1, af, bf)
                    .iter()
                    .filter(|(p, pole)| !pole && p.cmp(a) == Greater && p.cmp(b) == Less)
                    .all(|(p, _)| within(self.cmp_value(k, p, lo), self.cmp_value(k, p, hi)));
        }
        if self.rough[k].iter().any(|c| a <= c && c <= b) {
            return false;
        }
        let mut from = a.clone();
        loop {
            let i = self.d[k].beside(&from, true);
            let to = match self.d[k].cuts.get(i) {
                Some(c) if c < b => c.clone(),
                _ => b.clone(),
            };
            let i = if from == *b { self.d[k].at(b).0 } else { i };
            if !values_within(&self.d[k].polys[i], &from, &to, lo, hi) {
                return false;
            }
            if to >= *b {
                return true;
            }
            from = to;
        }
    }

    /// How many zeros of f⁽ᵏ⁾, and cuts where it has no value, lie in
    /// [a, b] (a stretch where it is ≡ 0 meeting it: all).
    fn roots_in(&self, k: usize, a: f64, b: f64) -> usize {
        if let Some(per) = &self.per {
            // (Infinitely many on a tail.)
            if !(a.is_finite() && b.is_finite()) {
                return usize::MAX;
            }
            return per.between(k, a, b).len();
        }
        if self.flats[k].iter().any(|s| meets(s, a, b)) {
            return usize::MAX;
        }
        self.events[k]
            .iter()
            .filter(|(p, _)| p.within(a, b))
            .count()
    }

    /// The zeros of f⁽ᵏ⁾ and the cuts where it has no value in [a, b].
    fn events_within(&self, k: usize, a: f64, b: f64) -> Vec<(Place, bool)> {
        if self.per.is_some() {
            return self
                .zeros_near(k, a, b)
                .into_iter()
                .filter(|p| p.within(a, b))
                .map(|p| (p, false))
                .collect();
        }
        self.events[k]
            .iter()
            .filter(|(p, _)| p.within(a, b))
            .cloned()
            .collect()
    }

    /// f⁽ᵏ⁾ ≡ 0 on [a, b], existing throughout.
    fn flat_over(&self, k: usize, a: f64, b: f64) -> bool {
        self.flat_cover(k, a, b)
            && !self.rough[k]
                .iter()
                .any(|c| Place::At(c.clone()).within(a, b))
    }

    /// [a, b] lies in the stretches where f⁽ᵏ⁾ ≡ 0 (a cut between two
    /// where it has no value aside).
    fn flat_cover(&self, k: usize, a: f64, b: f64) -> bool {
        let holds = |s: &Stretch, x: f64| {
            s.0.as_ref().is_none_or(|c| x.is_finite() && *c <= qd(x))
                && s.1.as_ref().is_none_or(|c| x.is_finite() && qd(x) <= *c)
        };
        // From a, stretch by stretch.
        let Some(mut s) = self.flats[k]
            .iter()
            .find(|s| holds(s, a) || (a == f64::NEG_INFINITY && s.0.is_none()))
        else {
            return false;
        };
        loop {
            match &s.1 {
                None => return true,
                Some(c) if b.is_finite() && qd(b) <= *c => return true,
                Some(c) => match self.flats[k].iter().find(|t| t.0.as_ref() == Some(c)) {
                    Some(t) => s = t,
                    None => return false,
                },
            }
        }
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
        Some(self.sign_q(k, &m)? == Greater)
    }

    /// The value of f⁽ᵏ⁾ at a double, where it exists.
    fn at(&self, k: usize, x: f64) -> Option<Rational> {
        self.value_q(k, &qd(x))
    }

    /// The sign of f⁽ᵏ⁾ at a double, where it exists.
    fn sign_at(&self, k: usize, x: f64) -> Option<Ordering> {
        self.sign_q(k, &qd(x))
    }

    /// The zeros of f⁽ᵏ⁾ strictly inside (a, b) where f is defined.
    fn roots_strictly_inside(&self, k: usize, a: f64, b: f64) -> usize {
        self.zeros_near(k, a, b)
            .iter()
            .filter(|p| {
                (!a.is_finite() || p.cmp(&qd(a)) == Greater)
                    && (!b.is_finite() || p.cmp(&qd(b)) == Less)
                    && self.dom.holds(p)
            })
            .count()
    }

    /// f⁽ᵏ⁾ wherever it exists on the stretch from a to b (an end open as
    /// asked; a cut where it has no value left out) is of the sign `s`:
    /// ≡ 0 for `Zero`.
    fn side_is(&self, k: usize, s: &KinkSign, a: f64, b: f64, open_a: bool, open_b: bool) -> bool {
        let inner = |p: &Place| {
            let lo = p.cmp(&qd(a));
            let hi = p.cmp(&qd(b));
            (lo == Greater || (lo == Equal && !open_a)) && (hi == Less || (hi == Equal && !open_b))
        };
        // (The reals strictly inside, and the closed ends asked.)
        let flat_meets = |st: &Stretch| {
            st.0.as_ref()
                .is_none_or(|c| *c < qd(b) || (!open_b && *c == qd(b)))
                && st
                    .1
                    .as_ref()
                    .is_none_or(|c| qd(a) < *c || (!open_a && *c == qd(a)))
        };
        if *s == KinkSign::Zero {
            // Every piece meeting the reals inside ≡ 0.
            let (i, _) = self.d[k].at(&qd(a));
            let (_, j) = self.d[k].at(&qd(b));
            return (i..=j).all(|n| {
                let (lo, hi) = self.d[k].ends(n);
                let touches = lo.is_some_and(|c| *c >= qd(b)) || hi.is_some_and(|c| *c <= qd(a));
                touches || self.d[k].polys[n].is_zero()
            });
        }
        let want = if *s == KinkSign::Pos { Greater } else { Less };
        if self.zeros_near(k, a, b).iter().any(inner) || self.flats[k].iter().any(flat_meets) {
            return false;
        }
        // f⁽ᵏ⁾'s sign on each stretch between the cuts where it has none.
        let mut cuts: Vec<Rational> = self.rough[k]
            .iter()
            .filter(|c| qd(a) < **c && **c < qd(b))
            .cloned()
            .collect();
        cuts.insert(0, qd(a));
        cuts.push(qd(b));
        cuts.windows(2).all(|w| {
            let m = Rational::from(&w[0] + &w[1]) / 2u32;
            self.sign_q(k, &m) == Some(want)
        })
    }

    /// Whether f⁽ᵏ⁾ is continuous and strictly monotone on a finite box.
    /// Isolated stationary points are allowed (x³), but no flat stretch
    /// is: each nondegenerate polynomial piece must move in the same
    /// direction. Exact critical values decide its derivative's weak sign.
    fn strictly_monotone(&self, k: usize, a: f64, b: f64) -> Option<bool> {
        if !(a.is_finite() && b.is_finite()) {
            return None;
        }
        let (a, b) = (qd(a), qd(b));
        if a > b || self.rough[k].iter().any(|c| a <= *c && *c <= b) {
            return Some(false);
        }
        if a == b {
            return Some(true);
        }
        if let Some(per) = &self.per {
            // For these four elementary trig functions every zero of
            // the next derivative is a change of its sign. A zero at an
            // end is allowed; a pole is excluded by the domain check.
            return Some(
                per.trig.amp.cmp0() != Equal
                    && !per
                        .between(k + 1, a.to_f64(), b.to_f64())
                        .iter()
                        .any(|(p, pole)| *pole || (p.cmp(&a) == Greater && p.cmp(&b) == Less)),
            );
        }
        let mut from = a;
        let mut direction = None;
        loop {
            let i = self.d[k].beside(&from, true);
            let to = self.d[k]
                .cuts
                .get(i)
                .filter(|c| **c < b)
                .cloned()
                .unwrap_or_else(|| b.clone());
            let p = &self.d[k].polys[i];
            let d = p.deriv();
            let going = p.eval(&to).cmp(&p.eval(&from));
            if d.is_zero() || going == Equal || direction.is_some_and(|s| s != going) {
                return Some(false);
            }
            direction = Some(going);
            let (lo, hi) = poly_range(&d, &from, &to);
            let signed = if going == Greater {
                lo >= 0 || values_within(&d, &from, &to, &qi(0), &hi)
            } else {
                hi <= 0 || values_within(&d, &from, &to, &lo, &qi(0))
            };
            if !signed {
                return Some(false);
            }
            if to == b {
                return Some(true);
            }
            from = to;
        }
    }

    /// `TailChain`'s chain of order m about f⁽ᵏ⁾ on the tail from `from`:
    /// f⁽ᵏ⁺ᵐ⁾ exists on the tail with one strict sign σ, and at `from`
    /// f⁽ᵏ⁺ⁱ⁾ (i < m) has the strict sign σ forces (σ on the right tail,
    /// σ·(−1)ᵐ⁻ⁱ on the left), σ the sign that leaves f⁽ᵏ⁾ on side `above`
    /// (the replay's `claims::tail_chain`). No chain holds on a tail of a
    /// periodic f (f⁽ᵏ⁺ᵐ⁾ changes sign there, or has poles).
    fn chain(&self, k: usize, side: Side, from: f64, above: bool, m: usize) -> bool {
        if self.per.is_some() || m == 0 || !from.is_finite() {
            return false;
        }
        let right = side == Side::Right;
        let sigma = if right || m.is_multiple_of(2) {
            above
        } else {
            !above
        };
        let want = |i: usize| {
            let s = if right {
                sigma
            } else {
                sigma == (m - i).is_multiple_of(2)
            };
            if s { Greater } else { Less }
        };
        let x = qd(from);
        let top = k + m;
        let mut d: Vec<Pw> = vec![self.d[0].clone()];
        for _ in 0..top {
            let next = d.last().expect("f").deriv();
            d.push(next);
        }
        let cuts = &self.d[0].cuts;
        // f⁽ᵗᵒᵖ⁾ exists on the closed tail: at each cut there, f′, …, f⁽ᵗᵒᵖ⁾
        // agree on both sides.
        for (j, c) in cuts.iter().enumerate() {
            let on = if right { *c >= x } else { *c <= x };
            if on && (1..=top).any(|n| d[n].polys[j].eval(c) != d[n].polys[j + 1].eval(c)) {
                return false;
            }
        }
        // Its strict sign on each piece's part of the tail.
        let want_top = if sigma { Greater } else { Less };
        for (i, p) in d[top].polys.iter().enumerate() {
            let (lo, hi) = d[top].ends(i);
            let (a, b) = if right {
                (
                    Some(lo.map_or(x.clone(), |l| l.clone().max(x.clone()))),
                    hi.cloned(),
                )
            } else {
                (
                    lo.cloned(),
                    Some(hi.map_or(x.clone(), |h| h.clone().min(x.clone()))),
                )
            };
            if let (Some(a), Some(b)) = (&a, &b)
                && a > b
            {
                continue;
            }
            if p.is_zero() {
                return false;
            }
            let inside = |r: &Root| {
                a.as_ref().is_none_or(|a| r.cmp(a) != Less)
                    && b.as_ref().is_none_or(|b| r.cmp(b) != Greater)
            };
            if real_roots(p).iter().any(inside) {
                return false;
            }
            let t = match (&a, &b) {
                (Some(a), Some(b)) => Rational::from(a + b) / 2u32,
                (Some(a), None) => Rational::from(a + 1u32),
                (None, Some(b)) => Rational::from(b - 1u32),
                (None, None) => qi(0),
            };
            if p.sign(&t) != want_top {
                return false;
            }
        }
        // The signs at `from` (each derivative exists there: checked).
        (0..m).all(|i| {
            let n = k + i;
            d[n].polys[d[n].at(&x).0].sign(&x) == want(i)
        })
    }

    /// Whether a claim is true (None: a kind this truth doesn't read).
    /// f⁽ᵏ⁾ is the polynomial's own derivative throughout the domain, at a
    /// closed end too, one-sided (as the replay reads `0*sqrt(u)`); at a
    /// cut where its two sides differ it has no value, and no claim of a
    /// sign or a value of it over a box holding one is true.
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
            Claim::Beyond { x, of, c, above } | Claim::Factors { x, of, c, above } => {
                let k = order(of)?;
                if *c != 0.0 {
                    return None;
                }
                sign_is(k, x.0, x.1, *above)
            }
            Claim::NoCross { x, of, c, above } => {
                let k = order(of)?;
                if *c != 0.0 {
                    return None;
                }
                sign_is(k, x.0, x.1, *above) && self.strictly_monotone(k, x.0, x.1)?
            }
            Claim::TailBeyond {
                side,
                from,
                of,
                c,
                above,
            } => {
                let k = order(of)?;
                if *c != 0.0 {
                    return None;
                }
                let (a, b) = tail(*side, *from);
                sign_is(k, a, b, *above)
            }
            Claim::TailChain {
                side,
                from,
                of,
                c,
                above,
                order: m,
            } => {
                let k = order(of)?;
                if *c != 0.0 {
                    return None;
                }
                // (Its chain too, not only where it ends: the rows read
                // f⁽ᵏ⁾ moving away from c on the tail from it (`rows::
                // tail_infinite`). x² + 1 above 0 on [−1, ∞) "by f′ > 0"
                // passed: the audit after review 24.)
                let (a, b) = tail(*side, *from);
                sign_is(k, a, b, *above) && self.chain(k, *side, *from, *above, *m)
            }
            Claim::OneCross { x, of, c } => {
                let k = order(of)?;
                if *c != 0.0 || self.d[k].is_zero() {
                    return None;
                }
                let monotone = self.strictly_monotone(k, x.0, x.1)?;
                let (a, b) = (self.sign_at(k, x.0), self.sign_at(k, x.1));
                ok_box(x)
                    && monotone
                    && a.is_some_and(|a| a != Equal)
                    && b.is_some_and(|b| b != Equal)
                    && a != b
                    && self.roots_in(k, x.0, x.1) == 1
            }
            Claim::ExactAt { x, of, c, at } => {
                let k = order(of)?;
                if *c != 0.0 || self.d[k].is_zero() {
                    return None;
                }
                ok_box(x)
                    && self.strictly_monotone(k, x.0, x.1)?
                    && x.0 <= *at
                    && *at <= x.1
                    && self.sign_at(k, *at) == Some(Equal)
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
                // (A box of doubles: `at` and the far end are ones.)
                ok_box(x)
                    && x.0.is_finite()
                    && x.1.is_finite()
                    && (*at == x.0 || *at == x.1)
                    && self.sign_at(k, *at) == Some(Equal)
                    && self.roots_in(k, x.0, x.1) == 1
                    && (x.0 == x.1
                        || self.sign_at(k, other) == Some(if *above { Greater } else { Less }))
            }
            Claim::Value { x, of, lo, hi } => {
                let k = order(of)?;
                // (An infinite end is no bound: a value past the doubles
                // is in [MAX, ∞], cot(5·10⁻³²⁴)°'s. Each value over a box
                // of doubles is below 10⁴⁰⁰⁰.)
                let bound = |v: f64| match v {
                    f64::INFINITY => Some(Rational::from(Integer::u_pow_u(10, 4000))),
                    f64::NEG_INFINITY => Some(-Rational::from(Integer::u_pow_u(10, 4000))),
                    v if v.is_finite() => Some(qd(v)),
                    _ => None,
                };
                let (Some(l), Some(h)) = (bound(*lo), bound(*hi)) else {
                    return Some(false);
                };
                // (A tail: f⁽ᵏ⁾ bounded there only where its outermost
                // piece is constant, as on both tails of a constant f, f′
                // ≡ 0 on (−∞, ∞), its period row's; then its values are
                // those up to the outermost cut.)
                let polys = &self.d[k].polys;
                let cuts = &self.d[k].cuts;
                let tail_ok = |right: bool| {
                    self.per.is_none()
                        && polys[if right { polys.len() - 1 } else { 0 }].degree() == 0
                };
                let a = match x.0 {
                    v if v.is_finite() => qd(v),
                    _ if tail_ok(false) => cuts.first().map_or(qi(0), |c| Rational::from(c - 1u32)),
                    _ => return Some(false),
                };
                let b = match x.1 {
                    v if v.is_finite() => qd(v),
                    _ if tail_ok(true) => cuts.last().map_or(qi(0), |c| Rational::from(c + 1u32)),
                    _ => return Some(false),
                };
                let a = if x.0.is_finite() { a } else { a.min(b.clone()) };
                let b = if x.1.is_finite() { b } else { b.max(a.clone()) };
                ok_box(x) && self.values_within(k, &a, &b, &l, &h)
            }
            Claim::Gap(x) => x.0 <= x.1,
            Claim::GapClear { x, of } => {
                let k = order(of)?;
                if x.0.is_nan() || x.1.is_nan() {
                    return Some(false);
                }
                if self.d[k].is_zero() {
                    return Some(true);
                }
                // (A periodic f's zeros over an infinite stretch: each
                // period's, if it has any. Its lists hold its span's
                // alone, and a box past the span was read as clear: round
                // 25's audit.)
                if let Some(per) = &self.per
                    && !(x.0.is_finite() && x.1.is_finite())
                {
                    return Some(x.0 >= x.1 || per.brk[k].iter().all(|(_, pole)| *pole));
                }
                // (≡ 0 throughout it, or no zero strictly inside it.)
                let inside = |s: &Stretch| {
                    s.0.as_ref()
                        .is_none_or(|c| !x.1.is_finite() || *c < qd(x.1))
                        && s.1
                            .as_ref()
                            .is_none_or(|c| !x.0.is_finite() || qd(x.0) < *c)
                };
                if self.flats[k].iter().any(inside) {
                    self.flat_cover(k, x.0, x.1)
                } else {
                    self.roots_strictly_inside(k, x.0, x.1) == 0
                }
            }
            Claim::KinkSides {
                x,
                at,
                order: k,
                left,
                right,
                joined,
            } => {
                // (A box of doubles: an infinite end is none.)
                if *k > 2
                    || *k == 0
                    || !ok_box(x)
                    || x.0 >= x.1
                    || !(x.0.is_finite() && x.1.is_finite())
                {
                    return Some(false);
                }
                // (Joined: f′ exists at the point and is continuous on x.)
                if *joined
                    && (*k != 2
                        || at.is_none()
                        || self.rough[1]
                            .iter()
                            .any(|c| Place::At(c.clone()).within(x.0, x.1)))
                {
                    return Some(false);
                }
                match at {
                    Some(p) => {
                        x.0 < *p
                            && *p < x.1
                            && self.side_is(*k, left, x.0, *p, false, true)
                            && self.side_is(*k, right, *p, x.1, true, false)
                    }
                    None => {
                        // Some c strictly inside with these signs on
                        // either side of it: a zero, or a cut where f⁽ᵏ⁾
                        // has no value; anywhere, where both sides agree.
                        let want = |s: &KinkSign| match s {
                            KinkSign::Pos => Greater,
                            KinkSign::Neg => Less,
                            KinkSign::Zero => Equal,
                        };
                        let whole = left == right && self.side_is(*k, left, x.0, x.1, false, false);
                        let evs = self.events_within(*k, x.0, x.1);
                        whole
                            || evs.iter().any(|(p, _)| {
                                // At a double: each side read on its own.
                                if let Place::At(c) = p
                                    && qd(c.to_f64()) == *c
                                {
                                    let d = c.to_f64();
                                    return x.0 < d
                                        && d < x.1
                                        && self.side_is(*k, left, x.0, d, false, true)
                                        && self.side_is(*k, right, d, x.1, true, false);
                                }
                                // Between doubles: the one in the box, and
                                // its sides.
                                let inner = p.cmp(&qd(x.0)) == Greater && p.cmp(&qd(x.1)) == Less;
                                let (l, r) = self.sides(*k, p);
                                inner
                                    && evs.len() == 1
                                    && !self.flats[*k].iter().any(|s| meets(s, x.0, x.1))
                                    && l == want(left)
                                    && r == want(right)
                            })
                    }
                }
            }
            Claim::KinkAt {
                x, left, right, at, ..
            } => {
                // The one corner in the box at `at`, f′ of the smooth form
                // f takes on each side strictly signed up to it (0 at `at`
                // itself allowed: x·|x|).
                // (Not modelled: that a kink argument of f's formula is 0
                // at `at`, a fact of the tree; |P| at an even root of P
                // has one and no corner. What the rows read, f strictly
                // monotone on each side with any corner at `at`, is.)
                let s = |up: bool| if up { KinkSign::Pos } else { KinkSign::Neg };
                ok_box(x)
                    && x.0.is_finite()
                    && x.1.is_finite()
                    && x.0 < *at
                    && *at < x.1
                    && self.rough[1]
                        .iter()
                        .all(|c| !Place::At(c.clone()).within(x.0, x.1) || *c == qd(*at))
                    && self.side_is(1, &s(*left), x.0, *at, false, true)
                    && self.side_is(1, &s(*right), *at, x.1, true, false)
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

/// `KinkSides` (`joined`: f′ continuous across the point, said of f″'s
/// sides at a point only).
fn c_sides(
    k: usize,
    a: f64,
    b: f64,
    at: Option<f64>,
    l: Ordering,
    r: Ordering,
    joined: bool,
) -> Value {
    json!({"KinkSides": {"x": bx(a, b), "at": at.map_or(Value::Null, num), "order": k,
        "left": kink_sign(l), "right": kink_sign(r), "joined": joined}})
}

fn c_kink_at(a: f64, b: f64, at: f64, left: bool, right: bool) -> Value {
    json!({"KinkAt": {"x": bx(a, b), "at": num(at), "left": left, "right": right}})
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
            None | Some(Equal) => None,
            Some(s) => Some(s == Greater),
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
        if self.t.sign_at(k, x).is_none() {
            // A corner (or a cut where f″ jumps): f is continuous there,
            // and f⁽ᵏ⁾ has no value.
            if self.rng.chance(0.5) {
                self.push(c_box("Kink", x, x));
            }
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
                    let (lo, hi) = self.t.val_enc(k, x).expect("a value");
                    self.push(c_value(k, x, x, lo, hi));
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
        let clear = Claim::GapClear {
            x: (a, b),
            of: Subject::F(self.k),
        };
        if self.t.claim_true(&clear) == Some(true) {
            if self.rng.chance(0.9) {
                self.push(c_clear(self.k, a, b));
            }
        } else if self.rng.chance(0.2) {
            // (A false clearness, which the claim check must refuse.)
            self.push(c_clear(self.k, a, b));
        }
    }

    /// The claim, if true (made up from the truth's own sides: a shape
    /// they don't fit is left out).
    fn push_true(&mut self, c: Value) {
        if replay::claim(&c).is_ok_and(|cl| self.t.claim_true(&cl) == Some(true)) {
            self.push(c);
        }
    }

    /// Claims about the box [l, r] about a cut where f⁽ᵏ⁾ has no value (a
    /// corner of f, or a jump of f″): the certifier's `Kink`, its `KinkAt`
    /// (f′'s one-sided signs, the corner at a double) and `KinkSides` (each
    /// derivative's sign on either side), each where true.
    fn kink_box(&mut self, l: f64, r: f64, c: &Place) {
        let k = self.k;
        self.push_true(c_box("Kink", l, r));
        let (sl, sr) = self.t.sides(k, c);
        let at = match c {
            Place::At(q) if qd(q.to_f64()) == *q && l < q.to_f64() && q.to_f64() < r => {
                Some(q.to_f64())
            }
            _ => None,
        };
        let joined = k == 2
            && at.is_some()
            && !self.t.rough[1]
                .iter()
                .any(|q| Place::At(q.clone()).within(l, r));
        match self.rng.below(4) {
            0 => {}
            1 if k == 1 && at.is_some() && sl != Equal && sr != Equal => {
                let p = at.expect("placed");
                self.push_true(c_kink_at(l, r, p, sl == Greater, sr == Greater));
            }
            _ => self.push_true(c_sides(k, l, r, at, sl, sr, joined)),
        }
        if self.rng.chance(0.2) {
            // (And the other order's, about the same box.)
            let o = 3 - k;
            let (ol, or) = self.t.sides(o, c);
            if o == 1
                && let Some(p) = at
                && self.rng.chance(0.5)
            {
                self.push_true(c_kink_at(l, r, p, ol == Greater, or == Greater));
            } else {
                let joined = o == 2
                    && at.is_some()
                    && !self.t.rough[1]
                        .iter()
                        .any(|q| Place::At(q.clone()).within(l, r));
                self.push_true(c_sides(o, l, r, at, ol, or, joined));
            }
        }
    }

    /// Claims covering [a, b] about the one cut c in it where f⁽ᵏ⁾ has no
    /// value: the certifier's shape (the stretches up to the kink box
    /// beside it, the box of the doubles about it), a kink box over all of
    /// [a, b], or a gap.
    fn rough_stretch(&mut self, a: f64, b: f64, c: &Place, inner: &[f64], depth: u32) {
        let (cl, ch) = c.clone().enc();
        let (bl, bh) = if cl == ch {
            (cl.next_down(), ch.next_up())
        } else {
            (cl, ch)
        };
        match self.rng.below(6) {
            0..=2 => {
                // Up to the kink's box, the box, and on from it.
                if a < bl {
                    self.stretch(a, bl.min(b), depth + 1);
                }
                self.kink_box(bl, bh, c);
                if bh < b {
                    self.stretch(bh.max(a), b, depth + 1);
                }
            }
            3 if a < cl && ch < b => self.kink_box(a, b, c),
            4 if narrow(a, b) => self.gap(a, b),
            _ if !inner.is_empty() => {
                let mut last = a;
                for &m in inner.iter().chain(std::iter::once(&b)) {
                    self.stretch(last, m, depth + 1);
                    last = m;
                }
            }
            _ => self.kink_box(a.min(bl), b.max(bh), c),
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
                1 if k > 0 => self.push(c_sides(k, a, b, None, Equal, Equal, false)),
                2 if narrow(a, b) => self.gap(a, b),
                _ => self.push(c_value(k, a, b, 0.0, 0.0)),
            }
            return;
        }
        if self.t.flats[k].iter().any(|s| meets(s, a, b)) {
            // A stretch where f⁽ᵏ⁾ ≡ 0 meets the box (one piece's).
            if self.t.flat_over(k, a, b) {
                match self.rng.below(3) {
                    0 => self.push(c_sides(k, a, b, None, Equal, Equal, false)),
                    1 if narrow(a, b) => self.gap(a, b),
                    _ => self.push(c_value(k, a, b, 0.0, 0.0)),
                }
            } else if inner.is_empty() {
                let evs = self.t.events_within(k, a, b);
                match evs.as_slice() {
                    [(c, true)] if self.rng.chance(0.7) => {
                        self.rough_stretch(a, b, &c.clone(), &inner, depth)
                    }
                    _ => self.gap(a, b),
                }
            } else {
                split(self, &inner);
            }
            return;
        }
        let rs = self.t.events_within(k, a, b);
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
                            self.push(c_sides(k, a, b, None, s, s, false));
                        }
                        _ => self.push(c_sign("Beyond", k, a, b, up)),
                    }
                }
            }
            1 if rs[0].1 => {
                let c = rs[0].0.clone();
                self.rough_stretch(a, b, &c, &inner, depth);
            }
            1 => {
                let mut r = rs[0].0.clone();
                let (rl, rh) = r.enc();
                let (sl, sr) = self.t.sides(k, &r);
                let change = sl != sr;
                if rl == rh {
                    let e = rl;
                    if a < e && e < b {
                        match self.rng.below(5) {
                            0 | 1 if self.monotone(a, b) => self.push(c_exact(k, a, b, e)),
                            2 if k > 0 => {
                                self.push(c_sides(k, a, b, Some(e), sl, sr, k == 2));
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
                            self.push(c_sides(k, a, b, None, sl, sr, false));
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
                    // (The outermost piece's degree.)
                    let polys = &self.t.d[k].polys;
                    let deg = polys[if right { polys.len() - 1 } else { 0 }]
                        .degree()
                        .max(1);
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
        let changes: Vec<Place> = self.t.roots[k]
            .iter()
            .filter(|r| {
                let (l, rr) = self.t.sides(k, r);
                l != rr
            })
            .cloned()
            .collect();
        if changes.is_empty() {
            return;
        }
        let mut r = self.rng.pick(&changes).clone();
        let (lo, hi) = r.enc();
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

    /// Claims at and about the zeros of f⁽ᵏ⁾ (and the cuts where it has no
    /// value): a point, a one-ulp gap and a point beside it, a sign from a
    /// double off it.
    fn noise(&mut self) {
        let k = self.k;
        let near: Vec<f64> = self.t.events[k]
            .iter()
            .flat_map(|(r, _)| {
                let (lo, hi) = r.clone().enc();
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
    /// One period of a periodic f, standing for the line (Certified).
    Period(f64, f64),
    Points,
}

impl Scope {
    fn covers(&self) -> Value {
        match self {
            Scope::Line => json!("Line"),
            Scope::Window(a, b) => json!({"Window": {"a": num(*a), "b": num(*b)}}),
            Scope::Period(a, b) => json!({"Period": {"a": num(*a), "b": num(*b)}}),
            Scope::Points => json!("Points"),
        }
    }

    fn status(&self) -> &'static str {
        if matches!(self, Scope::Line | Scope::Period(..)) {
            "Certified"
        } else {
            "Partial"
        }
    }

    fn complete(&self) -> bool {
        *self != Scope::Points
    }

    /// The whole line (by f's period, for a period).
    fn whole(&self) -> bool {
        matches!(self, Scope::Line | Scope::Period(..))
    }

    fn holds(&self, p: &Place) -> bool {
        match self {
            Scope::Line | Scope::Period(..) => true,
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
    /// A row shape outside this truth's model. Never counted as true or
    /// false; a generated or genuine row with this verdict fails the run.
    Unknown(String),
}

/// A listed item: where, f's value there and (an extremum) its kind.
struct Item {
    x: (f64, f64),
    y: Option<(f64, f64)>,
    max: Option<bool>,
    family: bool,
    /// A family's period (x its first member's place).
    every: Option<(f64, f64)>,
}

fn enc_of(v: &Value) -> Option<(f64, f64)> {
    Some((replay::r(v.get("lo")?).ok()?, replay::r(v.get("hi")?).ok()?))
}

/// An enclosure that holds a real: no NaN end, lo ≤ hi, not a point at
/// an infinity. (A NaN end read as an infinite one by `Place::within`
/// made an item `[NaN, NaN]` hold every feature, and the zero function's
/// list took it for a wide item: the audit after review 24.)
fn real_enc(x: (f64, f64)) -> bool {
    x.0 <= x.1 && x.0 != f64::INFINITY && x.1 != f64::NEG_INFINITY
}

/// An enclosure that holds a real, read as such (None: it holds none).
fn real_enc_of(v: &Value) -> Option<(f64, f64)> {
    enc_of(v).filter(|x| real_enc(*x))
}

/// A list row's items (None: unreadable, or an item that holds no real
/// number, or of a kind that is no extremum's: each a false row).
fn items_of(row: &str, value: &Value) -> Option<Vec<Item>> {
    value
        .as_array()?
        .iter()
        .map(|it| {
            if row == "x_intercepts" {
                if let Some(a) = it.get("At") {
                    return Some(Item {
                        x: real_enc_of(a)?,
                        y: None,
                        max: None,
                        family: false,
                        every: None,
                    });
                }
                let e = it.get("Every")?;
                return Some(Item {
                    x: real_enc_of(e.get("x0")?)?,
                    y: None,
                    max: None,
                    family: true,
                    every: Some(real_enc_of(e.get("period")?)?),
                });
            }
            let every = it.get("every").filter(|e| !e.is_null());
            // (A null y is no claim; one that is no enclosure of a real,
            // NaN or reversed, was read as none too, and so no false one.)
            let y = match it.get("y")? {
                Value::Null => None,
                y => Some(real_enc_of(y)?),
            };
            let max = match it.get("kind").filter(|k| !k.is_null()) {
                None => None,
                Some(k) => match k.as_str()? {
                    "Max" => Some(true),
                    "Min" => Some(false),
                    _ => return None,
                },
            };
            Some(Item {
                x: real_enc_of(it.get("x")?)?,
                y,
                max,
                family: every.is_some(),
                every: match every {
                    Some(e) => Some(real_enc_of(e)?),
                    None => None,
                },
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

/// A piece's low (`low`) or high end: `NegInf` only low, `PosInf` only
/// high, or at an enclosure that holds a real. (Any string was read as
/// the infinite end, so a range from `PosInf` was one from −∞: round
/// 25's audit.)
fn end_of(v: &Value, low: bool) -> Option<End> {
    if let Some(s) = v.as_str() {
        return (s == if low { "NegInf" } else { "PosInf" }).then_some(None);
    }
    let at = v.get("At")?;
    Some(Some((
        real_enc_of(at.get("x")?)?,
        at.get("closed")?.as_bool()?,
    )))
}

/// A monotone piece's direction: increasing (`Some(true)`), decreasing,
/// or constant (`None`); any other word is unreadable (it was read as
/// "constant", true of every piece of a constant f: the audit after
/// review 24).
fn dir_of(v: &Value) -> Option<Option<bool>> {
    match v.as_str()? {
        "Increasing" => Some(Some(true)),
        "Decreasing" => Some(Some(false)),
        "Constant" => Some(None),
        _ => None,
    }
}

/// An end of a stretch of x: −∞, a place, +∞.
#[derive(Clone, Debug)]
enum Ext {
    Neg,
    At(Place),
    Pos,
}

/// Two ends of stretches in order, exactly.
fn ecmp(a: &Ext, b: &Ext) -> Ordering {
    match (a, b) {
        (Ext::Neg, Ext::Neg) | (Ext::Pos, Ext::Pos) => Equal,
        (Ext::Neg, _) | (_, Ext::Pos) => Less,
        (_, Ext::Neg) | (Ext::Pos, _) => Greater,
        (Ext::At(p), Ext::At(q)) => pcmp(p, q),
    }
}

/// The closed stretches cover [from, to] (closed).
fn covers(stretches: &[(Ext, Ext)], from: &Ext, to: &Ext) -> bool {
    let mut reach = from.clone();
    loop {
        // (Of the stretches holding `reach`, the one reaching farthest.)
        let Some(best) = stretches
            .iter()
            .filter(|(p, q)| ecmp(p, &reach) != Greater && ecmp(q, &reach) != Less)
            .map(|(_, q)| q)
            .max_by(|a, b| ecmp(a, b))
        else {
            return false;
        };
        if ecmp(best, to) != Less {
            return true;
        }
        if ecmp(best, &reach) != Greater {
            // (The reals just past `reach` in none.)
            return false;
        }
        reach = best.clone();
    }
}

/// Whether one stretch chosen from each piece's (each with its repeats
/// by the period P = q0 + q1·π, when given) covers [from, to]: every
/// choice tried, None if there are too many.
fn covering(
    choices: &[Vec<(Ext, Ext)>],
    from: &Ext,
    to: &Ext,
    period: Option<&(Rational, Rational)>,
) -> Option<bool> {
    let n = choices
        .iter()
        .try_fold(1usize, |n, c| n.checked_mul(c.len()))?;
    if n > 4096 {
        return None;
    }
    // (Repeats cover only a finite [from, to]: one period.)
    let ends = match (period, from, to) {
        (Some(_), Ext::At(f), Ext::At(t)) => Some((pq(f), pq(t))),
        (Some(_), ..) => return None,
        _ => None,
    };
    let mut pick = vec![0usize; choices.len()];
    loop {
        let mut stretches: Vec<(Ext, Ext)> = Vec::new();
        for (c, i) in choices.iter().zip(&pick) {
            let (p, q) = &c[*i];
            match (period, p, q) {
                (Some(per), Ext::At(p), Ext::At(q)) => {
                    // (Its repeats that may meet [from, to]: k from
                    // ⌊(from − q)/P⌋, at most the first one's, to
                    // ⌊(to − p)/P⌋, exactly. Counted in doubles, one
                    // either side, a period from 10²⁰ got repeats a dozen
                    // periods past it, and sin(x)°'s two pieces left it
                    // out: review 25.)
                    let ((f0, f1), (t0, t1)) = ends.as_ref().expect("finite ends");
                    let (p, q) = (pq(p), pq(q));
                    let mut k = cycles(&(f0.clone() - &q.0), &(f1.clone() - &q.1), per);
                    let last = cycles(&(t0.clone() - &p.0), &(t1.clone() - &p.1), per);
                    while k <= last {
                        let k_q = Rational::from(&k);
                        let shift = |x: &(Rational, Rational)| {
                            pi_place(
                                Rational::from(&k_q * &per.0) + &x.0,
                                Rational::from(&k_q * &per.1) + &x.1,
                            )
                        };
                        stretches.push((Ext::At(shift(&p)), Ext::At(shift(&q))));
                        k += 1;
                    }
                }
                _ => stretches.push((p.clone(), q.clone())),
            }
        }
        if covers(&stretches, from, to) {
            return Some(true);
        }
        // (The next choice.)
        let mut i = 0;
        loop {
            if i == pick.len() {
                return Some(false);
            }
            pick[i] += 1;
            if pick[i] < choices[i].len() {
                break;
            }
            pick[i] = 0;
            i += 1;
        }
    }
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
            if items.iter().any(|i| i.family) {
                return Verdict::Unknown("zero-function repeating lists are not modeled".into());
            }
            // f ≡ 0: every point a zero, so an item is one where it meets
            // the domain; a complete list only over a window of one point.
            if items.iter().any(|i| self.dom.misses_box(i.x.0, i.x.1)) {
                return Verdict::False("an item outside the domain".into());
            }
            // A positive-width intersection holds infinitely many zeros,
            // so finitely many such items can name distinct ones. An
            // intersection of one point (including a clipped domain end)
            // has only that zero to offer: two items cannot share it.
            // (`items_of` refuses an item with no real point.)
            let mut points = std::collections::BTreeSet::new();
            for i in &items {
                let point = if i.x.0 == i.x.1 {
                    Some(qd(i.x.0))
                } else {
                    match &self.dom {
                        Dom::From(c) if i.x.1.is_finite() && qd(i.x.1) == *c => Some(c.clone()),
                        Dom::Upto(c) if i.x.0.is_finite() && qd(i.x.0) == *c => Some(c.clone()),
                        _ => None,
                    }
                };
                if let Some(point) = point
                    && !points.insert(point)
                {
                    return Verdict::False("two items name the same zero".into());
                }
            }
            return match scope {
                Scope::Points => Verdict::True,
                // (No point of the domain in the window: no zero.)
                Scope::Window(a, b) if self.dom.misses_box(*a, *b) => Verdict::True,
                Scope::Window(a, b)
                    if a == b
                        || matches!(&self.dom, Dom::From(c) if b.is_finite() && qd(*b) == *c)
                        || matches!(&self.dom, Dom::Upto(c) if a.is_finite() && qd(*a) == *c) =>
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
        // (Zeros on a stretch where f ≡ 0 though f isn't, a plateau's
        // turns: no list holds them, and no generator makes either. Read
        // as zeros and turns at the stretch's ends alone, they would pass
        // a list that leaves the stretch out: the audit after review 24.)
        if row == "x_intercepts" && !self.flats[0].is_empty() {
            return Verdict::Unknown("zeros on a stretch where f ≡ 0 are not modelled".into());
        }
        if row == "extrema" && self.plateau() {
            return Verdict::Unknown("a plateau's extrema are not modelled".into());
        }
        let mut feats = self.features(row);
        let ok = |it: &Item, f: &mut Feature| -> bool {
            f.at.within(it.x.0, it.x.1)
                && (it.max.is_none() || it.max == f.max)
                && it
                    .y
                    .is_none_or(|(lo, hi)| self.value_within_at(&f.at, lo, hi))
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
        // (f″ changing sign across a corner, or across a stretch where it
        // is ≡ 0: no inflection decided, so no complete row there.)
        if row == "inflections"
            && let Some((a, _)) = self
                .blur
                .iter()
                .find(|(a, b)| scope.holds(a) || scope.holds(b))
        {
            let (lo, hi) = a.clone().enc();
            return Verdict::False(format!(
                "f″ changes sign across a corner (or a flat stretch) at [{lo:e}, {hi:e}] in \
                 {scope:?}: no complete row of inflections there"
            ));
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
        // (A periodic f's from the span's ends: no stretch of it is
        // infinite, and those there are part of one.)
        let span = self.per.as_ref().map(|p| p.span);
        ends.push(match (&self.dom, span) {
            (_, Some((a, _))) => (Some(Place::At(qd(a))), true),
            (Dom::From(c), _) => (Some(Place::At(c.clone())), true),
            _ => (None, false),
        });
        let mut inner: Vec<(Place, bool)> = Vec::new();
        for b in &self.breaks {
            match (&self.dom, b) {
                (Dom::From(c) | Dom::Upto(c), Place::At(x)) if x == c => {}
                _ if !self.dom.holds(b) => inner.push((b.clone(), false)),
                _ => inner.push((b.clone(), true)),
            }
        }
        // (In order, exactly: a rational against a root by the root's
        // own comparison; two turns are roots of one square-free f′, apart.)
        inner.sort_by(|(a, _), (b, _)| pcmp(a, b));
        ends.extend(inner.into_iter().map(|(p, c)| (Some(p), c)));
        ends.push(match (&self.dom, span) {
            (_, Some((_, b))) => (Some(Place::At(qd(b))), true),
            (Dom::Upto(c), _) => (Some(Place::At(c.clone())), true),
            _ => (None, false),
        });
        ends.windows(2)
            .map(|w| {
                let dir = if self.d[1].is_zero() {
                    None
                } else {
                    // f′'s sign just inside the stretch.
                    let s = match (&w[0].0, &w[1].0) {
                        (Some(Place::At(x)), _) => self.beside_sign(1, x, true),
                        (Some(p), _) => self.sides(1, p).1,
                        (None, Some(Place::At(x))) => self.beside_sign(1, x, false),
                        (None, Some(p)) => self.sides(1, p).0,
                        (None, None) => self.beside_sign(1, &qi(0), true),
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
                Some(_) if x == f64::NEG_INFINITY => false,
                Some(_) if x == f64::INFINITY => true,
                Some(p) => p.cmp(&qd(x)) != Greater,
            }
        };
        let lt = |p: &Option<Place>, x: f64, low: bool| -> bool {
            match p {
                None => low,
                Some(_) if x == f64::NEG_INFINITY => false,
                Some(_) if x == f64::INFINITY => true,
                Some(p) => p.cmp(&qd(x)) == Less,
            }
        };
        let eq = |p: &Option<Place>, x: f64| {
            x.is_finite() && p.as_ref().is_some_and(|p| p.cmp(&qd(x)) == Equal)
        };
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
            let (Some(lo), Some(hi)) = (end_of(&on["lo"], true), end_of(&on["hi"], false)) else {
                return Verdict::False("unreadable".into());
            };
            let Some(dir) = dir_of(&it["dir"]) else {
                return Verdict::False("unreadable".into());
            };
            pieces.push((lo, hi, dir));
        }
        if self.plateau() {
            return Verdict::Unknown("a plateau's monotone pieces are not modelled".into());
        }
        for (lo, hi, dir) in &pieces {
            if let Err(why) = self.piece_true(lo, hi, *dir) {
                return Verdict::False(format!("the piece {lo:?} to {hi:?} {dir:?}: {why}"));
            }
        }
        if !scope.complete() {
            return Verdict::True;
        }
        // The part of the domain the scope asks for, closed (the union of
        // the pieces, each closed, holds the reals of a stretch just when
        // it holds the stretch's closure). (Where that is one point,
        // there is no piece to list: f is monotone on no stretch of it.)
        let (sa, sb) = match scope {
            Scope::Window(a, b) => (*a, *b),
            _ => (f64::NEG_INFINITY, f64::INFINITY),
        };
        let mut from = if sa.is_finite() {
            Ext::At(Place::At(qd(sa)))
        } else {
            Ext::Neg
        };
        let mut to = if sb.is_finite() {
            Ext::At(Place::At(qd(sb)))
        } else {
            Ext::Pos
        };
        match &self.dom {
            Dom::From(c) if ecmp(&from, &Ext::At(Place::At(c.clone()))) == Less => {
                from = Ext::At(Place::At(c.clone()));
            }
            Dom::Upto(c) if ecmp(&to, &Ext::At(Place::At(c.clone()))) == Greater => {
                to = Ext::At(Place::At(c.clone()));
            }
            _ => {}
        }
        if ecmp(&from, &to) != Less {
            // (Each piece listed is true: checked above.)
            return Verdict::True;
        }
        // Each piece stands for one stretch of f's: from a place in its low
        // end's enclosure where a piece of f may end (a zero of f′, the
        // domain's end or hole, the window's edge) to a later one in its
        // high end's, within one stretch of f in its direction. Which,
        // the row doesn't say: true if some choice, one for each piece,
        // covers the scope. (Each piece's ends were asked only of some
        // place in each enclosure, and the extents of the enclosures
        // covered: two pieces meeting at one enclosure that held two
        // turns, a piece of the other direction between them left out,
        // passed for true. x³ − x increasing up to [−0.6, 0.6] and from
        // it: the audit after review 24.)
        let mut choices: Vec<Vec<(Ext, Ext)>> = Vec::new();
        for (lo, hi, dir) in &pieces {
            match self.piece_choices(lo, hi, *dir, scope) {
                Ok(c) => choices.push(c),
                Err(why) => {
                    return Verdict::False(format!("the piece {lo:?} to {hi:?} {dir:?}: {why}"));
                }
            }
        }
        match covering(&choices, &from, &to, None) {
            Some(true) => Verdict::True,
            Some(false) => Verdict::False(format!(
                "the pieces, each from a place f's pieces end at to another, leave part of \
                 {scope:?} out"
            )),
            None => Verdict::Unknown("too many readings of the pieces' ends".into()),
        }
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
                && Place::At(c.clone()).within(x.0, x.1)
            {
                out.push(Place::At(c.clone()));
            }
            return out;
        }
        // (A corner too: a piece may end there.)
        let mut out: Vec<Place> = self.events[1]
            .iter()
            .map(|(p, _)| p.clone())
            .filter(|p| p.within(x.0, x.1) && self.dom.holds(p))
            .collect();
        if let Some(c) = self.dom.point()
            && Place::At(c.clone()).within(x.0, x.1)
        {
            out.push(Place::At(c.clone()));
        }
        if x.0.is_finite() && x.1.is_finite() {
            out.extend(self.dom.members(x.0, x.1));
        }
        if let Scope::Window(a, b) = scope {
            for e in [*a, *b] {
                if e.is_finite() && x.0 <= e && e <= x.1 {
                    out.push(Place::At(qd(e)));
                }
            }
        }
        out
    }

    /// A complete row's piece, as each stretch it may stand for: from an
    /// anchor in its low end's enclosure to a later one in its high
    /// end's, within one stretch of f in its direction, the widest such
    /// in each stretch (it covers what any other there covers).
    fn piece_choices(
        &self,
        lo: &End,
        hi: &End,
        dir: Option<bool>,
        scope: &Scope,
    ) -> Result<Vec<(Ext, Ext)>, String> {
        let ps: Option<Vec<Place>> = lo.map(|(x, _)| self.anchors_in(x, scope));
        let qs: Option<Vec<Place>> = hi.map(|(x, _)| self.anchors_in(x, scope));
        if ps.as_ref().is_some_and(Vec::is_empty) || qs.as_ref().is_some_and(Vec::is_empty) {
            return Err("an end where no piece of f ends".into());
        }
        let below = |a: &Option<Place>, b: &Place| a.as_ref().is_none_or(|a| pcmp(a, b) != Greater);
        let above = |a: &Option<Place>, b: &Place| a.as_ref().is_none_or(|a| pcmp(a, b) != Less);
        let mut out = Vec::new();
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
            // (The least start and the greatest end: if any start is below
            // any end, these are.)
            let ext = |p: &Option<Place>, low: bool| match p {
                Some(p) => Ext::At(p.clone()),
                None if low => Ext::Neg,
                None => Ext::Pos,
            };
            let p = starts.iter().map(|p| ext(p, true)).min_by(ecmp);
            let q = ends.iter().map(|q| ext(q, false)).max_by(ecmp);
            if let (Some(p), Some(q)) = (p, q)
                && ecmp(&p, &q) == Less
            {
                out.push((p, q));
            }
        }
        if out.is_empty() {
            return Err("no stretch of f between two of its anchors".into());
        }
        Ok(out)
    }

    fn judge(&self, row: &str, scope: &Scope, value: &Value) -> Verdict {
        // (A window or period with a NaN end, or reversed, holds no real;
        // read as one it made a complete row over nothing, or over the
        // line: the audit after review 24.)
        if let Scope::Window(a, b) | Scope::Period(a, b) = scope
            && !real_enc((*a, *b))
        {
            return Verdict::False(format!("a scope [{a:e}, {b:e}] that holds no real"));
        }
        // (The range and the period are statements about the whole line,
        // a part of either no row: the replay's `off_form`.)
        if matches!(row, "period" | "range") && !scope.whole() {
            return Verdict::False(format!(
                "a {row} row over {scope:?}: a part of one is no row"
            ));
        }
        if row == "period" {
            return self.judge_period(value);
        }
        // (Any other name was judged as a list of inflections.)
        if !matches!(
            row,
            "x_intercepts" | "extrema" | "inflections" | "monotonicity" | "range"
        ) {
            return Verdict::Unknown(format!("the {row} row is not modelled"));
        }
        if matches!(scope, Scope::Period(..)) && self.per.is_none() {
            return Verdict::False("a row over one period of an f with none".into());
        }
        match row {
            "monotonicity" if self.per.is_some() && scope.whole() => {
                self.judge_monotone_periodic(scope, value)
            }
            "monotonicity" => self.judge_monotone(scope, value),
            "range" => self.judge_range(value),
            _ if self.per.is_some() => self.judge_list_periodic(row, scope, value),
            _ => self.judge_list(row, scope, value),
        }
    }

    /// A list row of a periodic f. Each item holds a feature of its kind
    /// (f's value in its y); a family's period is a whole multiple of the
    /// shift that maps the features onto themselves, so each member is
    /// one; no feature is listed twice (two families' members meeting, an
    /// item alone among a family's); over the line (or one period) every
    /// feature is a family's member, and an item listed alone says its
    /// repeats are none; over a window each feature in it is listed.
    fn judge_list_periodic(&self, row: &str, scope: &Scope, value: &Value) -> Verdict {
        let per = self.per.as_ref().expect("periodic");
        let Some(items) = items_of(row, value) else {
            return Verdict::False("unreadable".into());
        };
        let ps = per.shift_s(row);
        let fits = |it: &Item, p: &Place, m: &Option<bool>| {
            (it.max.is_none() || it.max == *m)
                && it.y.is_none_or(|(lo, hi)| self.value_within_at(p, lo, hi))
        };
        // Each item's readings: an item alone, a feature it holds (its s);
        // a family, an orbit s0 + i·step of one (each orbit once). The row
        // is true if some reading of every item lists no feature twice
        // and, complete, every one: an enclosure stands for a number in
        // it, which one not said. (A family through [270.5, 450.5] every
        // 180 of −cot(−x + ½)° holds two zeros of one orbit; through
        // [−382.5, −292.5] every 180 of −sin(2x − 135)°'s inflections, two
        // orbits, either a true family.)
        #[derive(Clone)]
        enum Reading {
            One(Rational),
            Orbit(Rational, Rational),
        }
        let mut readings: Vec<Vec<Reading>> = Vec::new();
        let mut alone = false;
        let feats = per.feats_s(row);
        for it in &items {
            if !(it.x.0.is_finite() && it.x.1.is_finite()) {
                return Verdict::Unknown("a periodic f's item over an infinite stretch".into());
            }
            let held: Vec<Rational> = per
                .feats_in(row, it.x.0, it.x.1)
                .into_iter()
                .filter(|(p, _, m)| fits(it, p, m))
                .map(|(_, s, _)| s)
                .collect();
            if held.is_empty() {
                return Verdict::False(format!(
                    "the item at [{:e}, {:e}] is no {row} item",
                    it.x.0, it.x.1
                ));
            }
            match it.every {
                None => {
                    alone = true;
                    readings.push(held.into_iter().map(Reading::One).collect());
                }
                Some((lo, hi)) => {
                    let Some(ns) = per.multiples_in(&ps, lo, hi) else {
                        return Verdict::Unknown("too many readings of a family's period".into());
                    };
                    // (Every member of the orbit a feature of the kind listed,
                    // f's value there in the item's y: each of its residues
                    // in a period of T asked, not the first member's alone.
                    // f's values at features of one kind agree for these
                    // functions, so this held already: hardening, after
                    // review 24.)
                    let orbit_fits = |s0: &Rational, step: &Rational| {
                        let mut seen: Vec<Rational> = Vec::new();
                        let mut s = s0.clone();
                        loop {
                            let r = per.residue(&s);
                            if seen.contains(&r) {
                                return true;
                            }
                            let Some((_, m)) = feats.iter().find(|(f, _)| *f == r) else {
                                return false;
                            };
                            if !fits(it, &per.trig.place(&r), m) {
                                return false;
                            }
                            seen.push(r);
                            s += step;
                        }
                    };
                    let mut orbits: Vec<Reading> = Vec::new();
                    for n in ns {
                        let step = Rational::from(&ps * &Rational::from(n));
                        for s0 in held.iter().filter(|s0| orbit_fits(s0, &step)) {
                            let seen = orbits.iter().any(|o| {
                                matches!(o, Reading::Orbit(t, st) if *st == step && whole(&(Rational::from(s0 - t) / &step)))
                            });
                            if !seen {
                                orbits.push(Reading::Orbit(s0.clone(), step.clone()));
                            }
                        }
                    }
                    readings.push(orbits);
                }
            }
        }
        if alone && scope.whole() {
            return Verdict::False(format!(
                "an item listed alone in a row complete over the line: its repeats are {row} \
                 items too"
            ));
        }
        let in_orbit =
            |s: &Rational, s0: &Rational, step: &Rational| whole(&(Rational::from(s - s0) / step));
        // Two readings that are one feature (or share one).
        let meet = |a: &Reading, b: &Reading| match (a, b) {
            (Reading::One(x), Reading::One(y)) => x == y,
            (Reading::One(x), Reading::Orbit(s0, st))
            | (Reading::Orbit(s0, st), Reading::One(x)) => in_orbit(x, s0, st),
            (Reading::Orbit(a0, sa), Reading::Orbit(b0, sb)) => {
                // Members meet where b0 − a0 is a whole combination of the
                // two steps: a multiple of their greatest common one.
                let (na, nb) = (Rational::from(sa / &ps), Rational::from(sb / &ps));
                let g = Integer::from(na.numer().gcd_ref(nb.numer()));
                whole(&(Rational::from(b0 - a0) / Rational::from(&ps * &Rational::from(g))))
            }
        };
        // A complete line lists periodic subsets of the feature set.
        // The chosen orbits are pairwise disjoint (checked below), so
        // their exact total density equals the feature set's just when
        // no feature is missing. This avoids enumerating an LCM of their
        // periods, which may be arbitrarily large.
        let whole_density = scope
            .whole()
            .then(|| Rational::from(per.feats_s(row).len()) / per.trig.tp());
        let wanted: Vec<Rational> = match scope {
            // (A window with an infinite end, which `real_enc` lets
            // through, sent its end to `qd`: not modelled, as an item over
            // a tail isn't. Review 25.)
            Scope::Window(a, b) if !(a.is_finite() && b.is_finite()) => {
                return Verdict::Unknown("a periodic f's row over an infinite window".into());
            }
            Scope::Window(a, b) => per
                .feats_in(row, *a, *b)
                .into_iter()
                .map(|(_, s, _)| s)
                .collect(),
            _ => Vec::new(),
        };
        let covered = |chosen: &[Reading], s: &Rational| {
            chosen.iter().any(|r| match r {
                Reading::One(x) => x == s,
                Reading::Orbit(s0, st) => in_orbit(s, s0, st),
            })
        };
        // A reading of every item, each apart from the others (depth
        // first, a few thousand at most).
        let mut tried = 0u32;
        fn search(
            i: usize,
            readings: &[Vec<Reading>],
            chosen: &mut Vec<Reading>,
            ok: &dyn Fn(&[Reading]) -> bool,
            meet: &dyn Fn(&Reading, &Reading) -> bool,
            tried: &mut u32,
        ) -> bool {
            *tried += 1;
            if *tried > 20_000 {
                return false;
            }
            if i == readings.len() {
                return ok(chosen);
            }
            for r in &readings[i] {
                if chosen.iter().any(|c| meet(c, r)) {
                    continue;
                }
                chosen.push(r.clone());
                if search(i + 1, readings, chosen, ok, meet, tried) {
                    return true;
                }
                chosen.pop();
            }
            false
        }
        let ok = |chosen: &[Reading]| {
            if let Some(wanted) = &whole_density {
                let density = chosen.iter().fold(qi(0), |sum, r| match r {
                    Reading::Orbit(_, step) => sum + qi(1) / step,
                    Reading::One(_) => sum, // Already refused over the line.
                });
                density == *wanted
            } else {
                wanted.iter().all(|s| covered(chosen, s))
            }
        };
        if search(0, &readings, &mut Vec::new(), &ok, &meet, &mut tried) {
            Verdict::True
        } else if tried > 20_000 {
            // (Not every reading tried: no verdict, rather than a false one.)
            Verdict::Unknown("too many readings of the items".into())
        } else {
            Verdict::False(format!(
                "no reading of its items lists each {row} item once{}",
                if scope.complete() {
                    format!(" and every one in {scope:?}")
                } else {
                    String::new()
                }
            ))
        }
    }

    /// A monotonicity row of a periodic f over the line (or one period):
    /// each piece true and ending where f turns or has a pole, and with
    /// their repeats by f's period they cover one period.
    fn judge_monotone_periodic(&self, scope: &Scope, value: &Value) -> Verdict {
        let per = self.per.as_ref().expect("periodic");
        let Some(list) = value.as_array() else {
            return Verdict::False("unreadable".into());
        };
        let mut pieces: Vec<(End, End, Option<bool>)> = Vec::new();
        for it in list {
            let on = &it["on"];
            let (Some(lo), Some(hi)) = (end_of(&on["lo"], true), end_of(&on["hi"], false)) else {
                return Verdict::False("unreadable".into());
            };
            let Some(dir) = dir_of(&it["dir"]) else {
                return Verdict::False("unreadable".into());
            };
            pieces.push((lo, hi, dir));
        }
        let mut choices: Vec<Vec<(Ext, Ext)>> = Vec::new();
        for (lo, hi, dir) in &pieces {
            if let Err(why) = self.piece_true(lo, hi, *dir) {
                return Verdict::False(format!("the piece {lo:?} to {hi:?} {dir:?}: {why}"));
            }
            if lo.is_none() || hi.is_none() {
                return Verdict::False("an infinite piece of a periodic f".into());
            }
            match self.piece_choices(lo, hi, *dir, &Scope::Line) {
                Ok(c) => choices.push(c),
                Err(why) => {
                    return Verdict::False(format!("the piece {lo:?} to {hi:?} {dir:?}: {why}"));
                }
            }
        }
        // One period from the scope's start, covered by a choice of each
        // piece's stretch moved a whole number of periods (closed: the
        // poles between are no points of f's). (Each piece's ends were
        // asked only of some place in each enclosure and the enclosures'
        // extents moved: the hole `judge_monotone` closed after review 24.)
        // (Any period's start will do: the repeats are the same. A start
        // at −∞, which `real_enc` lets through, reached `qd`: review 25.)
        let s0 = match scope {
            Scope::Period(a, _) if a.is_finite() => *a,
            _ => 0.0,
        };
        let period = per.trig.period();
        let start = Ext::At(Place::At(qd(s0)));
        let end = Ext::At(pi_place(
            Rational::from(&period.0 + &qd(s0)),
            period.1.clone(),
        ));
        match covering(&choices, &start, &end, Some(&period)) {
            Some(true) => Verdict::True,
            Some(false) => Verdict::False(
                "the pieces and their repeats, each from a place f's pieces end at to another, \
                 leave part of a period out"
                    .into(),
            ),
            None => Verdict::Unknown("too many readings of the pieces' ends".into()),
        }
    }

    /// The least period: P, for a periodic f; none for any other (a
    /// constant has every period and no least: none too).
    fn judge_period(&self, value: &Value) -> Verdict {
        // (An enclosure with a NaN end held f's period by `within`: round
        // 25's audit.)
        match (&self.per, real_enc_of(value)) {
            (None, None) if value.is_null() => Verdict::True,
            (None, _) => Verdict::False("a period listed for an f with none".into()),
            (Some(_), None) => Verdict::False("no period listed for a periodic f".into()),
            (Some(per), Some((lo, hi))) => {
                let (q0, q1) = per.trig.period();
                let p = pi_place(q0, q1);
                if p.within(lo, hi) {
                    Verdict::True
                } else {
                    Verdict::False(format!(
                        "the period listed in [{lo:e}, {hi:e}] is not f's, {:e}",
                        p.approx()
                    ))
                }
            }
        }
    }

    /// f's range on its domain, as its components in order, each end
    /// infinite or the least (greatest) of f's values at some places.
    fn range_parts(&self) -> Vec<(REnd, REnd)> {
        if let Some(per) = &self.per {
            // sin, cos: [C − |A|, C + |A|], taken at the turns; tan, cot:
            // the line.
            if matches!(per.trig.func, TrigFn::Tan | TrigFn::Cot) {
                return vec![(REnd::Inf, REnd::Inf)];
            }
            let at = |max: bool| REnd::Val {
                at: self
                    .extrema
                    .iter()
                    .filter(|f| f.max == Some(max))
                    .map(|f| f.at.clone())
                    .collect(),
                attained: true,
            };
            return vec![(at(false), at(true))];
        }
        let inside_point = || -> Place {
            Place::At(match &self.dom {
                Dom::From(c) => Rational::from(c + 1u32),
                Dom::Upto(c) => Rational::from(c - 1u32),
                Dom::Hole(c) => Rational::from(c + 1u32),
                Dom::Line | Dom::Poles { .. } => qi(0),
            })
        };
        if self.constant() {
            let v = REnd::Val {
                at: vec![inside_point()],
                attained: true,
            };
            return vec![(v.clone(), v)];
        }
        // Where f may be least or greatest: its critical points inside
        // the domain (its corners among them), a closed end.
        let mut cands: Vec<Place> = self.events[1]
            .iter()
            .map(|(p, _)| p.clone())
            .filter(|p| self.dom.inside(p))
            .collect();
        if let Dom::From(c) | Dom::Upto(c) = &self.dom {
            cands.push(Place::At(c.clone()));
        }
        // Which way f goes at each infinite end of its domain (its
        // outermost pieces').
        let (first, last) = (
            &self.d[0].polys[0],
            self.d[0].polys.last().expect("a piece"),
        );
        let right = last.lead().cmp0() == Greater;
        let left = (first.lead().cmp0() == Greater) != (first.degree() % 2 == 1);
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
        let v = self.value_q(0, c).expect("f is continuous");
        let g = self.d[0].sub_const(&v);
        let elsewhere = g.inner_roots().iter().any(|r| r.cmp(c) != Equal)
            || g.cuts
                .iter()
                .enumerate()
                .any(|(j, q)| q != c && g.polys[j].sign(q) == Equal);
        if elsewhere {
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
        let below = cands.iter().any(|p| self.cmp_value(0, p, &v) == Less);
        let above = cands.iter().any(|p| self.cmp_value(0, p, &v) == Greater);
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
                // (An infinite end of the enclosure is no bound.)
                let not_below =
                    |p: &Place| !x.0.is_finite() || self.cmp_value(0, p, &qd(x.0)) != Less;
                let not_above =
                    |p: &Place| !x.1.is_finite() || self.cmp_value(0, p, &qd(x.1)) != Greater;
                let ok = if low {
                    at.iter().all(not_below) && at.iter().any(not_above)
                } else {
                    at.iter().all(not_above) && at.iter().any(not_below)
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
        // (Its ends read from the outermost pieces' leading terms: a
        // constant outer piece, a plateau, would read as unbounded.)
        if self.plateau() {
            return Verdict::Unknown("a plateau's range is not modelled".into());
        }
        let mut pieces: Vec<(End, End)> = Vec::new();
        for p in list {
            let (Some(lo), Some(hi)) = (end_of(&p["lo"], true), end_of(&p["hi"], false)) else {
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
        // (Which piece is which part the row doesn't say: some one-to-one
        // reading must fit. Matched in the order of their low ends' doubles
        // alone, a low end enclosed from −∞ tied with "NegInf" and kept its
        // place: x³ with a hole at 0, the upper piece listed first from
        // [−∞, 0], was false. Review 25.)
        let fits = |(lo, hi): &(REnd, REnd), (pl, ph): &(End, End)| {
            self.end_holds(lo, pl, true)
                .map_err(|why| format!("low end: {why}"))?;
            self.end_holds(hi, ph, false)
                .map_err(|why| format!("high end: {why}"))
        };
        let options: Vec<Vec<usize>> = parts
            .iter()
            .map(|part| {
                (0..pieces.len())
                    .filter(|j| fits(part, &pieces[*j]).is_ok())
                    .collect()
            })
            .collect();
        match matched(&options) {
            None => Verdict::True,
            Some(i) => Verdict::False(match fits(&parts[i], &pieces[i]) {
                Err(why) => why,
                Ok(()) => format!("no piece of its own for part {} of the range", i + 1),
            }),
        }
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
    /// A periodic f's window of one period (the genuine rows', or one
    /// from −0.37·P), and the facts every row of it rests on: the
    /// simplifier's period, the domain's excluded families.
    pwin: Option<(f64, f64)>,
    facts: Vec<Value>,
    /// The refuter's: the doubles at and beside each of the truth's
    /// zeros of f, f′, f″, f‴, cuts, poles and domain end, and the atoms,
    /// in order; f's tree at each double sampled; what it found of each
    /// claim.
    marks: Vec<f64>,
    samples: RefCell<HashMap<u64, Sample>>,
    refuted: RefCell<HashMap<String, Option<String>>>,
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
    let Some(excluded) = v["excluded"].as_array() else {
        return false;
    };
    let fams_ok = match dom {
        // (One family: the poles, its x0 one of them, its period theirs.)
        Dom::Poles { per, .. } => match excluded.as_slice() {
            [e] => {
                let x0 = real_enc_of(&e["x0"]).filter(|(a, b)| a.is_finite() && b.is_finite());
                let pe = real_enc_of(&e["period"]);
                x0.is_some_and(|(a, b)| !dom.members(a, b).is_empty())
                    && pe.is_some_and(|(a, b)| pi_place(per.0.clone(), per.1.clone()).within(a, b))
            }
            _ => false,
        },
        _ => excluded.is_empty(),
    };
    if !fams_ok {
        return false;
    }
    let Some(pieces) = v["pieces"].as_array() else {
        return false;
    };
    let at = |b: &Value, low: bool, c: &Rational, closed: bool| -> bool {
        end_of(b, low).is_some_and(|e| {
            e.is_some_and(|(x, cl)| cl == closed && Place::At(c.clone()).within(x.0, x.1))
        })
    };
    let inf = |b: &Value, s: &str| b.as_str() == Some(s);
    match (dom, pieces.as_slice()) {
        (Dom::Line | Dom::Poles { .. }, [p]) => inf(&p["lo"], "NegInf") && inf(&p["hi"], "PosInf"),
        (Dom::From(c), [p]) => at(&p["lo"], true, c, true) && inf(&p["hi"], "PosInf"),
        (Dom::Upto(c), [p]) => inf(&p["lo"], "NegInf") && at(&p["hi"], false, c, true),
        (Dom::Hole(c), [p, q]) => {
            inf(&p["lo"], "NegInf")
                && at(&p["hi"], false, c, false)
                && at(&q["lo"], true, c, false)
                && inf(&q["hi"], "PosInf")
        }
        _ => false,
    }
}

fn context(fun: Fun) -> Result<Ctx, String> {
    let opts = CompileOptions {
        trig_unit: if fun.trig.as_ref().is_some_and(|t| t.deg) {
            TrigUnit::Degrees
        } else {
            TrigUnit::Radians
        },
        ..CompileOptions::default()
    };
    let a = certify_text(&fun.text, opts, DEFAULT_BUDGET, None)
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
    // A periodic f's truth over four periods and more either side of 0,
    // its atoms within three (and the certifier's window of one period
    // within one of 0): every box and window made of atoms lies in the
    // span. (Atoms 8 past the last feature in a span of three periods of
    // ½·tan(2x)° made a monotone piece run past it, read as false.)
    let mut reach = f64::INFINITY;
    let truth = match &fun.trig {
        Some(t) => {
            let (q0, q1) = t.period();
            let pf = pi_place(q0, q1).approx();
            reach = 3.0 * pf;
            Truth::new_trig(t, (-4.0 * pf - 32.0, 4.0 * pf + 32.0))
        }
        None => Truth::new(&fun.f, &fun.dom),
    };
    // (A Certified domain row that says otherwise is a false genuine row,
    // the panel's, and fails the run; it was only counted. One withheld
    // is counted: the audit after review 24.)
    if !domain_as_built(&genuine["domain"], &truth.dom) {
        let certified = genuine["domain"].get("Certified").is_some();
        return Err(format!(
            "the domain row is {}: {}",
            if certified { "false" } else { "not as built" },
            genuine["domain"]
        ));
    }
    let fx = replay::function(&genuine)?;
    let mut near: Vec<f64> = Vec::new();
    let mut about = |lo: f64, hi: f64, n: i32| {
        for i in 0..=n {
            near.push(doubles_out(lo, -i));
            near.push(doubles_out(hi, i));
        }
    };
    for k in 0..3 {
        for (r, _) in &truth.events[k] {
            if r.approx().abs() <= reach {
                let (lo, hi) = r.clone().enc();
                about(lo, hi, 2);
            }
        }
    }
    for p in fun.planted.iter().chain(fun.dom.point()) {
        about(floor_double(p), ceil_double(p), 1);
    }
    if let Some(per) = &truth.per {
        for mut m in truth
            .dom
            .members(-reach.min(-per.span.0), reach.min(per.span.1))
        {
            let (lo, hi) = m.enc();
            about(lo, hi, 1);
        }
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
    for p in truth.d.iter().flat_map(|d| &d.polys) {
        if p.degree() >= 1 {
            let lead = p.lead();
            for c in &p.0 {
                far = far.max(1.0 + Rational::from(c / &lead).abs().to_f64());
            }
        }
    }
    for c in &truth.d[0].cuts {
        far = far.max(1.0 + c.to_f64().abs());
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
    // A periodic f's window of one period, and the facts its rows rest on.
    let (mut pwin, mut facts) = (None, Vec::new());
    if let Some(per) = &truth.per {
        for row in LISTS {
            let Some(body) = genuine[row].as_object().and_then(|o| o.values().next()) else {
                continue;
            };
            if let Some(w) = body["cert"]["covers"].get("Period") {
                pwin = Some((
                    replay::r(&w["a"]).unwrap_or(0.0),
                    replay::r(&w["b"]).unwrap_or(0.0),
                ));
            }
            for c in body["cert"]["claims"].as_array().into_iter().flatten() {
                let fact = c.get("Simplifier").is_some_and(|f| {
                    f["fact"]
                        .as_str()
                        .is_some_and(|t| t.starts_with("f(x + P) = f(x)"))
                });
                if (fact || c.get("Family").is_some()) && !facts.contains(c) {
                    facts.push(c.clone());
                }
            }
        }
        if pwin.is_none() {
            let (q0, q1) = per.trig.period();
            let pf = pi_place(q0, q1).approx();
            pwin = Some((-0.37 * pf, 0.63 * pf));
        }
    }
    // The refuter's marks: two doubles either side of each of the truth's
    // places (three of the domain's end), and the atoms.
    let mut marks = atoms.clone();
    let mut about = |lo: f64, hi: f64, n: i32| {
        for i in 0..=n {
            marks.push(doubles_out(lo, -i));
            marks.push(doubles_out(hi, i));
        }
    };
    for k in 0..4 {
        for (r, _) in &truth.events[k] {
            let (lo, hi) = r.clone().enc();
            about(lo, hi, 2);
        }
    }
    if let Some(c) = truth.dom.point() {
        about(floor_double(c), ceil_double(c), 3);
    }
    if let Some(per) = &truth.per {
        for mut m in truth.dom.members(per.span.0, per.span.1) {
            let (lo, hi) = m.enc();
            about(lo, hi, 2);
        }
    }
    marks.retain(|x| x.is_finite());
    marks.sort_by(f64::total_cmp);
    marks.dedup();
    Ok(Ctx {
        fun,
        truth,
        base,
        genuine,
        fx,
        session: replay::Session::default(),
        atoms,
        near,
        pwin,
        facts,
        marks,
        samples: RefCell::new(HashMap::new()),
        refuted: RefCell::new(HashMap::new()),
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
    /// The domain row withheld too: nothing says where f is defined but
    /// the row's own claims (`rows::in_domain`).
    undecided: bool,
}

impl Case {
    fn json(&self, base: &Value) -> Value {
        let mut v = base.clone();
        v[self.row] = json!({self.scope.status(): {"value": self.value.clone(),
            "cert": {"covers": self.scope.covers(), "claims": self.claims.clone()}}});
        if self.row == "range" {
            v["range_ends"] = self.ends.clone();
        }
        if self.undecided {
            v["domain"] = json!({"Unknown": {"reason": "withheld"}});
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

impl Truth {
    /// f's value at the place, enclosed in doubles.
    fn value_enc(&self, p: &Place) -> (f64, f64) {
        if let Some(per) = &self.per {
            // (At a zero, a turn or an inflection f is A·T + C with T
            // rational there.)
            let t = &per.trig;
            if let Some(sv) = t.s_of(p)
                && let Ok(Some(v)) = t.tk_exact(0, &sv)
            {
                let y = Rational::from(&t.amp * &v) + &t.c;
                return (floor_double(&y), ceil_double(&y));
            }
            return match p {
                Place::At(x) if qd(x.to_f64()) == *x => {
                    self.val_enc(0, x.to_f64()).expect("no pole")
                }
                _ => panic!("f's value at {p:?}"),
            };
        }
        let (i, _) = self.d[0].at_place(p);
        value_enc(&self.d[0].polys[i], p)
    }
}

/// f's value at the place, enclosed in doubles (f the piece's polynomial
/// there).
fn value_enc(f: &Poly, p: &Place) -> (f64, f64) {
    match p {
        Place::At(x) => {
            let v = f.eval(x);
            (floor_double(&v), ceil_double(&v))
        }
        Place::Pi(..) => unreachable!("a polynomial at q0 + q1·π"),
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
            return self.truth.val_enc(0, a);
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
        for (r, _) in &self.truth.events[k] {
            let (lo, hi) = r.clone().enc();
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
                    if let Some(w) = self.pwin
                        && g.rng.chance(0.6)
                    {
                        Scope::Period(w.0, w.1)
                    } else if g.rng.chance(0.5) {
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
                    Scope::Window(a, b) | Scope::Period(a, b) => g.cover(*a, *b),
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
            claims.extend(self.facts.iter().cloned());
            let mut seen = std::collections::HashSet::new();
            claims.retain(|c| seen.insert(c.to_string()));
            out.push(Case {
                row,
                scope,
                value,
                claims,
                ends: Value::Null,
                undecided: false,
            });
        }
        out
    }

    fn make_case(&self, rng: &mut Rng) -> Case {
        let (row, k) = *rng.pick(&[
            ROWS[0], ROWS[1], ROWS[1], ROWS[1], ROWS[2], ROWS[2], ROWS[3],
        ]);
        let scope = match rng.below(10) {
            0..=2 => match self.pwin {
                Some(w) if rng.chance(0.8) => Scope::Period(w.0, w.1),
                _ => Scope::Line,
            },
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
            Scope::Period(a, b) => g.cover(*a, *b),
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
        claims.extend(self.facts.iter().cloned());
        let mut seen = std::collections::HashSet::new();
        claims.retain(|c| seen.insert(c.to_string()));
        Case {
            row,
            scope,
            value,
            claims,
            ends: Value::Null,
            undecided: false,
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
        match self.pwin {
            Some((a, b)) => g.cover(a, b),
            None => g.cover(f64::NEG_INFINITY, f64::INFINITY),
        }
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
        if !self.truth.constant() && self.truth.per.is_none() {
            let polys = &self.truth.d[0].polys;
            let (first, last) = (&polys[0], &polys[polys.len() - 1]);
            let up_right = last.lead().cmp0() == Greater;
            let up_left = (first.lead().cmp0() == Greater) != (first.degree() % 2 == 1);
            for (right, up) in [(true, up_right), (false, up_left)] {
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
        claims.extend(self.facts.iter().cloned());
        let mut seen = std::collections::HashSet::new();
        claims.retain(|c| seen.insert(c.to_string()));
        Case {
            row: "range",
            scope: match self.pwin {
                Some((a, b)) => Scope::Period(a, b),
                None => Scope::Line,
            },
            value,
            claims,
            ends,
            undecided: false,
        }
    }

    /// A period row: the genuine row's claims (the simplifier's fact, f′'s
    /// bounds over a period, values apart for each prime; or what shows f
    /// has none), its value P, or changed: P halved, doubled, moved a
    /// double, none; a claim dropped, the fact's period halved.
    fn make_period_case(&self, rng: &mut Rng) -> Case {
        let body = self.genuine["period"]
            .as_object()
            .and_then(|o| o.get("Certified"))
            .cloned();
        let mut claims: Vec<Value> = body
            .as_ref()
            .and_then(|b| b["cert"]["claims"].as_array().cloned())
            .unwrap_or_default();
        let mut value = match &self.truth.per {
            Some(per) => {
                let (q0, q1) = per.trig.period();
                let (lo, hi) = pi_place(q0, q1).enc();
                enc(lo, hi)
            }
            None => Value::Null,
        };
        match rng.below(10) {
            0 | 1 => {
                if let Some((lo, hi)) = enc_of(&value) {
                    let k = *rng.pick(&[0.5, 2.0, 1.0 / 3.0]);
                    value = enc(lo * k, hi * k);
                } else {
                    value = enc(360.0, 360.0);
                    claims.push(json!({"Simplifier": {"fact":
                        "f(x + 360) = f(x) wherever f is defined"}}));
                }
            }
            2 => {
                if let Some((lo, hi)) = enc_of(&value) {
                    let d = if rng.chance(0.5) { 1 } else { -1 };
                    value = enc(doubles_out(lo, d), doubles_out(hi, d));
                }
            }
            3 => value = Value::Null,
            4 | 5 if !claims.is_empty() => {
                let i = rng.below(claims.len());
                claims.remove(i);
            }
            6 => {
                // The fact's period halved, and the value with it.
                for c in &mut claims {
                    if let Some(f) = c["Simplifier"]["fact"].as_str()
                        && let Some(rest) = f.strip_prefix("f(x + ")
                        && let Some((q, tail)) = rest.split_once(')')
                    {
                        let fact = format!("f(x + ({q})/2){tail}");
                        c["Simplifier"]["fact"] = json!(fact);
                        if let Some((lo, hi)) = enc_of(&value) {
                            value = enc(lo / 2.0, hi / 2.0);
                        }
                    }
                }
            }
            _ => {}
        }
        Case {
            row: "period",
            scope: Scope::Line,
            value,
            claims,
            ends: Value::Null,
            undecided: false,
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
                None => vals.push((x, self.truth.value_enc(&p), false)),
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

    /// A list row of a periodic f: over the line (or one period) one
    /// period's items as families every P (the certifier's), elsewhere
    /// those in the window or some, alone or (sometimes) as families;
    /// then changed: an item dropped, made up, moved a double, two
    /// merged, a kind flipped, a family's period doubled, halved, moved a
    /// double or the shift its items repeat by, a family listed alone, a
    /// first member moved half a period, a family again every 2P.
    fn list_row_periodic(
        &self,
        rng: &mut Rng,
        row: &str,
        scope: &Scope,
        claims: &mut Vec<Value>,
    ) -> Value {
        let per = self.truth.per.as_ref().expect("periodic");
        let (q0, q1) = per.trig.period();
        let pp = pi_place(q0.clone(), q1.clone());
        let pf = pp.approx();
        let pe = pp.clone().enc();
        let (a, b, fam) = match scope {
            Scope::Line | Scope::Period(..) => {
                let s0 = match scope {
                    Scope::Period(a, _) => *a,
                    _ => self.pwin.map_or(0.0, |w| w.0),
                };
                (s0, s0 + pf, true)
            }
            Scope::Window(a, b) => (*a, *b, rng.chance(0.2)),
            Scope::Points => {
                let Scope::Window(a, b) = self.window(rng, false) else {
                    unreachable!("a window")
                };
                (a, b, rng.chance(0.3))
            }
        };
        let mut feats = per.feats_in(row, a, b);
        if scope.whole() {
            // (One period: its far end is its near end's repeat.)
            let (s0, s1) = pq(&Place::At(qd(a)));
            let end = pi_place(s0 + &q0, s1 + &q1);
            feats.retain(|(p, _, _)| pcmp(p, &end) == Less);
        }
        if *scope == Scope::Points {
            feats.retain(|_| rng.chance(0.6));
        }
        // (x's enclosure, the family's period, a maximum?)
        type Listed = ((f64, f64), Option<(f64, f64)>, Option<bool>);
        let mut chosen: Vec<Listed> = feats
            .into_iter()
            .map(|(mut p, _, m)| (p.enc(), fam.then_some(pe), m))
            .collect();
        let shift = per.dx(&per.shift_s(row));
        let se = pi_place(shift.0, shift.1).enc();
        let n = chosen.len();
        match rng.below(14) {
            0 if n > 0 => {
                chosen.remove(rng.below(n));
            }
            1 => {
                let x = *rng.pick(&self.near);
                chosen.push(((x, x), fam.then_some(pe), Some(rng.chance(0.5))));
            }
            2 if n > 0 => {
                let i = rng.below(n);
                let d = if rng.chance(0.5) { 1 } else { -1 };
                chosen[i].0 = (doubles_out(chosen[i].0.0, d), doubles_out(chosen[i].0.1, d));
            }
            3 if n >= 2 => {
                let i = rng.below(n - 1);
                let (x, y) = (chosen[i].0, chosen[i + 1].0);
                chosen[i].0 = (x.0.min(y.0), x.1.max(y.1));
                chosen.remove(i + 1);
            }
            4 if n > 0 => {
                let i = rng.below(n);
                chosen[i].2 = chosen[i].2.map(|m| !m);
            }
            5 if n > 0 => {
                let i = rng.below(n);
                chosen[i].1 = Some((pe.0 * 2.0, pe.1 * 2.0));
            }
            6 if n > 0 => {
                let i = rng.below(n);
                chosen[i].1 = Some((pe.0 / 2.0, pe.1 / 2.0));
            }
            7 if n > 0 => {
                let i = rng.below(n);
                let d = if rng.chance(0.5) { 1 } else { -1 };
                chosen[i].1 = Some((doubles_out(pe.0, d), doubles_out(pe.1, d)));
            }
            8 if n > 0 => {
                let i = rng.below(n);
                chosen[i].1 = Some(se);
            }
            9 if n > 0 => {
                let i = rng.below(n);
                chosen[i].1 = if chosen[i].1.is_some() {
                    None
                } else {
                    Some(pe)
                };
            }
            10 if n > 0 => {
                let i = rng.below(n);
                let h = pf / 2.0;
                chosen[i].0 = (chosen[i].0.0 + h, chosen[i].0.1 + h);
                let x = chosen[i].0;
                chosen[i].0 = (x.0.next_down(), x.1.next_up());
            }
            11 if n > 0 => {
                // The same family again, every 2P from P on.
                let i = rng.below(n);
                let mut again = chosen[i];
                again.0 = ((again.0.0 + pf).next_down(), (again.0.1 + pf).next_up());
                again.1 = Some((pe.0 * 2.0, pe.1 * 2.0));
                chosen.push(again);
            }
            _ => {}
        }
        chosen.sort_by(|a, b| a.0.0.total_cmp(&b.0.0));
        let items: Vec<Value> = chosen
            .iter()
            .map(|&((lo, hi), every, max)| {
                let every_v = every.map_or(Value::Null, |(a, b)| enc(a, b));
                if row == "x_intercepts" {
                    return match every {
                        Some((pa, pb)) => {
                            json!({"Every": {"x0": enc(lo, hi), "period": enc(pa, pb)}})
                        }
                        None => json!({"At": enc(lo, hi)}),
                    };
                }
                let y = self.y_of(lo, hi);
                if let Some((ylo, yhi)) = y {
                    claims.push(c_value(0, lo, hi, ylo, yhi));
                }
                let (ylo, yhi) = y.unwrap_or((0.0, 0.0));
                let mut item = json!({"x": enc(lo, hi), "y": enc(ylo, yhi), "every": every_v});
                if row == "extrema" {
                    item["kind"] = json!(if max.unwrap_or(false) { "Max" } else { "Min" });
                }
                item
            })
            .collect();
        Value::Array(items)
    }

    fn list_row(&self, rng: &mut Rng, row: &str, scope: &Scope, claims: &mut Vec<Value>) -> Value {
        if self.truth.per.is_some() {
            return self.list_row_periodic(rng, row, scope, claims);
        }
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
        self.mono_row_x(rng, scope, true)
    }

    /// The monotone pieces over the scope, changed (`change`) or true.
    fn mono_row_x(&self, rng: &mut Rng, scope: &Scope, change: bool) -> Value {
        // The pieces' ends in order: (enclosure, closed); None: infinite.
        let mut ends: Vec<End> = Vec::new();
        if let Some(per) = &self.truth.per
            && scope.whole()
        {
            // One period of pieces, from the first turn (or pole) at or
            // after the window's start to its repeat.
            let s0 = match scope {
                Scope::Period(a, _) => *a,
                _ => self.pwin.map_or(0.0, |w| w.0),
            };
            let (q0, q1) = per.trig.period();
            let first = self
                .truth
                .breaks
                .iter()
                .find(|b| b.cmp(&qd(s0)) != Less)
                .cloned();
            if let Some(first) = first {
                let (f0, f1) = pq(&first);
                let last = pi_place(f0 + q0, f1 + q1);
                for b in &self.truth.breaks {
                    if pcmp(b, &first) != Less && pcmp(b, &last) != Greater {
                        let x = b.clone().enc();
                        ends.push(Some((x, self.truth.dom.holds(b))));
                    }
                }
            }
            return self.mono_pieces(rng, ends, change);
        }
        let (sa, sb) = match scope {
            Scope::Window(a, b) | Scope::Period(a, b) => (*a, *b),
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
                ends.push(Some((x, dom.holds(&b))));
            }
        }
        ends.push(end);
        self.mono_pieces(rng, ends, change)
    }

    /// Monotone pieces between each two of `ends` (each in its direction),
    /// then changed: a direction flipped, two joined, one cut at an atom,
    /// an end moved a double, one dropped, an end opened or closed.
    fn mono_pieces(&self, rng: &mut Rng, ends: Vec<End>, change: bool) -> Value {
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
            let s = match self.truth.sign_q(1, &m) {
                Some(Equal) | None => self.truth.beside_sign(1, &m, true),
                Some(s) => s,
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
        match if change { rng.below(12) } else { 0 } {
            4 if n > 0 => {
                let i = rng.below(n);
                pieces[i].2 = flip(pieces[i].2);
            }
            10 if n > 0 => {
                // Constant where f is not.
                let i = rng.below(n);
                pieces[i].2 = "Constant";
            }
            11 if n >= 3 => {
                // Two ends one: the piece between them dropped, its
                // neighbours meeting at their hull (an end holding two
                // turns).
                let i = rng.below(n - 2);
                if let (Some((x, c)), Some((y, _))) = (pieces[i].1, pieces[i + 1].1) {
                    let hull = Some(((x.0.min(y.0), x.1.max(y.1)), c));
                    pieces[i].1 = hull;
                    pieces[i + 2].0 = hull;
                    pieces.remove(i + 1);
                }
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

// ------------------------------------------------------------ the refuter

/// The refuter's working precision, in bits.
const REFUTE_PREC: u32 = 256;

/// What MPFR evaluation of f's own tree shows at one double: whether f is
/// defined there (None: not decided), and f, f′, f″, f‴ there, enclosed,
/// where shown to exist.
#[derive(Clone)]
struct Sample {
    def: Option<bool>,
    d: [Option<(Float, Float)>; 4],
}

/// An enclosure's sign, if it shows one (`Equal`: exactly 0).
fn enc_sign(v: &(Float, Float)) -> Option<Ordering> {
    if v.0 > 0 {
        Some(Greater)
    } else if v.1 < 0 {
        Some(Less)
    } else if v.0 == 0 && v.1 == 0 {
        Some(Equal)
    } else {
        None
    }
}

/// Two enclosed reals in order, if their enclosures show it (`Equal`:
/// both the one point).
fn enc_cmp(a: &(Float, Float), b: &(Float, Float)) -> Option<Ordering> {
    if a.1 < b.0 {
        Some(Less)
    } else if a.0 > b.1 {
        Some(Greater)
    } else if a.0 == a.1 && b.0 == b.1 && a.0 == b.0 {
        Some(Equal)
    } else {
        None
    }
}

fn sign_word(o: Ordering) -> &'static str {
    match o {
        Greater => "positive",
        Less => "negative",
        Equal => "0",
    }
}

fn show_enc(v: &(Float, Float)) -> String {
    format!("[{:e}, {:e}]", v.0.to_f64(), v.1.to_f64())
}

/// Whether some member x0 + k·period of a family (each enclosed) may meet
/// [a, b]: false only where none can (a stretch narrower than half a
/// period, the members about it each apart from it, exactly).
fn member_meets(x0: (f64, f64), per: (f64, f64), a: f64, b: f64) -> bool {
    let finite = [x0.0, x0.1, per.0, per.1, a, b]
        .iter()
        .all(|v| v.is_finite());
    if !finite || per.0 <= 0.0 || b - a >= per.0 / 2.0 {
        return true;
    }
    let mid = ((a / 2.0 + b / 2.0) - (x0.0 / 2.0 + x0.1 / 2.0)) / (per.0 / 2.0 + per.1 / 2.0);
    if !mid.is_finite() || mid.abs() > 1e15 {
        return true;
    }
    let k0 = mid.round() as i64;
    (k0 - 2..=k0 + 2).any(|k| {
        let kq = qi(k);
        let (pl, ph) = if k >= 0 {
            (per.0, per.1)
        } else {
            (per.1, per.0)
        };
        let lo = qd(x0.0) + Rational::from(&kq * &qd(pl));
        let hi = qd(x0.1) + Rational::from(&kq * &qd(ph));
        lo <= qd(b) && qd(a) <= hi
    })
}

/// A second check on the truth, by sampling: each claim and each row the
/// truth calls true is tried against f's own tree, evaluated by the
/// replay's MPFR series (`replay::iv`, written from the language's
/// specification; nothing of the truth's exact model, its polynomials,
/// pieces and roots, is read but where to look). It samples a grid over
/// the claim's box or the row's scope (40 doubles evenly by value, 16 by
/// the order of the doubles, its ends and three doubles in from each) and
/// every double within two of each of the truth's zeros of f, f′, f″ and
/// f‴, cuts, poles, and within three of the domain's end, and refutes only
/// what an enclosure shows: a strict sign or a monotone order broken at
/// samples, a listed zero, turn or inflection where f⁽ᵏ⁾ is shown nonzero
/// (or a box where it is shown away from 0), two items at one double, a
/// sign change of f⁽ᵏ⁾ between two samples (f continuous across: a zero,
/// a turn; f′ existing across: an inflection) that no listed item or
/// family member meets in a complete row, a sampled point where f is
/// defined that no monotone piece holds, or none of the direction f′ shows
/// there, a value of f in no piece of the range, f(x + P) apart from f(x)
/// for every P in the period's enclosure, and the truth's domain against
/// f's definedness. It confirms nothing. (Assumed of f, as of every
/// function the fuzzer makes: no plateau, so a sign change of f′ between
/// samples is a strict turn; f′ existing across a box is continuous.)
impl Ctx {
    /// f's tree at the double x (cached).
    fn sample(&self, x: f64) -> Sample {
        if let Some(s) = self.samples.borrow().get(&x.to_bits()) {
            return s.clone();
        }
        let keep = replay::iv::prec();
        replay::iv::set_prec(REFUTE_PREC);
        let s = self.fx.series(&self.fx.f, x, x, 3);
        replay::iv::set_prec(keep);
        let def = match s.first() {
            Some(c) if c.empty => Some(false),
            Some(c) if c.def => Some(true),
            _ => None,
        };
        // (s[k] encloses f⁽ᵏ⁾/k!.)
        let fact = [1u32, 1, 2, 6];
        let d = std::array::from_fn(|k| {
            let c = s.get(k)?;
            (def == Some(true) && !c.empty && c.def && c.bounded()).then(|| {
                (
                    Float::with_val_round(REFUTE_PREC, &c.lo * fact[k], Round::Down).0,
                    Float::with_val_round(REFUTE_PREC, &c.hi * fact[k], Round::Up).0,
                )
            })
        });
        let out = Sample { def, d };
        self.samples.borrow_mut().insert(x.to_bits(), out.clone());
        out
    }

    /// f's series over the box [a, b] to order n (f⁽ᵏ⁾/k!, each with
    /// where it exists).
    fn over(&self, a: f64, b: f64, n: usize) -> replay::series::S {
        let keep = replay::iv::prec();
        replay::iv::set_prec(REFUTE_PREC);
        let s = self.fx.series(&self.fx.f, a, b, n);
        replay::iv::set_prec(keep);
        s
    }

    /// The doubles tried over [a, b] (finite).
    fn grid(&self, a: f64, b: f64) -> Vec<f64> {
        if !(a.is_finite() && b.is_finite() && a <= b) {
            return Vec::new();
        }
        let mut out = vec![a, b];
        for i in 1..=3 {
            out.push(doubles_out(a, i));
            out.push(doubles_out(b, -i));
        }
        for i in 1..40 {
            let t = f64::from(i) / 40.0;
            out.push(a * (1.0 - t) + b * t);
        }
        let (oa, ob) = (i128::from(ord(a)), i128::from(ord(b)));
        for i in 1..16 {
            out.push(unord((oa + (ob - oa) * i / 16) as i64));
        }
        let lo = self.marks.partition_point(|m| *m < a);
        let hi = self.marks.partition_point(|m| *m <= b);
        out.extend_from_slice(&self.marks[lo..hi.max(lo)]);
        out.retain(|x| x.is_finite() && a <= *x && *x <= b);
        out.sort_by(f64::total_cmp);
        out.dedup();
        out
    }

    /// The doubles tried on the tail from `from` (finite): it, three
    /// doubles out, then ever farther, and every mark on it.
    fn tail_grid(&self, from: f64, right: bool) -> Vec<f64> {
        if !from.is_finite() {
            return Vec::new();
        }
        let s = if right { 1.0 } else { -1.0 };
        let mut out = vec![from];
        for i in 1..=3 {
            out.push(doubles_out(from, if right { i } else { -i }));
        }
        let base = 1.0 + from.abs();
        for j in [
            -6, -3, -1, 0, 1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 128, 256, 512, 1000,
        ] {
            out.push(from + s * base * 2f64.powi(j));
        }
        out.extend(
            self.marks
                .iter()
                .filter(|m| if right { **m >= from } else { **m <= from }),
        );
        out.retain(|x| x.is_finite() && if right { *x >= from } else { *x <= from });
        out.sort_by(f64::total_cmp);
        out.dedup();
        out
    }

    /// The doubles tried over [a, b], either end infinite: a grid over its
    /// finite part about the marks, tails beyond.
    fn region(&self, a: f64, b: f64) -> Vec<f64> {
        if a.is_nan() || b.is_nan() || a > b {
            return Vec::new();
        }
        let m0 = self.marks.first().copied().unwrap_or(-1.0);
        let m1 = self.marks.last().copied().unwrap_or(1.0);
        let lo = if a.is_finite() {
            a
        } else {
            m0.min(if b.is_finite() { b } else { m0 }) - 1.0
        };
        let hi = if b.is_finite() { b } else { m1.max(lo) + 1.0 };
        let mut out = self.grid(lo, hi);
        if !a.is_finite() {
            out.extend(self.tail_grid(lo, false));
        }
        if !b.is_finite() {
            out.extend(self.tail_grid(hi, true));
        }
        out.sort_by(f64::total_cmp);
        out.dedup();
        out
    }

    fn scope_samples(&self, scope: &Scope) -> Vec<f64> {
        match scope {
            Scope::Window(a, b) => self.region(*a, *b),
            Scope::Line | Scope::Period(..) => self.region(f64::NEG_INFINITY, f64::INFINITY),
            Scope::Points => Vec::new(),
        }
    }

    /// f undefined at one of the doubles.
    fn refute_defined(&self, xs: &[f64]) -> Option<String> {
        xs.iter()
            .find(|x| self.sample(**x).def == Some(false))
            .map(|x| format!("f is undefined at {x:e}"))
    }

    /// f⁽ᵏ⁾ strictly on side `above` of 0 at the doubles (`skip` aside),
    /// f defined at each.
    fn refute_sign(&self, k: usize, xs: &[f64], above: bool, skip: Option<f64>) -> Option<String> {
        let want = if above { Greater } else { Less };
        for &x in xs {
            if Some(x) == skip {
                continue;
            }
            let s = self.sample(x);
            if s.def == Some(false) {
                return Some(format!("f is undefined at {x:e}"));
            }
            if let Some(v) = &s.d[k]
                && let Some(o) = enc_sign(v)
                && o != want
            {
                return Some(format!("f⁽{k}⁾({x:e}) is {}", sign_word(o)));
            }
        }
        None
    }

    /// f⁽ᵏ⁾ strictly monotone over the doubles (in order): its values in
    /// one order, never equal, and f⁽ᵏ⁺¹⁾ never of both strict signs.
    fn refute_monotone(&self, k: usize, xs: &[f64]) -> Option<String> {
        let (mut up, mut down) = (None, None);
        let vals: Vec<(f64, (Float, Float))> = xs
            .iter()
            .filter_map(|x| self.sample(*x).d[k].clone().map(|v| (*x, v)))
            .collect();
        for w in vals.windows(2) {
            match enc_cmp(&w[0].1, &w[1].1) {
                Some(Equal) => {
                    return Some(format!(
                        "f⁽{k}⁾ is {} at both {:e} and {:e}",
                        show_enc(&w[0].1),
                        w[0].0,
                        w[1].0
                    ));
                }
                Some(Less) => up = up.or(Some((w[0].0, w[1].0))),
                Some(Greater) => down = down.or(Some((w[0].0, w[1].0))),
                None => {}
            }
        }
        if k < 3 {
            for &x in xs {
                match self.sample(x).d[k + 1].as_ref().and_then(enc_sign) {
                    Some(Greater) => up = up.or(Some((x, x))),
                    Some(Less) => down = down.or(Some((x, x))),
                    _ => {}
                }
            }
        }
        let ((a, b), (c, d)) = (up?, down?);
        Some(format!(
            "f⁽{k}⁾ rises at [{a:e}, {b:e}] and falls at [{c:e}, {d:e}]"
        ))
    }

    /// A claim the truth calls true, refuted (cached by the claim).
    fn refute_claim(&self, c: &Claim) -> Option<String> {
        let key = format!("{c:?}");
        if let Some(r) = self.refuted.borrow().get(&key) {
            return r.clone();
        }
        let r = self.refute_claim_now(c);
        self.refuted.borrow_mut().insert(key, r.clone());
        r
    }

    fn refute_claim_now(&self, c: &Claim) -> Option<String> {
        let order = |of: &Subject| match of {
            Subject::F(k) if *k <= 2 => Some(*k),
            _ => None,
        };
        match c {
            Claim::Defined(x) | Claim::Continuous(x) | Claim::Kink(x) => {
                self.refute_defined(&self.region(x.0, x.1))
            }
            Claim::Undefined(x) => self
                .region(x.0, x.1)
                .into_iter()
                .find(|p| self.sample(*p).def == Some(true))
                .map(|p| format!("f is defined at {p:e}")),
            Claim::Beyond { x, of, above, .. } | Claim::Factors { x, of, above, .. } => {
                self.refute_sign(order(of)?, &self.region(x.0, x.1), *above, None)
            }
            Claim::NoCross { x, of, above, .. } => {
                let k = order(of)?;
                let xs = self.region(x.0, x.1);
                self.refute_sign(k, &xs, *above, None)
                    .or_else(|| self.refute_monotone(k, &xs))
            }
            Claim::TailBeyond {
                side,
                from,
                of,
                above,
                ..
            } => self.refute_sign(
                order(of)?,
                &self.tail_grid(*from, *side == Side::Right),
                *above,
                None,
            ),
            Claim::TailChain {
                side,
                from,
                of,
                above,
                order: m,
                ..
            } => {
                let k = order(of)?;
                let right = *side == Side::Right;
                let xs = self.tail_grid(*from, right);
                self.refute_sign(k, &xs, *above, None).or_else(|| {
                    // (The chain, where the samples reach its order: f to
                    // f‴.)
                    let top = k + m;
                    if *m == 0 || top > 3 {
                        return None;
                    }
                    let sigma = if right || m.is_multiple_of(2) {
                        *above
                    } else {
                        !*above
                    };
                    self.refute_sign(top, &xs, sigma, None).or_else(|| {
                        let s = self.sample(*from);
                        (0..*m).find_map(|i| {
                            let want = if right {
                                sigma
                            } else {
                                sigma == (m - i).is_multiple_of(2)
                            };
                            let o = enc_sign(s.d[k + i].as_ref()?)?;
                            (o != if want { Greater } else { Less })
                                .then(|| format!("f⁽{}⁾({from:e}) is {}", k + i, sign_word(o)))
                        })
                    })
                })
            }
            Claim::OneCross { x, of, .. } => {
                let k = order(of)?;
                let xs = self.grid(x.0, x.1);
                let end = |p: f64| self.sample(p).d[k].as_ref().and_then(enc_sign);
                if let (Some(a), Some(b)) = (end(x.0), end(x.1))
                    && (a == Equal || b == Equal || a == b)
                {
                    return Some(format!(
                        "f⁽{k}⁾ is {} at {:e} and {} at {:e}",
                        sign_word(a),
                        x.0,
                        sign_word(b),
                        x.1
                    ));
                }
                self.refute_monotone(k, &xs)
                    .or_else(|| self.refute_changes(k, &xs, 1))
            }
            Claim::ExactAt { x, of, at, .. } => {
                let k = order(of)?;
                let xs = self.grid(x.0, x.1);
                if let Some(o) = self.sample(*at).d[k].as_ref().and_then(enc_sign)
                    && o != Equal
                {
                    return Some(format!("f⁽{k}⁾({at:e}) is {}", sign_word(o)));
                }
                if let Some(p) = xs.iter().find(|p| {
                    **p != *at && self.sample(**p).d[k].as_ref().and_then(enc_sign) == Some(Equal)
                }) {
                    return Some(format!("f⁽{k}⁾ is 0 at {p:e} too"));
                }
                let left: Vec<f64> = xs.iter().copied().filter(|p| p < at).collect();
                let right: Vec<f64> = xs.iter().copied().filter(|p| p > at).collect();
                self.refute_monotone(k, &xs)
                    .or_else(|| self.refute_changes(k, &left, 0))
                    .or_else(|| self.refute_changes(k, &right, 0))
            }
            Claim::Touch {
                x, of, at, above, ..
            } => {
                let k = order(of)?;
                if let Some(o) = self.sample(*at).d[k].as_ref().and_then(enc_sign)
                    && o != Equal
                {
                    return Some(format!("f⁽{k}⁾({at:e}) is {}", sign_word(o)));
                }
                self.refute_sign(k, &self.grid(x.0, x.1), *above, Some(*at))
            }
            Claim::Value { x, of, lo, hi } => {
                let k = order(of)?;
                for p in self.region(x.0, x.1) {
                    let s = self.sample(p);
                    if s.def == Some(false) {
                        return Some(format!("f is undefined at {p:e}"));
                    }
                    if let Some(v) = &s.d[k]
                        && (v.1 < *lo || v.0 > *hi)
                    {
                        return Some(format!(
                            "f⁽{k}⁾({p:e}) is in {}, not [{lo:e}, {hi:e}]",
                            show_enc(v)
                        ));
                    }
                }
                None
            }
            Claim::GapClear { x, of } => {
                let k = order(of)?;
                let xs: Vec<f64> = self
                    .grid(x.0, x.1)
                    .into_iter()
                    .filter(|p| x.0 < *p && *p < x.1)
                    .collect();
                let shown: Vec<(f64, Ordering)> = xs
                    .iter()
                    .filter_map(|p| Some((*p, enc_sign(self.sample(*p).d[k].as_ref()?)?)))
                    .collect();
                if let (Some(z), Some(n)) = (
                    shown.iter().find(|(_, o)| *o == Equal),
                    shown.iter().find(|(_, o)| *o != Equal),
                ) {
                    return Some(format!(
                        "f⁽{k}⁾ is 0 at {:e} and {} at {:e} inside the gap",
                        z.0,
                        sign_word(n.1),
                        n.0
                    ));
                }
                self.refute_changes(k, &xs, 0)
            }
            Claim::KinkSides {
                x,
                at,
                order: k,
                left,
                right,
                ..
            } => {
                if !(1..=2).contains(k) {
                    return None;
                }
                let want = |s: &KinkSign| match s {
                    KinkSign::Pos => Greater,
                    KinkSign::Neg => Less,
                    KinkSign::Zero => Equal,
                };
                let xs = self.grid(x.0, x.1);
                if let Some(why) = self.refute_defined(&xs) {
                    return Some(why);
                }
                let signs: Vec<(f64, Ordering)> = xs
                    .iter()
                    .filter_map(|p| Some((*p, enc_sign(self.sample(*p).d[*k].as_ref()?)?)))
                    .collect();
                // (Wrong on the left of c, or on the right. As reals: −0
                // is 0, no side of it.)
                let wrong = |c: &dyn Fn(f64) -> Ordering| {
                    signs.iter().find(|(p, o)| match c(*p) {
                        Less => *o != want(left),
                        Greater => *o != want(right),
                        Equal => false,
                    })
                };
                let side = |p: f64, c: f64| p.partial_cmp(&c).unwrap_or(Equal);
                match at {
                    Some(c) => wrong(&|p: f64| side(p, *c)).map(|(p, o)| {
                        format!("f⁽{k}⁾({p:e}) is {}, on its side of {c:e}", sign_word(*o))
                    }),
                    None => {
                        // (Refuted only if wrong wherever c may be: at each
                        // sample inside, and between each two.)
                        let inside: Vec<f64> = xs
                            .iter()
                            .copied()
                            .filter(|p| x.0 < *p && *p < x.1)
                            .collect();
                        let at_sample = |c: f64| wrong(&move |p: f64| side(p, c)).is_some();
                        let between = |i: usize| {
                            wrong(&|p: f64| if p <= xs[i] { Less } else { Greater }).is_some()
                        };
                        (inside.iter().all(|c| at_sample(*c))
                            && (0..xs.len().saturating_sub(1)).all(between))
                        .then(|| format!("f⁽{k}⁾'s signs over the box fit no point between"))
                    }
                }
            }
            Claim::KinkAt {
                x, at, left, right, ..
            } => {
                let xs = self.grid(x.0, x.1);
                let lefts: Vec<f64> = xs.iter().copied().filter(|p| p < at).collect();
                let rights: Vec<f64> = xs.iter().copied().filter(|p| p > at).collect();
                let mut l = lefts.clone();
                l.push(*at);
                let mut r = vec![*at];
                r.extend(&rights);
                self.refute_sign(1, &lefts, *left, None)
                    .or_else(|| self.refute_sign(1, &rights, *right, None))
                    .or_else(|| self.refute_monotone(0, &l))
                    .or_else(|| self.refute_monotone(0, &r))
            }
            _ => None,
        }
    }

    /// More than `most` changes of f⁽ᵏ⁾'s sign over the doubles (in order),
    /// each across a stretch where f⁽ᵏ⁾ is continuous: so more zeros.
    fn refute_changes(&self, k: usize, xs: &[f64], most: usize) -> Option<String> {
        let shown: Vec<(f64, Ordering)> = xs
            .iter()
            .filter_map(|p| Some((*p, enc_sign(self.sample(*p).d[k].as_ref()?)?)))
            .filter(|(_, o)| *o != Equal)
            .collect();
        let mut n = 0;
        for w in shown.windows(2) {
            if w[0].1 != w[1].1 && self.continuous_over(k, w[0].0, w[1].0) {
                n += 1;
                if n > most {
                    return Some(format!(
                        "f⁽{k}⁾ changes sign {n} times, the last between {:e} and {:e}",
                        w[0].0, w[1].0
                    ));
                }
            }
        }
        None
    }

    /// f⁽ᵏ⁾ shown to exist throughout [a, b], and continuous there (f by
    /// its decoration; f⁽ᵏ⁾, k ≥ 1, as f⁽ᵏ⁺¹⁾ exists throughout).
    fn continuous_over(&self, k: usize, a: f64, b: f64) -> bool {
        let s = self.over(a, b, if k == 0 { 0 } else { k + 1 });
        let ok = |c: &replay::iv::Iv| !c.empty && c.def;
        s.first().is_some_and(|c| ok(c) && c.cont)
            && (k == 0 || s.iter().take(k + 2).skip(1).all(ok))
    }

    /// A row the truth calls true, refuted.
    fn refute_row(&self, row: &str, scope: &Scope, value: &Value) -> Option<String> {
        match row {
            "monotonicity" => self.refute_monotone_row(scope, value),
            "range" => self.refute_range(value),
            "period" => self.refute_period(value),
            "x_intercepts" | "extrema" | "inflections" => self.refute_list(row, scope, value),
            _ => None,
        }
    }

    fn refute_list(&self, row: &str, scope: &Scope, value: &Value) -> Option<String> {
        let k = row_order(row);
        let what = match k {
            0 => "zero",
            1 => "turn",
            _ => "inflection",
        };
        let Some(items) = items_of(row, value) else {
            return Some("an item that holds no real, or of no kind".into());
        };
        // Each place listed: an item alone, or a family's members about
        // the marks (each enclosed, rounded out).
        let (m0, m1) = (
            self.marks.first().copied().unwrap_or(-1.0),
            self.marks.last().copied().unwrap_or(1.0),
        );
        let mut places: Vec<((f64, f64), usize)> = Vec::new();
        for (j, it) in items.iter().enumerate() {
            let Some(per) = it.every else {
                places.push((it.x, j));
                continue;
            };
            let finite = [it.x.0, it.x.1, per.0, per.1].iter().all(|v| v.is_finite());
            if !finite || per.0 <= 0.0 {
                continue;
            }
            let p = per.0 / 2.0 + per.1 / 2.0;
            let k_lo = ((m0 - it.x.1) / p).floor() - 1.0;
            let k_hi = ((m1 - it.x.0) / p).ceil() + 1.0;
            if !(k_lo.is_finite() && k_hi.is_finite()) || k_hi - k_lo > 2000.0 {
                continue;
            }
            for kk in k_lo as i64..=k_hi as i64 {
                let kq = qi(kk);
                let (pl, ph) = if kk >= 0 {
                    (per.0, per.1)
                } else {
                    (per.1, per.0)
                };
                let lo = qd(it.x.0) + Rational::from(&kq * &qd(pl));
                let hi = qd(it.x.1) + Rational::from(&kq * &qd(ph));
                places.push(((floor_double(&lo), ceil_double(&hi)), j));
            }
        }
        for ((lo, hi), j) in &places {
            let it = &items[*j];
            if lo == hi {
                let s = self.sample(*lo);
                if s.def == Some(false) {
                    return Some(format!("a {what} listed at {lo:e}, where f is undefined"));
                }
                if let (Some(y), Some(v)) = (it.y, &s.d[0])
                    && (v.1 < y.0 || v.0 > y.1)
                {
                    return Some(format!(
                        "f({lo:e}) is in {}, not the item's y [{:e}, {:e}]",
                        show_enc(v),
                        y.0,
                        y.1
                    ));
                }
                // (A turn or an inflection at a domain end or beside a
                // hole needs no zero of f⁽ᵏ⁾: asked only where f is
                // defined from the double below to the one above.)
                if let Some(o) = s.d[k].as_ref().and_then(enc_sign)
                    && o != Equal
                {
                    let inner = || {
                        let b = self.over(lo.next_down(), lo.next_up(), 0);
                        b.first().is_some_and(|c| !c.empty && c.def)
                    };
                    if k == 0 || inner() {
                        return Some(format!(
                            "a {what} listed at {lo:e}, where f⁽{k}⁾ is {}",
                            sign_word(o)
                        ));
                    }
                }
                // (A smooth turn there of the other kind: f′ exactly 0,
                // f″ of one strict sign.)
                if k == 1
                    && let Some(max) = it.max
                    && s.d[1].as_ref().and_then(enc_sign) == Some(Equal)
                    && let Some(o) = s.d[2].as_ref().and_then(enc_sign)
                    && o != Equal
                    && (o == Less) != max
                {
                    return Some(format!(
                        "a {} listed at {lo:e}, where f′ is 0 and f″ {}",
                        if max { "maximum" } else { "minimum" },
                        sign_word(o)
                    ));
                }
            } else if lo.is_finite() && hi.is_finite() {
                let (a, b) = if k == 0 {
                    (*lo, *hi)
                } else {
                    (lo.next_down(), hi.next_up())
                };
                let s = self.over(a, b, k);
                let f_def = s.first().is_some_and(|c| !c.empty && c.def && c.cont);
                if f_def
                    && let Some(c) = s.get(k)
                    && !c.empty
                    && c.def
                    && c.ne0()
                {
                    return Some(format!(
                        "a {what} listed in [{lo:e}, {hi:e}], where f⁽{k}⁾ is away from 0"
                    ));
                }
                if let (Some(y), Some(c)) = (it.y, s.first())
                    && f_def
                    && (c.hi < y.0 || c.lo > y.1)
                {
                    return Some(format!("f over [{lo:e}, {hi:e}] misses the item's y"));
                }
            }
        }
        // Two items at one double: one place, listed twice.
        let mut points: Vec<(f64, usize)> = places
            .iter()
            .filter(|((lo, hi), _)| lo == hi)
            .map(|((lo, _), j)| (*lo, *j))
            .collect();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        for w in points.windows(2) {
            if w[0].0 == w[1].0 && w[0].1 != w[1].1 {
                return Some(format!("two items at the one double {:e}", w[0].0));
            }
        }
        if !scope.complete() {
            return None;
        }
        // Complete: each change of f⁽ᵏ⁾'s sign between two samples, across
        // a stretch where it can't jump, holds a listed place.
        let held = |a: f64, b: f64| {
            items.iter().any(|it| match it.every {
                None => it.x.0 <= b && a <= it.x.1,
                Some(per) => member_meets(it.x, per, a, b),
            })
        };
        let xs = self.scope_samples(scope);
        let shown: Vec<(f64, Ordering)> = xs
            .iter()
            .filter_map(|p| Some((*p, enc_sign(self.sample(*p).d[k].as_ref()?)?)))
            .collect();
        if k == 0
            && let Some((p, _)) = shown.iter().find(|(p, o)| *o == Equal && !held(*p, *p))
        {
            return Some(format!("f({p:e}) = 0, and no item holds it"));
        }
        for w in shown.windows(2) {
            let ((p, a), (q, b)) = (w[0], w[1]);
            if a == Equal || b == Equal || a == b || held(p, q) {
                continue;
            }
            // (A zero of f where f is continuous; a turn where f is; an
            // inflection where f′ exists throughout.)
            let s = self.over(p, q, k.min(1));
            let ok = |c: &replay::iv::Iv| !c.empty && c.def;
            let across =
                s.first().is_some_and(|c| ok(c) && c.cont) && (k < 2 || s.get(1).is_some_and(ok));
            if across {
                return Some(format!(
                    "f⁽{k}⁾ is {} at {p:e} and {} at {q:e}, and no item lies between",
                    sign_word(a),
                    sign_word(b)
                ));
            }
        }
        None
    }

    fn refute_monotone_row(&self, scope: &Scope, value: &Value) -> Option<String> {
        let unreadable = || Some("a piece that holds no real, or of no direction".to_string());
        let mut pieces: Vec<(End, End, Option<bool>)> = Vec::new();
        let Some(list) = value.as_array() else {
            return unreadable();
        };
        for it in list {
            let on = &it["on"];
            let (Some(lo), Some(hi), Some(dir)) = (
                end_of(&on["lo"], true),
                end_of(&on["hi"], false),
                dir_of(&it["dir"]),
            ) else {
                return unreadable();
            };
            pieces.push((lo, hi, dir));
        }
        let word = |d: Option<bool>| match d {
            Some(true) => "increasing",
            Some(false) => "decreasing",
            None => "constant",
        };
        // Each piece over the reals between its ends' enclosures: f
        // defined there, f′ never of the other sign, its values in order.
        for (lo, hi, dir) in &pieces {
            let a = lo.map_or(f64::NEG_INFINITY, |(x, _)| x.1);
            let b = hi.map_or(f64::INFINITY, |(x, _)| x.0);
            let want = match dir {
                Some(true) => Less,
                Some(false) => Greater,
                None => Equal,
            };
            let mut last: Option<(f64, (Float, Float))> = None;
            for x in self.region(a, b).into_iter().filter(|x| a < *x && *x < b) {
                let s = self.sample(x);
                if s.def == Some(false) {
                    return Some(format!(
                        "a piece {} over {x:e}, where f is undefined",
                        word(*dir)
                    ));
                }
                if let Some(o) = s.d[1].as_ref().and_then(enc_sign) {
                    let bad = match dir {
                        Some(true) => o == Less,
                        Some(false) => o == Greater,
                        None => o != Equal,
                    };
                    if bad {
                        return Some(format!(
                            "f′({x:e}) is {} in a piece {}",
                            sign_word(o),
                            word(*dir)
                        ));
                    }
                }
                if let Some(v) = &s.d[0] {
                    if let Some((p, u)) = &last
                        && let Some(o) = enc_cmp(u, v)
                        && o != want
                    {
                        return Some(format!(
                            "f is {} at {p:e} and {} at {x:e}, in a piece {}",
                            show_enc(u),
                            show_enc(v),
                            word(*dir)
                        ));
                    }
                    last = Some((x, v.clone()));
                }
            }
        }
        if !scope.complete() {
            return None;
        }
        // Complete: every sampled double of the scope where f is defined
        // lies in some piece's extent, and where f′ is shown strictly
        // signed, in one of that direction (a monotone piece holding a
        // point where f′ > 0, even at its end, is increasing). A periodic
        // f's over the line: the extents repeated by its period, widened.
        let pf = self.truth.per.as_ref().filter(|_| scope.whole()).map(|p| {
            let (q0, q1) = p.trig.period();
            pi_place(q0, q1).approx()
        });
        let holds = |(lo, hi, _): &(End, End, Option<bool>), x: f64| -> bool {
            let a = lo.map_or(f64::NEG_INFINITY, |(e, _)| e.0);
            let b = hi.map_or(f64::INFINITY, |(e, _)| e.1);
            match pf {
                None => a <= x && x <= b,
                Some(pf) => {
                    if !(a.is_finite() && b.is_finite()) {
                        return true;
                    }
                    let k = ((x - a) / pf).floor();
                    [k - 1.0, k, k + 1.0].iter().any(|k| {
                        let (sa, sb) = (a + k * pf, b + k * pf);
                        let w = (sa.abs() + sb.abs() + pf) * 1e-12;
                        sa - w <= x && x <= sb + w
                    })
                }
            }
        };
        // (A point of the scope where f is defined needs a piece only on a
        // stretch of them: where the scope meets the domain in one point,
        // f is monotone on no stretch of it, and no piece is listed. Shown
        // so: f defined over the box to the next double, in the scope.)
        let in_scope = |y: f64| match scope {
            Scope::Window(a, b) => *a <= y && y <= *b,
            _ => true,
        };
        let stretch = |x: f64| {
            [(x, x.next_up()), (x.next_down(), x)].iter().any(|(a, b)| {
                in_scope(*a)
                    && in_scope(*b)
                    && self
                        .over(*a, *b, 0)
                        .first()
                        .is_some_and(|c| !c.empty && c.def)
            })
        };
        for x in self.scope_samples(scope) {
            let s = self.sample(x);
            if s.def != Some(true) {
                continue;
            }
            let covering: Vec<&(End, End, Option<bool>)> =
                pieces.iter().filter(|p| holds(p, x)).collect();
            if covering.is_empty() && stretch(x) {
                return Some(format!("{x:e}, where f is defined, is in no piece"));
            }
            if let Some(o) = s.d[1].as_ref().and_then(enc_sign)
                && o != Equal
                && !covering.iter().any(|p| p.2 == Some(o == Greater))
                && stretch(x)
            {
                return Some(format!(
                    "f′({x:e}) is {}, and no piece holding it is {}",
                    sign_word(o),
                    word(Some(o == Greater))
                ));
            }
        }
        None
    }

    fn refute_range(&self, value: &Value) -> Option<String> {
        let Some(pieces) = value.as_array().and_then(|l| {
            l.iter()
                .map(|p| Some((end_of(&p["lo"], true)?, end_of(&p["hi"], false)?)))
                .collect::<Option<Vec<(End, End)>>>()
        }) else {
            return Some("a range piece whose end holds no real".into());
        };
        // (v may lie in the piece: false only where its enclosure shows
        // it past an end.)
        let may = |(lo, hi): &(End, End), v: &(Float, Float)| {
            let above = match lo {
                None => true,
                Some((e, true)) => v.1 >= e.0,
                Some((e, false)) => v.1 > e.0,
            };
            let below = match hi {
                None => true,
                Some((e, true)) => v.0 <= e.1,
                Some((e, false)) => v.0 < e.1,
            };
            above && below
        };
        for x in self.region(f64::NEG_INFINITY, f64::INFINITY) {
            if let Some(v) = &self.sample(x).d[0]
                && !pieces.iter().any(|p| may(p, v))
            {
                return Some(format!(
                    "f({x:e}) is in {}, in no piece of the range",
                    show_enc(v)
                ));
            }
        }
        None
    }

    fn refute_period(&self, value: &Value) -> Option<String> {
        // (No period listed: nothing to sample.)
        if value.is_null() {
            return None;
        }
        let Some((lo, hi)) = real_enc_of(value) else {
            return Some("a period that holds no real".into());
        };
        if !(lo.is_finite() && hi.is_finite()) {
            return None;
        }
        let xs = self.region(f64::NEG_INFINITY, f64::INFINITY);
        for &x in xs.iter().step_by((xs.len() / 24).max(1)) {
            let Some(v) = self.sample(x).d[0].clone() else {
                continue;
            };
            let (a, b) = ((x + lo).next_down(), (x + hi).next_up());
            if !(a.is_finite() && b.is_finite()) {
                continue;
            }
            let s = self.over(a, b, 0);
            if let Some(c) = s.first()
                && !c.empty
                && c.def
                && c.bounded()
                && (c.hi < v.0 || c.lo > v.1)
            {
                return Some(format!(
                    "f({x:e}) is in {}, f(x + P) for P in [{lo:e}, {hi:e}] in [{:e}, {:e}]",
                    show_enc(&v),
                    c.lo.to_f64(),
                    c.hi.to_f64()
                ));
            }
        }
        None
    }

    /// The truth's domain against f's own definedness at the samples.
    fn refute_domain(&self) -> Option<String> {
        for x in self.region(f64::NEG_INFINITY, f64::INFINITY) {
            let holds = self.truth.dom.holds_q(&qd(x));
            match self.sample(x).def {
                Some(false) if holds => {
                    return Some(format!(
                        "the truth's domain holds {x:e}, where f is undefined"
                    ));
                }
                Some(true) if !holds => {
                    return Some(format!(
                        "the truth's domain leaves out {x:e}, where f is defined"
                    ));
                }
                _ => {}
            }
        }
        None
    }

    /// A case whose row the truth calls true and the refuter refutes, cut
    /// down: its claims (neither reads them), then items dropped while it
    /// still is.
    fn minimise_refuted(&self, case: &Case) -> Case {
        let mut c = case.clone();
        c.claims.clear();
        let mut i = 0;
        while c.value.as_array().is_some_and(|a| i < a.len()) {
            let mut t = c.clone();
            t.value.as_array_mut().expect("a list").remove(i);
            if self.truth.judge(t.row, &t.scope, &t.value) == Verdict::True
                && self.refute_row(t.row, &t.scope, &t.value).is_some()
            {
                c = t;
            } else {
                i += 1;
            }
        }
        c
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
    /// Constructed or genuine rows this truth cannot judge. They fail a
    /// run, separately from false rows, so a coverage gap cannot pass.
    unjudged: Vec<String>,
    /// (accepted, true) by row.
    by_row: BTreeMap<String, [u64; 4]>,
    kept: BTreeMap<&'static str, u64>,
    /// The kinds of claims about kinks kept (`REPLAY_FUZZ_KINDS` prints
    /// every kind's).
    kept_kinds: BTreeMap<&'static str, u64>,
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
    /// Claims and rows the truth calls true that the refuter refutes:
    /// the truth's own errors, each failing the run.
    refuted: Vec<String>,
    /// Claims and rows the refuter tried, and the doubles it sampled f
    /// at.
    tried: u64,
    sampled: u64,
}

impl Tally {
    fn absorb(&mut self, o: Tally) {
        self.functions += o.functions;
        self.not_made += o.not_made;
        self.not_certified += o.not_certified;
        self.domain_not_as_built.extend(o.domain_not_as_built);
        self.cases += o.cases;
        self.unreadable += o.unreadable;
        self.unjudged.extend(o.unjudged);
        for (k, v) in o.by_row {
            let e = self.by_row.entry(k).or_default();
            for i in 0..4 {
                e[i] += v[i];
            }
        }
        for (k, v) in o.kept {
            *self.kept.entry(k).or_default() += v;
        }
        for (k, v) in o.kept_kinds {
            *self.kept_kinds.entry(k).or_default() += v;
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
        self.refuted.extend(o.refuted);
        self.tried += o.tried;
        self.sampled += o.sampled;
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
        println!("  rows not judged: {}", self.unjudged.len());
        println!(
            "  refuter: {} claims and rows the truth calls true tried ({} doubles sampled), {} \
             refuted",
            self.tried,
            self.sampled,
            self.refuted.len()
        );
        println!("  row: accepted true / accepted FALSE / refused true / refused false");
        for (row, v) in &self.by_row {
            println!("  {row:>13}: {} / {} / {} / {}", v[0], v[1], v[2], v[3]);
        }
        println!(
            "  premises kept: {:?}; dropped: {:?}",
            self.kept, self.dropped
        );
        let all_kinds = std::env::var_os("REPLAY_FUZZ_KINDS").is_some();
        let kinds: BTreeMap<_, _> = self
            .kept_kinds
            .iter()
            .filter(|(k, _)| all_kinds || k.starts_with("Kink") || **k == "Family")
            .collect();
        if !kinds.is_empty() {
            println!("  kept by kind: {kinds:?}");
        }
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
            ("TRUTH REFUTED", &self.refuted),
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
        "period",
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
            _ => match (covers.get("Window"), covers.get("Period")) {
                (Some(w), _) => Scope::Window(
                    replay::r(&w["a"]).unwrap_or(f64::NAN),
                    replay::r(&w["b"]).unwrap_or(f64::NAN),
                ),
                (_, Some(w)) => Scope::Period(
                    replay::r(&w["a"]).unwrap_or(f64::NAN),
                    replay::r(&w["b"]).unwrap_or(f64::NAN),
                ),
                _ => continue,
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
        // (Its status is part of what it says: Certified is all of them,
        // over the line or one period; Partial, over a window or points.
        // The truth read only the scope: the audit after review 24.)
        let verdict = if (status == "Certified") != scope.whole() {
            Verdict::False(format!("{status} over {covers}"))
        } else {
            ctx.truth.judge(row, &scope, &body["value"])
        };
        match verdict {
            Verdict::Unknown(why) => t.unjudged.push(format!(
                "genuine y={} {row}: truth cannot judge this row: {why}",
                ctx.fun.text
            )),
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
                t.tried += 1;
                if let Some(r) = ctx.refute_row(row, &scope, &body["value"]) {
                    t.refuted.push(format!(
                        "genuine y={} {row} {status} over {covers}: {}: the truth calls it true; \
                         refuted: {r}",
                        ctx.fun.text, body["value"]
                    ));
                }
                if let Some(why) = refused {
                    t.genuine_refused_true
                        .push(format!("y={} {row}: {why}", ctx.fun.text));
                }
            }
        }
    }
}

/// Which functions a run makes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// Polynomials (`make_fun`), with a closed domain end or a hole.
    Poly,
    /// Functions with corners (`make_kinked`).
    Kink,
    /// Periodic functions (`make_trig`).
    Trig,
    /// Each function of a kind drawn from its own stream: a run's i-th
    /// polynomial is the polynomial-only run's i-th.
    Mixed,
}

impl Kind {
    /// `REPLAY_FUZZ_KIND` (poly, kink, trig or mixed), mixed unless set.
    fn from_env() -> Kind {
        match std::env::var("REPLAY_FUZZ_KIND").as_deref() {
            Ok("poly") => Kind::Poly,
            Ok("kink") => Kind::Kink,
            Ok("trig") => Kind::Trig,
            Ok("mixed") | Err(_) => Kind::Mixed,
            Ok(other) => panic!("REPLAY_FUZZ_KIND={other}: poly, kink, trig or mixed"),
        }
    }
}

/// The kind of the i-th function of a run (its own stream, apart from
/// the function's: `Rng::new(seed, i, 0)` makes the same polynomial
/// whichever kinds a run mixes).
fn kind_of(seed: u64, i: u64, kind: Kind) -> Kind {
    match kind {
        Kind::Mixed => match Rng::new(seed, i, u64::MAX).below(10) {
            0..=4 => Kind::Poly,
            5..=7 => Kind::Kink,
            _ => Kind::Trig,
        },
        k => k,
    }
}

/// The i-th function of a run.
fn make_kind(seed: u64, i: u64, kind: Kind) -> Option<Fun> {
    let mut rng = Rng::new(seed, i, 0);
    match kind_of(seed, i, kind) {
        Kind::Poly | Kind::Mixed => (0..20).find_map(|_| make_fun(&mut rng)),
        Kind::Kink => (0..20).find_map(|_| make_kinked(&mut rng)),
        Kind::Trig => (0..20).find_map(|_| make_trig(&mut rng)),
    }
}

fn run_function(seed: u64, i: u64, cases: u64, kind: Kind) -> Tally {
    let Some(fun) = make_kind(seed, i, kind) else {
        return Tally {
            functions: 1,
            not_made: 1,
            ..Tally::default()
        };
    };
    // `REPLAY_FUZZ_TIMES`: each function's time and findings as it ends.
    let started = std::time::Instant::now();
    let text = fun.text.clone();
    if std::env::var_os("REPLAY_FUZZ_TIMES").is_some() {
        println!("function {i}: y={text}");
    }
    let t = run_fun(fun, seed, i, cases);
    if std::env::var_os("REPLAY_FUZZ_TIMES").is_some() {
        println!(
            "function {i} ({:.1} s): y={text}: {} cases, {} accepted false, {} false premises, \
             {} genuine false",
            started.elapsed().as_secs_f64(),
            t.cases,
            t.failures.len(),
            t.false_premises.len(),
            t.genuine_false.len()
        );
    }
    t
}

/// One function: its genuine certificate held to the truth, and `cases`
/// cases of made-up claims and rows.
fn run_fun(fun: Fun, seed: u64, i: u64, cases: u64) -> Tally {
    let mut t = Tally {
        functions: 1,
        ..Tally::default()
    };
    let mut ctx = match context(fun.clone()) {
        Ok(c) => c,
        Err(e) => {
            if e.starts_with("the domain row is false") {
                t.genuine_rows += 1;
                t.genuine_false.push(format!("y={}: {e}", fun.text));
            } else if e.starts_with("the domain row") {
                t.domain_not_as_built.push(format!("y={}: {e}", fun.text));
            } else {
                t.not_certified += 1;
            }
            return t;
        }
    };
    genuine(&mut ctx, &mut t);
    t.tried += 1;
    if let Some(r) = ctx.refute_domain() {
        t.refuted.push(format!("y={}: {r}", ctx.fun.text));
    }
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
        let mut made = match rng.below(100) {
            0..=49 => ctx.make_local_cases(&mut rng, 6),
            50..=59 => vec![ctx.make_range_case(&mut rng)],
            60..=65 => vec![ctx.make_period_case(&mut rng)],
            _ => vec![ctx.make_case(&mut rng)],
        };
        // (From a stream of their own: the domain withheld; a periodic
        // f's row without the fact of its period, or over less than one.)
        let mut also = Rng::new(seed, i, (j + 1) | (1 << 40));
        for case in &mut made {
            case.undecided = also.chance(0.06);
            if ctx.truth.per.is_some() {
                if also.chance(0.1) {
                    case.claims.retain(|c| !ctx.facts.contains(c));
                }
                if let Scope::Period(a, b) = case.scope
                    && also.chance(0.1)
                {
                    case.scope = Scope::Period(a, a + (b - a) / 2.0);
                }
            }
        }
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
                *t.kept_kinds.entry(c.claim.kind()).or_default() += 1;
                let truth = ctx.truth.claim_true(&c.claim);
                if truth == Some(true) {
                    t.tried += 1;
                    if let Some(r) = ctx.refute_claim(&c.claim) {
                        t.refuted.push(format!(
                            "seed {seed} function {i} case {j}: y={}: the truth calls {:?} true; \
                             refuted: {r}",
                            ctx.fun.text, c.claim
                        ));
                    }
                }
                if truth == Some(false) {
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
                (Verdict::Unknown(why), _) => {
                    t.unjudged
                        .push(format!("function {i} case {j}: {}", ctx.show(&case, why)));
                    continue;
                }
                (Verdict::True, true) => 0,
                (Verdict::False(_), true) => 1,
                (Verdict::True, false) => 2,
                (Verdict::False(_), false) => 3,
            };
            t.by_row.entry(case.row.to_string()).or_default()[slot] += 1;
            if verdict == Verdict::True {
                t.tried += 1;
                if let Some(r) = ctx.refute_row(case.row, &case.scope, &case.value) {
                    let small = ctx.minimise_refuted(&case);
                    let why = ctx
                        .refute_row(small.row, &small.scope, &small.value)
                        .unwrap_or(r);
                    t.refuted.push(format!(
                        "seed {seed} function {i} case {j}:\n{}",
                        ctx.show(&small, &format!("TRUE, refuted: {why}"))
                    ));
                }
            }
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
    t.sampled += ctx.samples.borrow().len() as u64;
    t
}

fn run(seed: u64, functions: u64, cases: u64, threads: u64, kind: Kind) -> Tally {
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
                        let t = run_function(seed, i, cases, kind);
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
        t.failures.is_empty()
            && t.false_premises.is_empty()
            && t.genuine_false.is_empty()
            && t.unjudged.is_empty()
            && t.refuted.is_empty(),
        "{} accepted false rows, {} proved false premises, {} genuine rows the truth refuses \
         ({} of them accepted by the replay); {} rows not judged; {} the truth calls true \
         refuted; the first: {}",
        t.failures.len(),
        t.false_premises.len(),
        t.genuine_false.len(),
        t.genuine_false_accepted.len(),
        t.unjudged.len(),
        t.refuted.len(),
        t.failures
            .first()
            .or(t.false_premises.first())
            .or(t.genuine_false.first())
            .or(t.unjudged.first())
            .or(t.refuted.first())
            .map_or("", String::as_str)
    );
}

// ------------------------------------------------------------ exhaustive

/// A fixed function: its text, its pieces (in x), its domain.
fn fixed(text: &str, cuts: &[Rational], polys: Vec<Poly>, dom: Dom) -> Fun {
    let f = Pw {
        cuts: cuts.to_vec(),
        polys,
    };
    let mut planted: Vec<Rational> = cuts.to_vec();
    planted.extend(dom.point().cloned());
    // (A polynomial with a domain end or a hole: its derivatives' trees,
    // as `make_fun` gives them.)
    let mut evaluated = Vec::new();
    if f.cuts.is_empty()
        && let Some(c) = dom.point()
    {
        // (In u = x − c, where its coefficients are short.)
        let d1 = f.polys[0].at_shift(&Rational::from(-c)).deriv();
        evaluated = vec![
            format!("f′ = {}", poly_formula(&d1, c).expect("written")),
            format!("f″ = {}", poly_formula(&d1.deriv(), c).expect("written")),
        ];
    }
    Fun {
        text: text.into(),
        evaluated,
        f: f.within(&dom),
        dom,
        planted,
        expanded: Vec::new(),
        trig: None,
    }
}

fn fixed_trig(text: &str, func: TrigFn, a: i64, b: i64, amp: i64, c: i64) -> Fun {
    Fun {
        text: text.into(),
        evaluated: Vec::new(),
        f: Pw::one(Poly::constant(qi(1))),
        dom: Dom::Line,
        planted: Vec::new(),
        expanded: Vec::new(),
        trig: Some(Trig {
            func,
            a: qi(a),
            b: qi(b),
            amp: qi(amp),
            c: qi(c),
            deg: true,
        }),
    }
}

/// The exhaustive mode's functions: the reviewers' (x² − 1 from a gap at
/// its turn, the turn with f′'s next zero a double off, alone and at a
/// closed end, (x − 1)² and (x − 1)³ at a point), the long runs' turn
/// beside a hole, zeros at adjacent doubles and between doubles; corners
/// at doubles and between them, a joined kink with an inflection, a kink
/// box that holds no corner, a corner at a hole and at a closed end;
/// sin, tan and a shifted sine in degrees. The CI slice takes the first
/// `CI_FUNS`.
fn exhaustive_funs() -> Vec<Fun> {
    let x = |c: i64| Poly::x_minus(&qi(c));
    let ulp = qd(2f64.powi(-52));
    let cubic = Poly::new(vec![qi(0), qi(0), Rational::from(&ulp * 3u32), qi(-2)]).at_shift(&qi(1));
    let third = Rational::from((1, 3));
    let near_hole = Poly::new(vec![
        third.clone(),
        qi(0),
        -qd(2f64.powi(-53)),
        third.clone(),
    ])
    .at_shift(&qi(-2));
    let sq = |p: &Poly| p.mul(p);
    let mut cubic_fun = fixed(
        "3*2^(-52)*(x-1)^2-2*(x-1)^3",
        &[],
        vec![cubic.clone()],
        Dom::Line,
    );
    cubic_fun.evaluated = vec!["f′ = Mul(Mul(6,Sub(x,1)),Sub(Pow(2,Neg(52)),Sub(x,1)))".into()];
    let mut cubic_end = fixed(
        "3*2^(-52)*(x-1)^2-2*(x-1)^3+0*sqrt(x-1)",
        &[],
        vec![cubic],
        Dom::From(qi(1)),
    );
    cubic_end.evaluated = cubic_fun.evaluated.clone();
    let x2m1 = Poly::new(vec![qi(-1), qi(0), qi(1)]);
    vec![
        fixed("x^2-1", &[], vec![x2m1.clone()], Dom::Line),
        cubic_fun,
        fixed("(x-1)^2", &[], vec![sq(&x(1))], Dom::Line),
        fixed(
            "abs(x-1)*(x-1)",
            &[qi(1)],
            vec![sq(&x(1)).scale(&qi(-1)), sq(&x(1))],
            Dom::Line,
        ),
        // (f″ ≡ 0 on the line's piece: KinkSides with a flat side.)
        fixed(
            "max(x^2,x)",
            &[qi(0), qi(1)],
            vec![sq(&x(0)), x(0), sq(&x(0))],
            Dom::Line,
        ),
        // (Two corners, each kink argument signed over the other's box.)
        fixed(
            "(x-1)^2+abs(x)-2*abs(x-1/3)",
            &[qi(0), third.clone()],
            {
                let c = |a: i64, b: Rational| sq(&x(1)).add(&Poly::new(vec![b, qi(a)]));
                vec![
                    c(1, Rational::from((-2, 3))),
                    c(3, Rational::from((-2, 3))),
                    c(-1, Rational::from((2, 3))),
                ]
            },
            Dom::Line,
        ),
        fixed(
            "abs((x-1)*(x+1))",
            &[qi(-1), qi(1)],
            vec![x2m1.clone(), x2m1.scale(&qi(-1)), x2m1.clone()],
            Dom::Line,
        ),
        cubic_end,
        fixed("(x-1)^3", &[], vec![x(1).pow(3)], Dom::Line),
        fixed(
            "(x-1)*(x-1-2^(-52))",
            &[],
            vec![x(1).mul(&Poly::x_minus(&Rational::from(&ulp + 1u32)))],
            Dom::Line,
        ),
        fixed(
            "x^2-1/9",
            &[],
            vec![Poly::new(vec![Rational::from((-1, 9)), qi(0), qi(1)])],
            Dom::Line,
        ),
        fixed(
            "1/3*(x+2)^3-2^(-53)*(x+2)^2+1/3+0/(x-(-9007199254740991*2^(-52)))",
            &[],
            vec![near_hole],
            Dom::Hole(qd(-2.0 + f64::EPSILON)),
        ),
        fixed(
            "(x-1)^2+2*abs(x-1)",
            &[qi(1)],
            vec![
                sq(&x(1)).sub(&x(1).scale(&qi(2))),
                sq(&x(1)).add(&x(1).scale(&qi(2))),
            ],
            Dom::Line,
        ),
        fixed(
            "x^3/3+abs(x-1/3)",
            std::slice::from_ref(&third),
            {
                let c = Poly::new(vec![qi(0), qi(0), qi(0), third.clone()]);
                let u = Poly::x_minus(&third);
                vec![c.sub(&u), c.add(&u)]
            },
            Dom::Line,
        ),
        fixed(
            "max(x^2,2*x-1)",
            &[],
            vec![Poly::new(vec![qi(0), qi(0), qi(1)])],
            Dom::Line,
        ),
        fixed(
            "abs(x-1)+0/(x-1)",
            &[qi(1)],
            vec![x(1).scale(&qi(-1)), x(1)],
            Dom::Hole(qi(1)),
        ),
        fixed(
            "abs(x-1)+(x-1)^2+0*sqrt(x-1)",
            &[qi(1)],
            vec![sq(&x(1)).sub(&x(1)), sq(&x(1)).add(&x(1))],
            Dom::From(qi(1)),
        ),
        fixed_trig("sin(x)", TrigFn::Sin, 1, 0, 1, 0),
        fixed_trig("tan(x)", TrigFn::Tan, 1, 0, 1, 0),
        fixed_trig("2*sin(3*x+30)+1", TrigFn::Sin, 3, 30, 2, 1),
    ]
}

/// The functions the CI slice takes.
const CI_FUNS: usize = 6;

/// What the exhaustive mode found and how much it tried.
#[derive(Default)]
struct Sweep {
    /// (function, row, place, pool size, combinations tried).
    places: Vec<(String, &'static str, f64, usize, u64)>,
    combos: u64,
    /// Rows the truth refuses among the variants, and true ones.
    false_rows: u64,
    true_rows: u64,
    failures: Vec<String>,
    genuine: Tally,
}

impl Ctx {
    /// The places a row's features sit at (or might): the zeros of f⁽ᵏ⁾
    /// (and its cuts with no value), the domain's end or hole, a
    /// periodic f's over one period.
    fn sweep_places(&self, k: usize) -> Vec<Place> {
        let mut out: Vec<Place> = self.truth.events[k]
            .iter()
            .map(|(p, _)| p.clone())
            .collect();
        if let Some(c) = self.truth.dom.point() {
            out.push(Place::At(c.clone()));
        }
        if let (Some(per), Some((a, _))) = (&self.truth.per, self.pwin) {
            let (q0, q1) = per.trig.period();
            let end = pi_place(q0 + qd(a), q1);
            out.retain(|p| p.cmp(&qd(a)) != Less && pcmp(p, &end) == Less);
        }
        out
    }

    /// The pool about a place: true claims, each proven by the replay's
    /// own check, about its doubles and the two either side, the gaps
    /// between, boxes from far off and from two doubles off ending on
    /// each, crossing and kink boxes about it. Each element is a claim or
    /// two that go together (a gap with its clearness).
    fn sweep_pool(&mut self, row: &'static str, k: usize, place: &Place) -> Vec<Vec<Value>> {
        let (lo, hi) = place.clone().enc();
        let mut pts = vec![doubles_out(lo, -2), lo.next_down(), lo];
        if hi != lo {
            pts.push(hi);
        }
        pts.extend([hi.next_up(), doubles_out(hi, 2)]);
        let left = self
            .atoms
            .iter()
            .copied()
            .rfind(|a| *a < doubles_out(lo, -3))
            .unwrap_or(doubles_out(lo, -3));
        let right = self
            .atoms
            .iter()
            .copied()
            .find(|a| *a > doubles_out(hi, 3))
            .unwrap_or(doubles_out(hi, 3));
        let t = &self.truth;
        let mut cands: Vec<Vec<Value>> = Vec::new();
        for &x in &pts {
            match t.sign_at(k, x) {
                _ if t.dom.misses_box(x, x) => cands.push(vec![c_box("Undefined", x, x)]),
                None => {
                    if k > 0 {
                        cands.push(vec![c_box("Kink", x, x)]);
                    }
                }
                Some(Equal) => {
                    cands.push(vec![c_value(k, x, x, 0.0, 0.0)]);
                    cands.push(vec![c_exact(k, x, x, x)]);
                    if k > 0 {
                        cands.push(vec![c_box("Kink", x, x)]);
                    }
                }
                Some(s) => {
                    cands.push(vec![c_sign("Beyond", k, x, x, s == Greater)]);
                    if let Some((a, b)) = t.val_enc(k, x) {
                        cands.push(vec![c_value(k, x, x, a, b)]);
                    }
                }
            }
        }
        for w in pts.windows(2) {
            if w[0] < w[1] {
                cands.push(vec![c_box("Gap", w[0], w[1]), c_clear(k, w[0], w[1])]);
                cands.push(vec![c_box("Gap", w[0], w[1])]);
            }
        }
        let (near_l, near_r) = (pts[0], pts[pts.len() - 1]);
        for &x in &pts {
            for (a, b) in [(left, x), (x, right), (near_l, x), (x, near_r)] {
                if a >= b {
                    continue;
                }
                if let Some(up) = t.sign_over(k, a, b) {
                    cands.push(vec![c_sign("Beyond", k, a, b, up)]);
                }
                if t.sign_at(k, x) == Some(Equal) {
                    let other = if a == x { b } else { a };
                    if let Some(s) = t.sign_at(k, other).filter(|s| *s != Equal) {
                        cands.push(vec![c_touch(k, a, b, x, s == Greater)]);
                    }
                    cands.push(vec![c_exact(k, a, b, x)]);
                }
                if k == 0 || row == "extrema" {
                    cands.push(vec![c_box("Continuous", a, b)]);
                }
            }
        }
        // About the place itself: crossing boxes (two overlapping too), a
        // kink's box and its sides.
        let n = pts.len();
        for (i, j) in [(0, n - 1), (1, n - 2), (0, n - 2), (1, n - 1)] {
            cands.push(vec![c_cross(k, pts[i], pts[j])]);
        }
        for (a, b) in [(pts[1], pts[n - 2]), (pts[0], pts[n - 1])] {
            if a < b && k > 0 {
                cands.push(vec![c_box("Kink", a, b)]);
                let at = (lo == hi && a < lo && lo < b).then_some(lo);
                let (sl, sr) = t.sides(k, place);
                cands.push(vec![c_sides(k, a, b, at, sl, sr, false)]);
                cands.push(vec![c_sides(k, a, b, None, sl, sr, false)]);
            }
        }
        let (a, b) = (pts[1], pts[n - 2]);
        if k > 0 {
            let at = (lo == hi && a < lo && lo < b).then_some(lo);
            let (sl, sr) = t.sides(k, place);
            cands.push(vec![c_box("Kink", a, b)]);
            cands.push(vec![c_sides(k, a, b, at, sl, sr, false)]);
            cands.push(vec![c_sides(k, a, b, at, sl, sr, k == 2 && at.is_some())]);
            cands.push(vec![c_sides(k, a, b, None, sl, sr, false)]);
            if k == 1
                && let Some(p) = at
                && sl != Equal
                && sr != Equal
            {
                cands.push(vec![c_kink_at(a, b, p, sl == Greater, sr == Greater)]);
            }
        }
        // True by the truth, and proven by the replay (each element on
        // its own).
        cands.retain(|el| {
            el.iter()
                .all(|c| replay::claim(c).is_ok_and(|cl| t.claim_true(&cl) == Some(true)))
        });
        let mut seen = std::collections::HashSet::new();
        cands.retain(|el| seen.insert(format!("{el:?}")));
        cands
            .into_iter()
            .filter(|el| {
                let case = Case {
                    row,
                    scope: Scope::Points,
                    value: json!([]),
                    claims: el.clone(),
                    ends: Value::Null,
                    undecided: false,
                };
                let v = case.json(&self.base);
                replay::replay_in(&v, &mut self.session, Some(row)).is_ok_and(|rep| {
                    rep.claims
                        .iter()
                        .all(|c| matches!(c.outcome.class, Class::Strong | Class::Weak))
                })
            })
            .take(SWEEP_POOL)
            .collect()
    }

    /// Rows about a place, every way: over the place, a window from it
    /// or to it, the doubles beside it, points (and one period of a
    /// periodic f); each true, and changed every way one change goes (an
    /// item dropped, moved a double either way, made up at the place or a
    /// double beside, of the other kind; a piece's direction flipped, two
    /// joined, an end moved). With the value claims its items need.
    fn sweep_rows(&self, row: &'static str, place: &Place) -> Vec<(Scope, Value, Vec<Value>)> {
        let (lo, hi) = place.clone().enc();
        let mut scopes = vec![
            Scope::Window(lo, hi),
            Scope::Window(lo, doubles_out(hi, 4)),
            Scope::Window(doubles_out(lo, -4), hi),
            Scope::Window(lo.next_down(), hi.next_up()),
            Scope::Points,
        ];
        if let Some((a, b)) = self.pwin {
            scopes.push(Scope::Period(a, b));
        }
        let mut out = Vec::new();
        for scope in scopes {
            if row == "monotonicity" {
                if matches!(scope, Scope::Window(a, b) if a == b) || scope == Scope::Points {
                    continue;
                }
                let base = self.mono_row_x(&mut Rng::new(0, 0, 0), &scope, false);
                let pieces = base.as_array().cloned().unwrap_or_default();
                out.push((scope.clone(), Value::Array(pieces.clone()), Vec::new()));
                for i in 0..pieces.len() {
                    let mut v = pieces.clone();
                    let d = v[i]["dir"].as_str().unwrap_or("");
                    v[i]["dir"] = json!(if d == "Increasing" {
                        "Decreasing"
                    } else {
                        "Increasing"
                    });
                    out.push((scope.clone(), Value::Array(v), Vec::new()));
                    let mut v = pieces.clone();
                    v.remove(i);
                    out.push((scope.clone(), Value::Array(v), Vec::new()));
                    for (end, d) in [("lo", 1), ("lo", -1), ("hi", 1), ("hi", -1)] {
                        let mut v = pieces.clone();
                        if let Some(x) = v[i]["on"][end].get_mut("At").map(|a| &mut a["x"])
                            && let Some((a, b)) = enc_of(x)
                        {
                            *x = enc(doubles_out(a, d), doubles_out(b, d));
                            out.push((scope.clone(), Value::Array(v), Vec::new()));
                        }
                    }
                    if i + 1 < pieces.len() {
                        let mut v = pieces.clone();
                        v[i]["on"]["hi"] = v[i + 1]["on"]["hi"].clone();
                        v.remove(i + 1);
                        out.push((scope.clone(), Value::Array(v), Vec::new()));
                    }
                }
                continue;
            }
            // The true items in the scope (about the place for points).
            let (a, b) = match &scope {
                Scope::Window(a, b) | Scope::Period(a, b) => (*a, *b),
                _ => (doubles_out(lo, -4), doubles_out(hi, 4)),
            };
            // (x's enclosure, a maximum?)
            type Listed = ((f64, f64), Option<bool>);
            let mut feats: Vec<Listed> = match &self.truth.per {
                Some(per) => per
                    .feats_in(row, a, b)
                    .into_iter()
                    .map(|(mut p, _, m)| (p.enc(), m))
                    .collect(),
                None => self
                    .truth
                    .features(row)
                    .into_iter()
                    .filter(|f| f.at.within(a, b))
                    .map(|mut f| (f.at.enc(), f.max))
                    .collect(),
            };
            if let (Scope::Period(..), Some(per)) = (&scope, &self.truth.per) {
                let (q0, q1) = per.trig.period();
                let end = pi_place(q0 + qd(a), q1);
                feats.retain(|f| Place::At(qd(f.0.0)).cmp(&qd(end.approx())) == Less);
            }
            let fam = matches!(scope, Scope::Period(..)).then(|| {
                let per = self.truth.per.as_ref().expect("periodic");
                let (q0, q1) = per.trig.period();
                pi_place(q0, q1).enc()
            });
            let mut variants: Vec<Vec<Listed>> = vec![feats.clone()];
            for i in 0..feats.len() {
                let mut v = feats.clone();
                v.remove(i);
                variants.push(v);
                for d in [1, -1] {
                    let mut v = feats.clone();
                    v[i].0 = (doubles_out(v[i].0.0, d), doubles_out(v[i].0.1, d));
                    variants.push(v);
                }
                if row == "extrema" {
                    let mut v = feats.clone();
                    v[i].1 = v[i].1.map(|m| !m);
                    variants.push(v);
                }
            }
            for x in [lo.next_down(), lo, hi.next_up()] {
                for m in [Some(true), Some(false)] {
                    let mut v = feats.clone();
                    v.push(((x, x), m));
                    variants.push(v);
                }
            }
            for v in variants {
                let mut claims = Vec::new();
                let items: Vec<Value> = v
                    .iter()
                    .map(|&((lo, hi), max)| {
                        if row == "x_intercepts" {
                            return match fam {
                                Some((pa, pb)) => {
                                    json!({"Every": {"x0": enc(lo, hi), "period": enc(pa, pb)}})
                                }
                                None => json!({"At": enc(lo, hi)}),
                            };
                        }
                        let y = self.y_of(lo, hi);
                        if let Some((ylo, yhi)) = y {
                            claims.push(c_value(0, lo, hi, ylo, yhi));
                        }
                        let (ylo, yhi) = y.unwrap_or((0.0, 0.0));
                        let every = fam.map_or(Value::Null, |(pa, pb)| enc(pa, pb));
                        let mut item =
                            json!({"x": enc(lo, hi), "y": enc(ylo, yhi), "every": every});
                        if row == "extrema" {
                            item["kind"] = json!(if max.unwrap_or(false) { "Max" } else { "Min" });
                        }
                        item
                    })
                    .collect();
                out.push((scope.clone(), Value::Array(items), claims));
            }
        }
        out
    }
}

/// The most elements of a pool the exhaustive mode takes.
const SWEEP_POOL: usize = 48;

/// Every subset of up to `max` of `n` elements (by index), the empty one
/// among them.
fn subsets(n: usize, max: usize) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new()];
    let mut frontier = vec![Vec::new()];
    for _ in 0..max {
        let mut next = Vec::new();
        for s in &frontier {
            let from = s.last().map_or(0, |l: &usize| l + 1);
            for i in from..n {
                let mut t = s.clone();
                t.push(i);
                next.push(t);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out
}

/// The exhaustive mode on one function: about each place of each row's
/// features, every subset of up to `max` of its pool, crossed with every
/// false variant of the row; and its genuine rows held to the truth.
fn sweep_fun(fun: Fun, max: usize) -> Sweep {
    let mut out = Sweep::default();
    let text = fun.text.clone();
    let mut ctx = match context(fun) {
        Ok(c) => c,
        Err(e) => {
            out.failures.push(format!("y={text}: no context: {e}"));
            return out;
        }
    };
    out.genuine.functions = 1;
    genuine(&mut ctx, &mut out.genuine);
    out.genuine.tried += 1;
    if let Some(r) = ctx.refute_domain() {
        out.genuine
            .refuted
            .push(format!("exhaustive y={text}: {r}"));
    }
    for (row, k) in ROWS {
        for place in ctx.sweep_places(k) {
            let pool = ctx.sweep_pool(row, k, &place);
            // (Each claim of the pool the truth calls true, refuted?)
            for c in pool.iter().flatten() {
                let Ok(cl) = replay::claim(c) else { continue };
                out.genuine.tried += 1;
                if let Some(r) = ctx.refute_claim(&cl) {
                    out.genuine.refuted.push(format!(
                        "exhaustive y={text}: the truth calls {cl:?} true; refuted: {r}"
                    ));
                }
            }
            let rows = ctx.sweep_rows(row, &place);
            let mut tried = 0u64;
            let falses: Vec<&(Scope, Value, Vec<Value>)> = rows
                .iter()
                .filter(|(scope, value, _)| {
                    let f = match ctx.truth.judge(row, scope, value) {
                        Verdict::False(_) => true,
                        Verdict::True => {
                            out.genuine.tried += 1;
                            if let Some(r) = ctx.refute_row(row, scope, value) {
                                out.genuine.refuted.push(format!(
                                    "exhaustive y={text} {row} over {scope:?}: {value}: the truth \
                                     calls it true; refuted: {r}"
                                ));
                            }
                            false
                        }
                        Verdict::Unknown(why) => {
                            out.genuine
                                .unjudged
                                .push(format!("exhaustive y={text} {row}: {why}"));
                            return false;
                        }
                    };
                    if f {
                        out.false_rows += 1;
                    } else {
                        out.true_rows += 1;
                    }
                    f
                })
                .collect();
            for set in subsets(pool.len(), max) {
                let claims: Vec<Value> =
                    set.iter().flat_map(|i| pool[*i].iter().cloned()).collect();
                for (scope, value, extra) in &falses {
                    tried += 1;
                    let mut cl = claims.clone();
                    cl.extend(extra.iter().cloned());
                    cl.extend(ctx.facts.iter().cloned());
                    let case = Case {
                        row,
                        scope: scope.clone(),
                        value: value.clone(),
                        claims: cl,
                        ends: Value::Null,
                        undecided: false,
                    };
                    if ctx.accepted_false(&case) {
                        let small = ctx.minimise(&case);
                        let why = match ctx.truth.judge(row, &small.scope, &small.value) {
                            Verdict::False(w) => w,
                            Verdict::True => "?".into(),
                            Verdict::Unknown(why) => format!("truth cannot judge: {why}"),
                        };
                        out.failures.push(format!(
                            "exhaustive, {row} about {:e}:\n{}",
                            place.approx(),
                            ctx.show(&small, &why)
                        ));
                    }
                }
            }
            out.combos += tried;
            out.places
                .push((text.clone(), row, place.approx(), pool.len(), tried));
        }
    }
    out.genuine.sampled += ctx.samples.borrow().len() as u64;
    out
}

/// The exhaustive mode over the first `funs` fixed functions, `max`
/// claims at most from each pool, on `threads` threads.
fn sweep(funs: usize, max: usize, threads: u64) -> Sweep {
    let all: Vec<Fun> = exhaustive_funs().into_iter().take(funs).collect();
    let total = Mutex::new(Sweep::default());
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            let (total, next, all) = (&total, &next, &all);
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(s, move || {
                    replay::iv::init();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(fun) = all.get(i) else { break };
                        let o = sweep_fun(fun.clone(), max);
                        let mut t = total.lock().unwrap();
                        t.places.extend(o.places);
                        t.combos += o.combos;
                        t.false_rows += o.false_rows;
                        t.true_rows += o.true_rows;
                        t.failures.extend(o.failures);
                        t.genuine.absorb(o.genuine);
                    }
                })
                .expect("a sweeping thread");
        }
    });
    let mut t = total.into_inner().unwrap();
    t.places
        .sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)).then(a.2.total_cmp(&b.2)));
    t
}

impl Sweep {
    fn print(&self, max: usize) {
        let mut by_fun: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
        for (f, _, _, _, n) in &self.places {
            let e = by_fun.entry(f).or_default();
            e.0 += 1;
            e.1 += n;
        }
        println!(
            "exhaustive (subsets of up to {max}): {} places, {} combinations of claims and false \
             rows ({} false variants, {} true ones); {} accepted false",
            self.places.len(),
            self.combos,
            self.false_rows,
            self.true_rows,
            self.failures.len()
        );
        for (f, (places, n)) in by_fun {
            println!("  y={f}: {places} places, {n} combinations");
        }
        let pools: Vec<usize> = self.places.iter().map(|p| p.3).collect();
        println!(
            "  pools: {} to {} elements; genuine rows {}, {} refused by the truth",
            pools.iter().min().unwrap_or(&0),
            pools.iter().max().unwrap_or(&0),
            self.genuine.genuine_rows,
            self.genuine.genuine_false.len()
        );
        for s in self.failures.iter().take(10) {
            println!("ACCEPTED FALSE: {s}");
        }
        for s in self.genuine.genuine_false.iter().take(10) {
            println!("GENUINE ROW THE TRUTH REFUSES: {s}");
        }
        for s in self.genuine.unjudged.iter().take(10) {
            println!("ROW THE TRUTH CANNOT JUDGE: {s}");
        }
        println!(
            "  refuter: {} claims and rows the truth calls true tried ({} doubles sampled), {} \
             refuted",
            self.genuine.tried,
            self.genuine.sampled,
            self.genuine.refuted.len()
        );
        for s in self.genuine.refuted.iter().take(10) {
            println!("TRUTH REFUTED: {s}");
        }
    }
}

/// The exhaustive mode's CI slice: the first functions, up to two claims
/// from each pool.
#[test]
fn exhaustive_slice() {
    let t = sweep(CI_FUNS, 2, threads());
    t.print(2);
    assert!(
        t.failures.is_empty()
            && t.genuine.genuine_false.is_empty()
            && t.genuine.unjudged.is_empty()
            && t.genuine.refuted.is_empty(),
        "{:?}",
        t.failures
            .first()
            .or(t.genuine.genuine_false.first())
            .or(t.genuine.unjudged.first())
            .or(t.genuine.refuted.first())
    );
}

/// The full exhaustive run: every fixed function, up to three claims
/// (`REPLAY_FUZZ_SUBSETS` to change it), release.
#[test]
#[ignore = "the full exhaustive run: up to an hour"]
fn exhaustive_full() {
    let max = env_u64("REPLAY_FUZZ_SUBSETS").unwrap_or(3) as usize;
    let started = std::time::Instant::now();
    let t = sweep(usize::MAX, max, threads());
    t.print(max);
    println!("{:.1} s", started.elapsed().as_secs_f64());
    assert!(
        t.failures.is_empty()
            && t.genuine.genuine_false.is_empty()
            && t.genuine.unjudged.is_empty()
            && t.genuine.refuted.is_empty(),
        "{:?}",
        t.failures
            .first()
            .or(t.genuine.genuine_false.first())
            .or(t.genuine.unjudged.first())
            .or(t.genuine.refuted.first())
    );
}

const SHORT_SEED: u64 = 20_261_024;
const SHORT_FUNCTIONS: u64 = 24;
const CASES: u64 = 24;

/// The short run: a fixed seed, seconds.
#[test]
fn short_run() {
    let t = run(SHORT_SEED, SHORT_FUNCTIONS, CASES, threads(), Kind::Poly);
    t.print(SHORT_SEED);
    assert_clean(&t);
}

/// The short run of functions with corners: a fixed seed, seconds.
#[test]
fn short_run_kinked() {
    let t = run(KINK_SEED, KINK_FUNCTIONS, CASES, threads(), Kind::Kink);
    t.print(KINK_SEED);
    assert_clean(&t);
}

const KINK_SEED: u64 = 20_261_101;
const KINK_FUNCTIONS: u64 = 12;

/// The short run of periodic functions: a fixed seed, seconds.
#[test]
fn short_run_periodic() {
    let t = run(TRIG_SEED, SHORT_FUNCTIONS, CASES, threads(), Kind::Trig);
    t.print(TRIG_SEED);
    assert_clean(&t);
}

const TRIG_SEED: u64 = 20_261_102;

/// The long run: `REPLAY_FUZZ_SEED`, `REPLAY_FUZZ_FUNCTIONS`,
/// `REPLAY_FUZZ_CASES`, `REPLAY_FUZZ_THREADS`, `REPLAY_FUZZ_KIND`
/// (release).
#[test]
#[ignore = "the long run: minutes"]
fn long_run() {
    let seed = env_u64("REPLAY_FUZZ_SEED").unwrap_or(SHORT_SEED);
    let functions = env_u64("REPLAY_FUZZ_FUNCTIONS").unwrap_or(2000);
    let cases = env_u64("REPLAY_FUZZ_CASES").unwrap_or(CASES);
    let started = std::time::Instant::now();
    let t = run(seed, functions, cases, threads(), Kind::from_env());
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

/// The periodic truth against what is known by hand: sin(x°)'s zeros at
/// 0 and 180, its maximum at 90, inflections where it is 0, period 360;
/// tan(x°)'s poles at 90 + 180k, its inflection at 0, rising between;
/// 2·sin(3x + 30)° + 1's zeros at −20 and 60 (mod 120); sin's zero at π
/// in radians, above the double nearest π; and a value at a double read
/// against the doubles beside it.
#[test]
fn the_truth_knows_its_trig() {
    let tr = |func, a: i64, b: i64, amp: i64, c: i64, deg| Trig {
        func,
        a: qi(a),
        b: qi(b),
        amp: qi(amp),
        c: qi(c),
        deg,
    };
    let at = |t: &Truth, row: &str| -> Vec<f64> {
        t.features(row).iter().map(|f| f.at.approx()).collect()
    };
    let sin = Truth::new_trig(&tr(TrigFn::Sin, 1, 0, 1, 0, true), (-1.0, 361.0));
    assert_eq!(at(&sin, "x_intercepts"), vec![0.0, 180.0, 360.0]);
    assert_eq!(at(&sin, "extrema"), vec![90.0, 270.0]);
    assert_eq!(sin.extrema[0].max, Some(true));
    assert_eq!(at(&sin, "inflections"), vec![0.0, 180.0, 360.0]);
    assert_eq!(sin.judge_period(&enc(360.0, 360.0)), Verdict::True);
    assert!(matches!(
        sin.judge_period(&enc(180.0, 180.0)),
        Verdict::False(_)
    ));
    assert_eq!(sin.sign_at(0, 30.0), Some(Greater));
    assert_eq!(sin.sign_at(1, 90.0), Some(Equal));
    assert_eq!(sin.sign_at(2, 30.0), Some(Less));
    let tan = Truth::new_trig(&tr(TrigFn::Tan, 1, 0, 1, 0, true), (-100.0, 100.0));
    assert!(tan.dom.misses_box(90.0, 90.0) && tan.dom.misses_box(-90.0, -90.0));
    assert!(!tan.dom.holds_box(80.0, 100.0) && tan.dom.holds_box(-80.0, 80.0));
    assert_eq!(at(&tan, "inflections"), vec![0.0]);
    assert!(tan.extrema.is_empty());
    assert_eq!(tan.sign_over(1, -89.0, 89.0), Some(true));
    let shifted = Truth::new_trig(&tr(TrigFn::Sin, 3, 30, 2, 1, true), (-21.0, 61.0));
    assert_eq!(at(&shifted, "x_intercepts"), vec![-20.0, 60.0]);
    let rad = Truth::new_trig(&tr(TrigFn::Sin, 1, 0, 1, 0, false), (-1.0, 4.0));
    let mut pi = rad.zeros[1].at.clone();
    assert_eq!(pi.cmp(&qd(std::f64::consts::PI)), Greater);
    assert_eq!(
        pi.enc(),
        (std::f64::consts::PI, std::f64::consts::PI.next_up())
    );
    // sin(1) = 0.8414709848078965 (to the nearest), between the doubles
    // about it.
    let (lo, hi) = rad.val_enc(0, 1.0).expect("a value");
    assert!(lo < hi && lo.next_up() == hi && lo <= 0.8414709848078965 && 0.8414709848078965 <= hi);
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
        let f = Pw::one(
            Poly::new(vec![qi(0), qi(0), Rational::from(&du * 3u32), qi(-2)]).at_shift(&qi(1)),
        );
        Fun {
            text: t.into(),
            evaluated: vec!["f′ = Mul(Mul(6,Sub(x,1)),Sub(Pow(2,Neg(52)),Sub(x,1)))".into()],
            f,
            dom,
            planted: vec![qi(1)],
            expanded: Vec::new(),
            trig: None,
        }
    };
    let kink = |a: f64| json!({"Kink": {"x": bx(a, a)}});
    let cases: Vec<(Fun, &'static str, Scope, Vec<Value>)> = vec![
        // R22-M-01: x² − 1, "no extrema for 0 ≤ x ≤ 2", a gap from 0.
        (
            Fun {
                text: "x^2-1".into(),
                evaluated: Vec::new(),
                f: Pw::one(Poly::new(vec![qi(-1), qi(0), qi(1)])),
                dom: Dom::Line,
                planted: vec![qi(0)],
                expanded: Vec::new(),
                trig: None,
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
                f: Pw::one(Poly::x_minus(&qi(1)).pow(2)),
                dom: Dom::Line,
                planted: vec![qi(1)],
                expanded: Vec::new(),
                trig: None,
            },
            "extrema",
            Scope::Window(1.0, 1.0),
            vec![kink(1.0)],
        ),
        (
            Fun {
                text: "(x-1)^3".into(),
                evaluated: Vec::new(),
                f: Pw::one(Poly::x_minus(&qi(1)).pow(3)),
                dom: Dom::Line,
                planted: vec![qi(1)],
                expanded: Vec::new(),
                trig: None,
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
            undecided: false,
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
                // (The polynomial the run made as its i-th.)
                let (i, fun) = (0..2000)
                    .find_map(|i| {
                        let mut rng = Rng::new(seed, i, 0);
                        (0..20)
                            .find_map(|_| make_fun(&mut rng))
                            .filter(|f| f.text == text)
                            .map(|f| (i, f))
                    })
                    .unwrap_or_else(|| panic!("{text}: not made by seed {seed}"));
                let t = run_fun(fun, seed, i, 0);
                assert!(t.genuine_rows > 0, "{text}: no genuine row judged");
                bad.extend(t.genuine_false);
                bad.extend(t.unjudged);
            }
            bad
        })
        .expect("a thread")
        .join()
        .expect("held to the truth");
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn the_zero_functions_list_names_distinct_places() {
    let t = Truth::new(&Pw::one(Poly::constant(qi(0))), &Dom::Line);
    let zero = json!({"At": {"lo": 0.0, "hi": 0.0}});
    let one = json!({"At": {"lo": 1.0, "hi": 1.0}});
    let wide = json!({"At": {"lo": 0.0, "hi": 1.0}});
    let mut errors = Vec::new();
    for (label, scope, value, expected) in [
        (
            "duplicate point, Points",
            Scope::Points,
            json!([zero, zero]),
            false,
        ),
        (
            "duplicate point, Window",
            Scope::Window(0.0, 0.0),
            json!([zero, zero]),
            false,
        ),
        ("distinct points", Scope::Points, json!([zero, one]), true),
        (
            "wide intervals have distinct zeros",
            Scope::Points,
            json!([wide, wide]),
            true,
        ),
        (
            "one required point",
            Scope::Window(0.0, 0.0),
            json!([zero]),
            true,
        ),
        (
            "missing required point",
            Scope::Window(0.0, 0.0),
            json!([]),
            false,
        ),
    ] {
        let verdict = t.judge("x_intercepts", &scope, &value);
        println!("{label}: {verdict:?}; expected true = {expected}");
        if matches!(verdict, Verdict::True) != expected {
            errors.push(label);
        }
    }
    let end = Truth::new(&Pw::one(Poly::constant(qi(0))), &Dom::From(qi(1)));
    let clipped = json!({"At": {"lo": 0.0, "hi": 1.0}});
    let verdict = end.judge("x_intercepts", &Scope::Points, &json!([clipped, clipped]));
    println!("duplicate domain-end point: {verdict:?}; expected false");
    if !matches!(verdict, Verdict::False(_)) {
        errors.push("duplicate domain-end point");
    }
    let end = Truth::new(&Pw::one(Poly::constant(qi(0))), &Dom::Upto(qi(1)));
    let clipped = json!({"At": {"lo": 1.0, "hi": 2.0}});
    assert!(matches!(
        end.judge("x_intercepts", &Scope::Points, &json!([clipped, clipped])),
        Verdict::False(_)
    ));
    assert!(errors.is_empty(), "false truth verdicts: {errors:?}");
}

#[test]
fn the_crossing_claims_need_continuity_and_strict_monotonicity() {
    let line = Dom::Line;
    let square = Truth::new(&Pw::one(Poly::new(vec![qi(1), qi(0), qi(1)])), &line);
    let cubic = Truth::new(
        &Pw::one(Poly::new(vec![qi(0), qi(-1), qi(0), qi(1)])),
        &line,
    );
    let x = Poly::x_minus(&qi(0));
    let corner = Truth::new(
        &Pw {
            cuts: vec![qi(0)],
            polys: vec![x.scale(&qi(-1)), x.clone()],
        },
        &line,
    );
    let identity = Truth::new(&Pw::one(x.clone()), &line);
    let strict_cubic = Truth::new(&Pw::one(x.pow(3)), &line);
    let cases = [
        (
            "nonmonotone NoCross",
            &square,
            Claim::NoCross {
                x: (-1.0, 1.0),
                of: Subject::F(0),
                c: 0.0,
                above: true,
            },
            false,
        ),
        (
            "nonmonotone OneCross",
            &cubic,
            Claim::OneCross {
                x: (-1.5, -0.5),
                of: Subject::F(0),
                c: 0.0,
            },
            false,
        ),
        (
            "nonmonotone ExactAt",
            &cubic,
            Claim::ExactAt {
                x: (-1.5, -0.5),
                of: Subject::F(0),
                c: 0.0,
                at: -1.0,
            },
            false,
        ),
        (
            "discontinuous derivative OneCross",
            &corner,
            Claim::OneCross {
                x: (-1.0, 1.0),
                of: Subject::F(1),
                c: 0.0,
            },
            false,
        ),
        (
            "identity OneCross",
            &identity,
            Claim::OneCross {
                x: (-1.0, 1.0),
                of: Subject::F(0),
                c: 0.0,
            },
            true,
        ),
        (
            "identity ExactAt",
            &identity,
            Claim::ExactAt {
                x: (-1.0, 1.0),
                of: Subject::F(0),
                c: 0.0,
                at: 0.0,
            },
            true,
        ),
        (
            "identity NoCross",
            &identity,
            Claim::NoCross {
                x: (1.0, 2.0),
                of: Subject::F(0),
                c: 0.0,
                above: true,
            },
            true,
        ),
        (
            "strict cubic with stationary zero",
            &strict_cubic,
            Claim::OneCross {
                x: (-1.0, 1.0),
                of: Subject::F(0),
                c: 0.0,
            },
            true,
        ),
    ];
    let mut errors = Vec::new();
    for (label, t, claim, expected) in cases {
        let actual = t.claim_true(&claim);
        println!("{label}: {actual:?}; expected {expected}");
        if actual != Some(expected) {
            errors.push(label);
        }
    }
    assert!(errors.is_empty(), "false claim truth verdicts: {errors:?}");
    let sin = Truth::new_trig(
        &Trig {
            func: TrigFn::Sin,
            a: qi(1),
            b: qi(0),
            amp: qi(1),
            c: qi(0),
            deg: true,
        },
        (-180.0, 180.0),
    );
    for (a, b, expected) in [(-100.0, 100.0, false), (-90.0, 90.0, true)] {
        assert_eq!(
            sin.claim_true(&Claim::OneCross {
                x: (a, b),
                of: Subject::F(0),
                c: 0.0
            }),
            Some(expected)
        );
    }
}

#[test]
fn the_periodic_truth_counts_every_orbit_without_truncating() {
    let t = Truth::new_trig(
        &Trig {
            func: TrigFn::Sin,
            a: qi(1),
            b: qi(0),
            amp: qi(1),
            c: qi(0),
            deg: true,
        },
        (-1.0, 361.0),
    );
    let family = |first: f64, period: f64| {
        json!({"Every": {
            "x0": {"lo": first, "hi": first}, "period": {"lo": period, "hi": period}
        }})
    };
    let n = 2f64.powi(34);
    let sparse = json!([family(0.0, 180.0 * n), family(180.0, 180.0 * n)]);
    // These are true subsets, but the complete line omits 360 and more.
    assert_eq!(
        t.judge("x_intercepts", &Scope::Points, &sparse),
        Verdict::True
    );
    assert!(matches!(
        t.judge("x_intercepts", &Scope::Line, &sparse),
        Verdict::False(_)
    ));
    assert_eq!(
        t.judge("x_intercepts", &Scope::Line, &json!([family(0.0, 180.0)])),
        Verdict::True
    );
    assert!(matches!(
        t.judge("x_intercepts", &Scope::Line, &json!([family(0.0, 360.0)])),
        Verdict::False(_)
    ));
    // A large common period can also be correct: partition all integers
    // by their first set bit, and the multiples of 2^34.
    let mut cover: Vec<Value> = (1..=34)
        .map(|k| family(180.0 * 2f64.powi(k - 1), 180.0 * 2f64.powi(k)))
        .collect();
    cover.push(family(0.0, 180.0 * n));
    assert_eq!(
        t.judge("x_intercepts", &Scope::Line, &json!(cover)),
        Verdict::True
    );
}

#[test]
fn zero_function_families_are_not_read_as_one_point() {
    let family = json!({"Every": {"x0": enc(1.0,1.0), "period": enc(1.0,1.0)}});
    for dom in [
        Dom::Line,
        Dom::From(qi(0)),
        Dom::Upto(qi(2)),
        Dom::Hole(qi(0)),
    ] {
        let t = Truth::new(&Pw::one(Poly::constant(qi(0))), &dom);
        assert!(matches!(
            t.judge("x_intercepts", &Scope::Points, &json!([family])),
            Verdict::Unknown(_)
        ));
    }
}

#[test]
#[should_panic(expected = "rows not judged")]
fn a_row_the_truth_cannot_judge_does_not_pass_the_run() {
    replay::iv::init();
    let mut ctx = context(Fun {
        text: "0*sqrt(x)".into(),
        evaluated: Vec::new(),
        f: Pw::one(Poly::constant(qi(0))),
        dom: Dom::From(qi(0)),
        planted: Vec::new(),
        expanded: Vec::new(),
        trig: None,
    })
    .expect("zero function on a half-line");
    // Exercise row ingestion with a repeating list the model does not
    // read. A true/false tally alone must not let this run pass.
    let case = Case {
        row: "x_intercepts",
        scope: Scope::Points,
        value: json!([{"Every": {"x0": enc(1.0,1.0), "period": enc(1.0,1.0)}}]),
        claims: vec![c_value(0, 1.0, 1.0, 0.0, 0.0)],
        ends: Value::Null,
        undecided: false,
    };
    ctx.genuine = case.json(&ctx.base);
    let mut tally = Tally::default();
    genuine(&mut ctx, &mut tally);
    assert_eq!(tally.unjudged.len(), 1);
    assert_clean(&tally);
}

// ---------------------------------------------------- the audit after review 24

/// Monotone pieces meeting at one enclosure that holds two turns: each
/// piece true up to some place in it, the enclosures' extents covering
/// the line, yet the piece between the turns left out. The truth asked
/// each piece's ends of some place on its own and the coverage of the
/// enclosures' extents; it called these true (the replay refuses them:
/// "a piece's end holds two turns", round 20's audit). Now one place
/// for each end is chosen, and the pieces between them must cover.
#[test]
fn a_piece_end_holding_two_turns_leaves_the_piece_between_out() {
    // x³ − x: increasing up to −1/√3, decreasing to 1/√3, increasing on.
    let t = Truth::new(
        &Pw::one(Poly::new(vec![qi(0), qi(-1), qi(0), qi(1)])),
        &Dom::Line,
    );
    let at = |x: (f64, f64)| json!({"At": {"x": enc(x.0, x.1), "closed": true}});
    let piece = |lo: Value, hi: Value, dir: &str| json!({"on": {"lo": lo, "hi": hi}, "dir": dir});
    let hull = at((-0.6, 0.6));
    let row = json!([
        piece(json!("NegInf"), hull.clone(), "Increasing"),
        piece(hull, json!("PosInf"), "Increasing"),
    ]);
    let v = t.judge("monotonicity", &Scope::Line, &row);
    assert!(matches!(v, Verdict::False(_)), "{v:?}");
    // The true row, its ends the turns' own enclosures.
    let (a, b) = (t.breaks[0].clone().enc(), t.breaks[1].clone().enc());
    let row = json!([
        piece(json!("NegInf"), at(a), "Increasing"),
        piece(at(a), at(b), "Decreasing"),
        piece(at(b), json!("PosInf"), "Increasing"),
    ]);
    assert_eq!(t.judge("monotonicity", &Scope::Line, &row), Verdict::True);
    // The same over one period of sin(x)°: increasing from −90 to [90,
    // 270] and on from there to 450, the decreasing piece left out.
    let sin = Truth::new_trig(
        &Trig {
            func: TrigFn::Sin,
            a: qi(1),
            b: qi(0),
            amp: qi(1),
            c: qi(0),
            deg: true,
        },
        (-1500.0, 1500.0),
    );
    let period = Scope::Period(-90.0, 270.0);
    let row = json!([
        piece(at((-90.0, -90.0)), at((90.0, 270.0)), "Increasing"),
        piece(at((90.0, 270.0)), at((450.0, 450.0)), "Increasing"),
    ]);
    let v = sin.judge("monotonicity", &period, &row);
    assert!(matches!(v, Verdict::False(_)), "{v:?}");
    let row = json!([
        piece(at((-90.0, -90.0)), at((90.0, 90.0)), "Increasing"),
        piece(at((90.0, 90.0)), at((270.0, 270.0)), "Decreasing"),
    ]);
    assert_eq!(sin.judge("monotonicity", &period, &row), Verdict::True);
}

/// `TailChain` says more than its conclusion: f⁽ᵏ⁺ᵐ⁾ of one sign on the
/// tail and the signs at `from` that keep f⁽ᵏ⁾ moving away from c (the
/// rows read f → ∞ from one about f′). The truth read the conclusion
/// alone: x² + 1 above 0 on [−1, ∞) "by f′ > 0" was true.
#[test]
fn a_tail_chain_is_true_only_with_its_chain() {
    let t = Truth::new(&Pw::one(Poly::new(vec![qi(1), qi(0), qi(1)])), &Dom::Line);
    let chain = |side: Side, from: f64, order: usize| Claim::TailChain {
        side,
        from,
        of: Subject::F(0),
        c: 0.0,
        above: true,
        order,
    };
    for (claim, expected) in [
        (chain(Side::Right, -1.0, 1), false),
        (chain(Side::Right, -1.0, 2), false),
        (chain(Side::Right, 0.0, 2), false),
        (chain(Side::Right, 1.0, 1), true),
        (chain(Side::Right, 1.0, 2), true),
        (chain(Side::Left, -1.0, 1), true),
        (chain(Side::Left, 1.0, 1), false),
        (chain(Side::Left, -1.0, 2), true),
    ] {
        assert_eq!(t.claim_true(&claim), Some(expected), "{claim:?}");
    }
}

/// A periodic f's zeros over an infinite gap box: its lists hold its
/// span's alone, so a box past the span was read as clear of them.
#[test]
fn a_periodic_gap_clear_over_a_tail_asks_every_period() {
    let trig = |func| Trig {
        func,
        a: qi(1),
        b: qi(0),
        amp: qi(1),
        c: qi(0),
        deg: true,
    };
    let sin = Truth::new_trig(&trig(TrigFn::Sin), (-1500.0, 1500.0));
    let tan = Truth::new_trig(&trig(TrigFn::Tan), (-1500.0, 1500.0));
    let clear = |k: usize, a: f64, b: f64| Claim::GapClear {
        x: (a, b),
        of: Subject::F(k),
    };
    assert_eq!(sin.claim_true(&clear(0, 1e6, f64::INFINITY)), Some(false));
    assert_eq!(
        sin.claim_true(&clear(0, f64::NEG_INFINITY, -1e6)),
        Some(false)
    );
    // (tan′ = sec² has no zero at all.)
    assert_eq!(tan.claim_true(&clear(1, 1e6, f64::INFINITY)), Some(true));
    assert_eq!(sin.claim_true(&clear(0, 10.0, 170.0)), Some(true));
}

/// What holds no real number is no item, end, period or scope: an
/// enclosure with a NaN end (read as an infinite one, it held every
/// feature), a low end read from "PosInf" (any string was the infinite
/// end), a direction or an extremum's kind of no known word (read as
/// constant, and as a minimum), a range over a window (a part of it is no
/// row; it was judged as the whole range).
#[test]
fn the_truth_refuses_what_holds_no_real_or_says_nothing() {
    let nan = json!({"lo": "nan", "hi": "nan"});
    let square = Truth::new(&Pw::one(Poly::new(vec![qi(-1), qi(0), qi(1)])), &Dom::Line);
    let zero = Truth::new(&Pw::one(Poly::constant(qi(0))), &Dom::Line);
    let cube = Truth::new(
        &Pw::one(Poly::new(vec![qi(0), qi(0), qi(0), qi(1)])),
        &Dom::Line,
    );
    let up = Truth::new(&Pw::one(Poly::new(vec![qi(1), qi(0), qi(1)])), &Dom::Line);
    let two = Truth::new(&Pw::one(Poly::constant(qi(2))), &Dom::Line);
    let sin = Truth::new_trig(
        &Trig {
            func: TrigFn::Sin,
            a: qi(1),
            b: qi(0),
            amp: qi(1),
            c: qi(0),
            deg: true,
        },
        (-1500.0, 1500.0),
    );
    let at = |x: f64, closed: bool| json!({"At": {"x": enc(x, x), "closed": closed}});
    let cases: Vec<(&str, &Truth, &str, Scope, Value)> = vec![
        (
            "NaN zero of x² − 1",
            &square,
            "x_intercepts",
            Scope::Points,
            json!([{ "At": nan }]),
        ),
        (
            "NaN zero of f ≡ 0",
            &zero,
            "x_intercepts",
            Scope::Points,
            json!([{ "At": nan }]),
        ),
        (
            "NaN period of sin(x)°",
            &sin,
            "period",
            Scope::Line,
            nan.clone(),
        ),
        (
            "x² + 1's range up to NegInf",
            &up,
            "range",
            Scope::Line,
            json!([{"lo": at(1.0, true), "hi": "NegInf"}]),
        ),
        (
            "x³'s range from PosInf",
            &cube,
            "range",
            Scope::Line,
            json!([{"lo": "PosInf", "hi": "PosInf"}]),
        ),
        (
            "2 going sideways",
            &two,
            "monotonicity",
            Scope::Line,
            json!([{"on": {"lo": "NegInf", "hi": "PosInf"}, "dir": "Sideways"}]),
        ),
        (
            "x² − 1's saddle at 0",
            &square,
            "extrema",
            Scope::Points,
            json!([{"x": enc(0.0, 0.0), "y": enc(-1.0, -1.0), "kind": "Saddle", "every": null}]),
        ),
        (
            "x² + 1's range over a window",
            &up,
            "range",
            Scope::Window(5.0, 6.0),
            json!([{"lo": at(1.0, true), "hi": "PosInf"}]),
        ),
    ];
    for (what, t, row, scope, value) in cases {
        let v = t.judge(row, &scope, &value);
        assert!(matches!(v, Verdict::False(_)), "{what}: {v:?}");
    }
    // Controls: the same rows written right.
    assert_eq!(
        up.judge(
            "range",
            &Scope::Line,
            &json!([{"lo": at(1.0, true), "hi": "PosInf"}])
        ),
        Verdict::True
    );
    assert_eq!(
        cube.judge(
            "range",
            &Scope::Line,
            &json!([{"lo": "NegInf", "hi": "PosInf"}])
        ),
        Verdict::True
    );
    assert_eq!(
        square.judge(
            "extrema",
            &Scope::Points,
            &json!([{"x": enc(0.0, 0.0), "y": enc(-1.0, -1.0), "kind": "Min", "every": null}])
        ),
        Verdict::True
    );
}

/// A plateau (f constant on a stretch, not everywhere) or zeros on a
/// stretch where f ≡ 0: max(x, 0) was "increasing" on the line, its range
/// (−∞, ∞) and its zeros 0 alone. No generator makes one; the truth now
/// says it can't judge, which fails a run.
#[test]
fn a_plateau_is_not_judged() {
    let f = Pw {
        cuts: vec![qi(0)],
        polys: vec![Poly::default(), Poly::x_minus(&qi(0))],
    };
    let t = Truth::new(&f, &Dom::Line);
    for (row, value) in [
        (
            "monotonicity",
            json!([{"on": {"lo": "NegInf", "hi": "PosInf"}, "dir": "Increasing"}]),
        ),
        ("range", json!([{"lo": "NegInf", "hi": "PosInf"}])),
        ("x_intercepts", json!([{"At": enc(0.0, 0.0)}])),
        ("extrema", json!([])),
    ] {
        let v = t.judge(row, &Scope::Line, &value);
        assert!(matches!(v, Verdict::Unknown(_)), "{row}: {v:?}");
    }
}

/// Shapes the truth panicked on (a box with an infinite end where it
/// reads doubles, a periodic item over a tail, an infinite y or range
/// end): each now read so, false, true or unjudged. Hardening: none was
/// called true that isn't.
#[test]
fn the_truth_reads_infinite_ends_without_panicking() {
    let square = Truth::new(&Pw::one(Poly::new(vec![qi(-1), qi(0), qi(1)])), &Dom::Line);
    let zero = Truth::new(&Pw::one(Poly::constant(qi(0))), &Dom::From(qi(1)));
    let sin = Truth::new_trig(
        &Trig {
            func: TrigFn::Sin,
            a: qi(1),
            b: qi(0),
            amp: qi(1),
            c: qi(0),
            deg: true,
        },
        (-1500.0, 1500.0),
    );
    let inf = f64::INFINITY;
    for claim in [
        Claim::Touch {
            x: (-inf, 1.0),
            of: Subject::F(0),
            c: 0.0,
            at: -inf,
            order: 0,
            above: true,
        },
        Claim::KinkSides {
            x: (-inf, inf),
            at: None,
            order: 1,
            left: KinkSign::Pos,
            right: KinkSign::Pos,
            joined: false,
        },
        Claim::KinkAt {
            x: (-inf, inf),
            at: 0.0,
            left: false,
            right: true,
        },
    ] {
        assert_eq!(square.claim_true(&claim), Some(false), "{claim:?}");
    }
    let v = sin.judge(
        "x_intercepts",
        &Scope::Points,
        &json!([{"At": enc(-inf, 0.0)}]),
    );
    assert!(matches!(v, Verdict::Unknown(_)), "{v:?}");
    let v = square.judge(
        "extrema",
        &Scope::Points,
        &json!([{"x": enc(0.0, 0.0), "y": enc(-inf, inf), "kind": "Min", "every": null}]),
    );
    assert_eq!(v, Verdict::True);
    let v = zero.judge("x_intercepts", &Scope::Window(0.0, inf), &json!([]));
    assert!(matches!(v, Verdict::False(_)), "{v:?}");
    let v = square.judge(
        "range",
        &Scope::Line,
        &json!([{"lo": {"At": {"x": enc(-inf, -1.0), "closed": true}}, "hi": "PosInf"}]),
    );
    assert_eq!(v, Verdict::True);
}

/// The refuter on its own, each claim and row put to it whatever the
/// truth says: review 24's (R24-L-02's three crossing claims; two items
/// at one double of f ≡ 0, R24-L-01; sin(x)°'s zeros as two families
/// every 180·2³⁴, R24-L-03; a family of f ≡ 0 on [0, ∞) through −1,
/// R24-L-04) and the audit after review 24's (pieces meeting at one
/// enclosure of two turns; a tail chain from the wrong side of f′'s
/// zero; max(x, 0) increasing, its zeros {0}) refuted; each with a true
/// control that is not. (Not reached: R24-L-01's two items clipped to
/// one domain end, whose one point only the domain's exact end shows; a
/// gap clear to infinity; the range of max(x, 0) said (−∞, ∞), every
/// sampled value in it.)
#[test]
fn the_refuter_catches_review_24s_examples() {
    replay::iv::init();
    let x = |c: i64| Poly::x_minus(&qi(c));
    let line = |text: &str, f: Pw| {
        context(Fun {
            text: text.into(),
            evaluated: Vec::new(),
            f,
            dom: Dom::Line,
            planted: Vec::new(),
            expanded: Vec::new(),
            trig: None,
        })
        .unwrap_or_else(|e| panic!("{text}: {e}"))
    };
    let square = line("x^2+1", Pw::one(Poly::new(vec![qi(1), qi(0), qi(1)])));
    let cubic = line(
        "x^3-x",
        Pw::one(Poly::new(vec![qi(0), qi(-1), qi(0), qi(1)])),
    );
    let corner = line(
        "abs(x)",
        Pw {
            cuts: vec![qi(0)],
            polys: vec![x(0).scale(&qi(-1)), x(0)],
        },
    );
    let ramp = line(
        "max(x,0)",
        Pw {
            cuts: vec![qi(0)],
            polys: vec![Poly::default(), x(0)],
        },
    );
    let zero = line("0", Pw::one(Poly::constant(qi(0))));
    let half = context(Fun {
        text: "0*sqrt(x)".into(),
        evaluated: Vec::new(),
        f: Pw::one(Poly::constant(qi(0))),
        dom: Dom::From(qi(0)),
        planted: Vec::new(),
        expanded: Vec::new(),
        trig: None,
    })
    .expect("0*sqrt(x)");
    let sin = context(fixed_trig("sin(x)", TrigFn::Sin, 1, 0, 1, 0)).expect("sin(x)°");
    let f = |k: usize| Subject::F(k);
    let claims: Vec<(&str, &Ctx, Claim, bool)> = vec![
        (
            "x²+1 never crossing 0 on [−1, 1], monotone",
            &square,
            Claim::NoCross {
                x: (-1.0, 1.0),
                of: f(0),
                c: 0.0,
                above: true,
            },
            true,
        ),
        (
            "x³−x crossing 0 once on [−1.5, −0.5], monotone",
            &cubic,
            Claim::OneCross {
                x: (-1.5, -0.5),
                of: f(0),
                c: 0.0,
            },
            true,
        ),
        (
            "x³−x 0 at −1 alone on [−1.5, −0.5], monotone",
            &cubic,
            Claim::ExactAt {
                x: (-1.5, -0.5),
                of: f(0),
                c: 0.0,
                at: -1.0,
            },
            true,
        ),
        (
            "|x|′ crossing 0 once on [−1, 1]",
            &corner,
            Claim::OneCross {
                x: (-1.0, 1.0),
                of: f(1),
                c: 0.0,
            },
            true,
        ),
        (
            "x²+1 above 0 on [−1, ∞) by f′ > 0",
            &square,
            Claim::TailChain {
                side: Side::Right,
                from: -1.0,
                of: f(0),
                c: 0.0,
                above: true,
                order: 1,
            },
            true,
        ),
        (
            "x²+1 never crossing 0 on [1, 2]",
            &square,
            Claim::NoCross {
                x: (1.0, 2.0),
                of: f(0),
                c: 0.0,
                above: true,
            },
            false,
        ),
        (
            "x³−x crossing 0 once on [−1.2, −0.8]",
            &cubic,
            Claim::OneCross {
                x: (-1.2, -0.8),
                of: f(0),
                c: 0.0,
            },
            false,
        ),
        (
            "x²+1 above 0 on [1, ∞) by f′ > 0",
            &square,
            Claim::TailChain {
                side: Side::Right,
                from: 1.0,
                of: f(0),
                c: 0.0,
                above: true,
                order: 1,
            },
            false,
        ),
    ];
    let mut wrong = Vec::new();
    for (what, ctx, claim, refuted) in claims {
        let r = ctx.refute_claim(&claim);
        println!("{what}: {r:?}");
        if r.is_some() != refuted {
            wrong.push(what);
        }
    }
    let family = |x0: f64, p: f64| json!({"Every": {"x0": enc(x0, x0), "period": enc(p, p)}});
    let at = |x: f64| json!({"At": enc(x, x)});
    let end = |x: (f64, f64)| json!({"At": {"x": enc(x.0, x.1), "closed": true}});
    let piece = |lo: Value, hi: Value, dir: &str| json!({"on": {"lo": lo, "hi": hi}, "dir": dir});
    let n = 2f64.powi(34);
    let (t0, t1) = (
        cubic.truth.breaks[0].clone().enc(),
        cubic.truth.breaks[1].clone().enc(),
    );
    let rows: Vec<(&str, &Ctx, &str, Scope, Value, bool)> = vec![
        (
            "f ≡ 0's zero at 0, twice",
            &zero,
            "x_intercepts",
            Scope::Points,
            json!([at(0.0), at(0.0)]),
            true,
        ),
        (
            "sin(x)°'s zeros every 180·2³⁴ from 0 and 180",
            &sin,
            "x_intercepts",
            Scope::Line,
            json!([family(0.0, 180.0 * n), family(180.0, 180.0 * n)]),
            true,
        ),
        (
            "0·√x's zeros every 1 from 1",
            &half,
            "x_intercepts",
            Scope::Points,
            json!([family(1.0, 1.0)]),
            true,
        ),
        (
            "x³ − x increasing to [−0.6, 0.6] and from it",
            &cubic,
            "monotonicity",
            Scope::Line,
            json!([
                piece(json!("NegInf"), end((-0.6, 0.6)), "Increasing"),
                piece(end((-0.6, 0.6)), json!("PosInf"), "Increasing"),
            ]),
            true,
        ),
        (
            "max(x, 0) increasing",
            &ramp,
            "monotonicity",
            Scope::Line,
            json!([piece(json!("NegInf"), json!("PosInf"), "Increasing")]),
            true,
        ),
        (
            "max(x, 0)'s zeros {0}",
            &ramp,
            "x_intercepts",
            Scope::Line,
            json!([at(0.0)]),
            true,
        ),
        (
            "x² + 1's maximum at 0",
            &square,
            "extrema",
            Scope::Points,
            json!([{"x": enc(0.0, 0.0), "y": enc(1.0, 1.0), "kind": "Max", "every": null}]),
            true,
        ),
        (
            "x² + 1's minimum at 0",
            &square,
            "extrema",
            Scope::Points,
            json!([{"x": enc(0.0, 0.0), "y": enc(1.0, 1.0), "kind": "Min", "every": null}]),
            false,
        ),
        (
            "f ≡ 0's zeros 0 and 1",
            &zero,
            "x_intercepts",
            Scope::Points,
            json!([at(0.0), at(1.0)]),
            false,
        ),
        (
            "sin(x)°'s zeros every 180 from 0",
            &sin,
            "x_intercepts",
            Scope::Line,
            json!([family(0.0, 180.0)]),
            false,
        ),
        (
            "f ≡ 0's zeros every 1 from 1",
            &zero,
            "x_intercepts",
            Scope::Points,
            json!([family(1.0, 1.0)]),
            false,
        ),
        (
            "x³ − x's monotone pieces",
            &cubic,
            "monotonicity",
            Scope::Line,
            json!([
                piece(json!("NegInf"), end(t0), "Increasing"),
                piece(end(t0), end(t1), "Decreasing"),
                piece(end(t1), json!("PosInf"), "Increasing"),
            ]),
            false,
        ),
    ];
    for (what, ctx, row, scope, value, refuted) in rows {
        let r = ctx.refute_row(row, &scope, &value);
        println!("{what}: {r:?}");
        if r.is_some() != refuted {
            wrong.push(what);
        }
    }
    assert!(wrong.is_empty(), "the refuter got these wrong: {wrong:?}");
}

#[test]
fn review25_ambiguous_periods_can_choose_disjoint_orbits() {
    for deg in [true, false] {
        let mut fun = fixed_trig("sin(x)", TrigFn::Sin, 1, 0, 1, 0);
        fun.trig.as_mut().unwrap().deg = deg;
        let ctx = context(fun).unwrap();
        let mut half = if deg {
            Place::At(qi(180))
        } else {
            pi_place(qi(0), qi(1))
        };
        let mut full = if deg {
            Place::At(qi(360))
        } else {
            pi_place(qi(0), qi(2))
        };
        let h = half.enc();
        let f = full.enc();
        let list = |period: (f64, f64)| {
            json!([
                {"Every": {"x0": enc(0.0, 0.0), "period": enc(period.0, period.1)}},
                {"Every": {"x0": enc(h.0, h.1), "period": enc(period.0, period.1)}}
            ])
        };
        let v = list((h.0, f.1));
        assert_eq!(
            ctx.truth.judge("x_intercepts", &Scope::Line, &v),
            Verdict::True
        );
        assert_eq!(ctx.refute_row("x_intercepts", &Scope::Line, &v), None);
        assert_eq!(
            ctx.truth.judge("x_intercepts", &Scope::Line, &list(f)),
            Verdict::True
        );
        assert!(matches!(
            ctx.truth.judge("x_intercepts", &Scope::Line, &list(h)),
            Verdict::False(_)
        ));
        let broad = list((0.0, 10000.0 * f.1));
        assert!(matches!(
            ctx.truth.judge("x_intercepts", &Scope::Line, &broad),
            Verdict::Unknown(_)
        ));
    }
}

#[test]
fn review25_large_phase_cancellation_does_not_lose_a_zero() {
    let t = Trig {
        func: TrigFn::Sin,
        a: qi(1),
        b: -qd(1e20) + qi(8100),
        amp: qi(1),
        c: qi(0),
        deg: true,
    };
    let truth = Truth::new_trig(&t, (-1500.0, 1500.0));
    assert_eq!(t.val_cmp(0, &qd(1e20), &qi(0)), Equal);
    let v = json!([{"At": enc(1e20, 1e20)}]);
    assert_eq!(
        truth.judge("x_intercepts", &Scope::Points, &v),
        Verdict::True
    );
}

#[test]
fn review25_unbounded_monotone_enclosure_is_read_without_a_panic() {
    let truth = Truth::new(
        &Pw::one(Poly::new(vec![qi(0), qi(-1), qi(0), qi(1)])),
        &Dom::Line,
    );
    let v = json!([{"on": {
        "lo": {"At": {"x": enc(f64::NEG_INFINITY, -1.0), "closed": true}},
        "hi": {"At": {"x": enc(-0.7, -0.6), "closed": true}}},
        "dir": "Increasing"}]);
    assert_eq!(
        truth.judge("monotonicity", &Scope::Points, &v),
        Verdict::True
    );
    assert_eq!(
        truth.judge("monotonicity", &Scope::Window(-1.0, -0.6), &v),
        Verdict::True
    );
}

#[test]
fn review25_a_far_singleton_window_must_not_omit_its_zero() {
    let ctx = context(fixed_trig("sin(-2x-60)", TrigFn::Sin, -2, -60, 1, 0)).unwrap();
    let x = 1e20 - 10.0 * 16384.0;
    assert_eq!(
        ctx.truth
            .per
            .as_ref()
            .unwrap()
            .trig
            .val_cmp(0, &qd(x), &qi(0)),
        Equal
    );
    let scope = Scope::Window(x, x);
    println!(
        "far-window refuter: {:?}",
        ctx.refute_row("x_intercepts", &scope, &json!([]))
    );
    assert!(matches!(
        ctx.truth.judge("x_intercepts", &scope, &json!([])),
        Verdict::False(_)
    ));
}

#[test]
fn review25_genuine_certificate_search() {
    for text in [
        "sin(x)",
        "sin(-2x-60)",
        "sin(x-99999999999999991900)",
        "x^3-x",
    ] {
        for mode in [TrigUnit::Degrees, TrigUnit::Radians, TrigUnit::Grads] {
            let a = certify_text(
                text,
                CompileOptions {
                    trig_unit: mode,
                    ..CompileOptions::default()
                },
                DEFAULT_BUDGET,
                None,
            )
            .unwrap();
            let cert: Value = serde_json::to_value(a).unwrap();
            let report = replay::replay(&cert).unwrap();
            println!("genuine {text} {mode:?}: rows checked");
            println!(
                "genuine replay {text} {mode:?}: {:?}",
                report.row_problems()
            );
            assert!(report.row_problems().is_empty());
        }
    }
}

#[test]
fn review25_a_wide_constant_end_can_mean_the_domain_end() {
    let truth = Truth::new(&Pw::one(Poly::constant(qi(1))), &Dom::From(qi(0)));
    let v = json!([{"on": {"lo": {"At": {"x": enc(f64::NEG_INFINITY, 1.0), "closed": true}},
        "hi": "PosInf"}, "dir": "Constant"}]);
    assert_eq!(truth.judge("monotonicity", &Scope::Line, &v), Verdict::True);
}

// ------------------------------------------- review 25's class, swept

/// A degree trig f for the truth alone (no certificate).
fn trig_deg(func: TrigFn, a: i64, b: Rational) -> Trig {
    Trig {
        func,
        a: qi(a),
        b,
        amp: qi(1),
        c: qi(0),
        deg: true,
    }
}

/// The domain's poles in a box, counted from doubles: (a − x0)/P
/// rounded far out, and x0 itself rounded where it is far (R25-L-01's
/// two ways). cot(x − 60)° has a pole at the double 10²⁰ − 10·2¹⁴, R25-
/// L-01's: the box of it alone was "defined" (and "undefined"). tan(x −
/// 10²⁰ + 8100)°'s x0, 10²⁰ − 8010, rounds to 10²⁰, and its poles near 0
/// (10 + 180k) went uncounted: [0, 20] "defined", and a true piece
/// between two poles false, its ends no pole.
#[test]
fn poles_far_out_or_past_a_far_phase_are_counted_exactly() {
    let cot = Truth::new_trig(&trig_deg(TrigFn::Cot, 1, qi(-60)), (-752.0, 752.0));
    let x = 1e20 - 10.0 * 16384.0;
    assert!(cot.dom.pole(&Place::At(qd(x))));
    assert_eq!(cot.claim_true(&Claim::Defined((x, x))), Some(false));
    assert_eq!(cot.claim_true(&Claim::Undefined((x, x))), Some(true));
    let tan = Truth::new_trig(
        &trig_deg(TrigFn::Tan, 1, -qd(1e20) + qi(8100)),
        (-1500.0, 1500.0),
    );
    assert!(tan.dom.pole(&Place::At(qi(10))));
    assert_eq!(tan.claim_true(&Claim::Defined((0.0, 20.0))), Some(false));
    assert_eq!(tan.claim_true(&Claim::Defined((20.0, 180.0))), Some(true));
    let at = |x: f64| json!({"At": {"x": enc(x, x), "closed": false}});
    let row = json!([{"on": {"lo": at(10.0), "hi": at(190.0)}, "dir": "Increasing"}]);
    assert_eq!(tan.judge("monotonicity", &Scope::Line, &row), Verdict::True);
}

/// A monotone row over one period from 10²⁰: each piece's repeats about
/// it were counted in doubles, one either side, and sin(x)°'s two
/// pieces, true, left the period out (their repeats taken a dozen
/// periods past it).
#[test]
fn a_period_from_far_out_meets_the_pieces_own_repeats() {
    let sin = Truth::new_trig(&trig_deg(TrigFn::Sin, 1, qi(0)), (-1472.0, 1472.0));
    let at = |x: f64| json!({"At": {"x": enc(x, x), "closed": true}});
    let piece =
        |lo: f64, hi: f64, dir: &str| json!({"on": {"lo": at(lo), "hi": at(hi)}, "dir": dir});
    let row = json!([
        piece(-90.0, 90.0, "Increasing"),
        piece(90.0, 270.0, "Decreasing")
    ]);
    // (The next double past 10²⁰ is 2¹⁴ on: a window as wide as P.)
    let far = Scope::Period(1e20, 1e20 + 16384.0);
    assert_eq!(sin.judge("monotonicity", &far, &row), Verdict::True);
    let gap = json!([piece(-90.0, 90.0, "Increasing")]);
    assert!(matches!(
        sin.judge("monotonicity", &far, &gap),
        Verdict::False(_)
    ));
}

/// A periodic f's scope from −∞ (`real_enc` holds it a real stretch):
/// one period from −∞ sent −∞ to `qd`, as did a window from −∞ to the
/// features of a list. Now any start of a period does (the repeats are
/// one), and a list over a tail is not modelled, as an item over one
/// isn't.
#[test]
fn a_periodic_scope_from_minus_infinity_reaches_no_double() {
    let sin = Truth::new_trig(&trig_deg(TrigFn::Sin, 1, qi(0)), (-1472.0, 1472.0));
    let at = |x: f64| json!({"At": {"x": enc(x, x), "closed": true}});
    let piece =
        |lo: f64, hi: f64, dir: &str| json!({"on": {"lo": at(lo), "hi": at(hi)}, "dir": dir});
    let row = json!([
        piece(-90.0, 90.0, "Increasing"),
        piece(90.0, 270.0, "Decreasing")
    ]);
    let scope = Scope::Period(f64::NEG_INFINITY, 0.0);
    assert_eq!(sin.judge("monotonicity", &scope, &row), Verdict::True);
    let zeros = json!([{"Every": {"x0": enc(0.0, 0.0), "period": enc(180.0, 180.0)}}]);
    for scope in [
        Scope::Window(f64::NEG_INFINITY, 0.0),
        Scope::Window(0.0, f64::INFINITY),
    ] {
        assert!(matches!(
            sin.judge("x_intercepts", &scope, &zeros),
            Verdict::Unknown(_)
        ));
    }
}

/// A range's pieces were matched to its parts in the order of their low
/// ends' lower doubles; a low end enclosed from −∞ tied with "NegInf" and
/// kept the listed order. x³ with a hole at 0, (−∞, 0) ∪ (0, ∞), its
/// upper piece listed first from [−∞, 0] (0 in it, open): false.
#[test]
fn range_pieces_are_matched_to_parts_by_some_reading() {
    let t = Truth::new(
        &Pw::one(Poly::new(vec![qi(0), qi(0), qi(0), qi(1)])),
        &Dom::Hole(qi(0)),
    );
    let open = |lo: f64, hi: f64| json!({"At": {"x": enc(lo, hi), "closed": false}});
    let upper = json!({"lo": open(f64::NEG_INFINITY, 0.0), "hi": "PosInf"});
    let lower = json!({"lo": "NegInf", "hi": open(0.0, 0.0)});
    for row in [
        json!([upper.clone(), lower.clone()]),
        json!([lower.clone(), upper.clone()]),
    ] {
        assert_eq!(t.judge("range", &Scope::Line, &row), Verdict::True);
    }
    // (Each part needs a piece of its own.)
    let v = t.judge("range", &Scope::Line, &json!([upper.clone(), upper]));
    assert!(matches!(v, Verdict::False(_)), "{v:?}");
}
