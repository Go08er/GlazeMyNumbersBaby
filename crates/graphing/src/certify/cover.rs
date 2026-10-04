//! Covering boxes of x by where a function stands against thresholds.
//!
//! [`Cover::run`] takes a target h (f⁽ᵏ⁾, or a sub-expression of f), an
//! ascending list of thresholds `c₀ < c₁ < …`, and boxes of x, and splits
//! the boxes until each is decided:
//!
//! * `Band`: h is valid on the box and strictly between two thresholds
//!   (band i lies above i of them), from its enclosure or, when it is
//!   strictly monotone there, from both ends;
//! * `Cross`: h is continuous and strictly monotone on the box and passes
//!   cⱼ exactly once (opposite strict signs of h − cⱼ at the two ends, or
//!   h = cⱼ exactly at a double), located in a narrow box;
//! * `At`: h = cⱼ exactly at a double (a point box);
//! * `Touch`: h = cⱼ exactly at an end of the box and strictly on one side
//!   of it elsewhere there (a double root at a double, by Taylor's theorem);
//! * `Equal`: h ≡ cⱼ on the box;
//! * `Undefined`: h is defined nowhere on the box;
//! * `Flag`: undecided (too narrow to split, out of budget, beyond reach).
//!
//! Boxes nearest the origin are decided first, so when the budget runs out
//! what is left undecided is far out.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use super::cert::{Claim, R, Subject, Tail, XBox};
use super::fun::{Fun, REACH, Stop, atomic, split, usable};
use crate::ast::Expr;
use crate::interval::{Dec, DecInterval, Interval, Series};

/// Boxes within this of 0 are not split (see `step`).
const NEAR: f64 = 1e-300;
const NEAR_END: f64 = 1e-150;

/// The most crossings a cover lists before leaving the rest undecided.
pub const MAX_FEATURES: usize = 64;

/// Below this (relative to the threshold) a value that hasn't been decided
/// is lost to underflow: splitting further can't help.
const TINY: f64 = 1e-290;

/// What a cover is about.
#[derive(Clone)]
pub struct Target<'e> {
    /// The expression: f's tree or one of its sub-trees.
    pub expr: &'e Expr,
    /// The derivative order: 0 for the value, 1 for f′, 2 for f″.
    pub k: usize,
}

/// One decided box.
#[derive(Clone, Debug, PartialEq)]
pub enum Leaf {
    /// h is strictly inside band `band` on `[a, b]` (an end may be ±∞: a
    /// tail); `mono`: the enclosure reached threshold `mono`, which h is
    /// shown not to cross from monotonicity and the ends' signs; `chain`
    /// (a tail, when not 0): every threshold is shown not crossed by the
    /// sign chain through h's derivatives up to that order (see
    /// [`Claim::TailChain`]); `cont`: the function is also continuous on
    /// the box (and so bounded on a finite one).
    Band {
        a: f64,
        b: f64,
        band: usize,
        mono: Option<usize>,
        chain: usize,
        /// Decided from h's zero factors (see `by_factors`), not its
        /// enclosure.
        factors: bool,
        cont: bool,
    },
    /// h is continuous and strictly monotone on `[a, b]` (rising or not)
    /// and passes threshold `j` exactly once there: in `[l, r]`, or at the
    /// double `exact`.
    Cross {
        a: f64,
        b: f64,
        l: f64,
        r: f64,
        j: usize,
        exact: Option<f64>,
        rising: bool,
    },
    /// h(p) = cⱼ exactly.
    At { p: f64, j: usize },
    /// h = cⱼ exactly at the end `p` of `[a, b]` and strictly on side
    /// `above` of it on the rest of the box: Taylor's theorem at p, with
    /// the coefficients of orders 1 to `order` − 1 exactly 0 there and the
    /// order-`order` one away from 0 over the whole box.
    Touch {
        a: f64,
        b: f64,
        p: f64,
        j: usize,
        order: usize,
        above: bool,
    },
    /// h ≡ cⱼ on `[a, b]` (continuous there when `cont`).
    Equal {
        a: f64,
        b: f64,
        j: usize,
        cont: bool,
    },
    /// h is defined nowhere on `[a, b]`.
    Undefined { a: f64, b: f64 },
    /// Not decided.
    Flag { a: f64, b: f64, why: &'static str },
}

impl Leaf {
    /// The part of the line the leaf decides.
    pub fn span(&self) -> (f64, f64) {
        match *self {
            Leaf::Band { a, b, .. }
            | Leaf::Equal { a, b, .. }
            | Leaf::Undefined { a, b }
            | Leaf::Flag { a, b, .. }
            | Leaf::Touch { a, b, .. }
            | Leaf::Cross { a, b, .. } => (a, b),
            Leaf::At { p, .. } => (p, p),
        }
    }
}

/// The result of a cover: leaves in x order, and whether it stopped early.
#[derive(Clone, Debug)]
pub struct Cover {
    pub leaves: Vec<Leaf>,
    pub stopped: Option<Stop>,
}

struct Item {
    a: f64,
    b: f64,
    key: f64,
}

impl PartialEq for Item {
    fn eq(&self, o: &Item) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Item) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Item {
    // A max-heap: the nearest box (smallest key) first, and of two as near
    // the narrower (a point box before the box it ends), so exact points
    // are known before the boxes that touch them.
    fn cmp(&self, o: &Item) -> Ordering {
        o.key
            .total_cmp(&self.key)
            .then((o.b - o.a).total_cmp(&(self.b - self.a)))
    }
}

fn item(a: f64, b: f64) -> Item {
    let key = if a <= 0.0 && b >= 0.0 {
        0.0
    } else {
        a.abs().min(b.abs())
    };
    Item { a, b, key }
}

/// Where an enclosure stands against the thresholds.
enum Place {
    /// Strictly inside band i (above exactly i thresholds).
    Band(usize),
    /// Exactly the threshold cⱼ (a point enclosure).
    Exactly(usize),
    /// Reaches threshold cⱼ and no other.
    Touches(usize),
    /// Reaches several thresholds.
    Many,
}

/// Strict side of `v` against `c`: above (true), below (false), or not
/// decided. Strict-sign facts count against 0.
pub fn side(v: &DecInterval, c: f64) -> Option<bool> {
    if v.is_empty() {
        return None;
    }
    if v.lo() > c || (c == 0.0 && v.gt0()) {
        Some(true)
    } else if v.hi() < c || (c == 0.0 && v.lt0()) {
        Some(false)
    } else {
        None
    }
}

fn place(v: &DecInterval, cs: &[f64]) -> Place {
    let mut touch = None;
    for (j, &c) in cs.iter().enumerate() {
        if side(v, c).is_none() {
            if v.lo() == c && v.hi() == c {
                return Place::Exactly(j);
            }
            if touch.is_some() {
                return Place::Many;
            }
            touch = Some(j);
        }
    }
    match touch {
        Some(j) => Place::Touches(j),
        None => Place::Band(cs.iter().filter(|&&c| side(v, c) == Some(true)).count()),
    }
}

impl Cover {
    /// Covers `boxes` (closed, possibly with infinite ends) for `t` against
    /// the thresholds `cs` (ascending).
    pub fn run(fun: &Fun<'_>, t: &Target<'_>, cs: &[f64], boxes: &[(f64, f64)]) -> Cover {
        let mut heap: BinaryHeap<Item> = boxes
            .iter()
            .filter(|(a, b)| a <= b)
            .map(|&(a, b)| item(a, b))
            .collect();
        let mut leaves = Vec::new();
        let mut stopped = None;
        let mut found = 0;
        // Doubles where h is exactly a threshold (point, threshold).
        let mut exacts: Vec<(f64, usize)> = Vec::new();
        while let Some(Item { a, b, .. }) = heap.pop() {
            if stopped.is_some() {
                leaves.push(Leaf::Flag {
                    a,
                    b,
                    why: "budget",
                });
                continue;
            }
            // A function with this many crossings near the origin has
            // more further out than a list can show (sin x): what's left,
            // furthest out, stays undecided.
            if found >= MAX_FEATURES {
                leaves.push(Leaf::Flag {
                    a,
                    b,
                    why: "too many features",
                });
                continue;
            }
            match step(fun, t, cs, a, b, &exacts) {
                Ok(Step::Leaves(ls)) => {
                    // Each crossing once (one at a known exact point is
                    // already counted).
                    found += ls
                        .iter()
                        .filter(|l| match **l {
                            Leaf::At { .. } | Leaf::Cross { exact: None, .. } => true,
                            Leaf::Cross { exact: Some(p), .. } => !exacts.iter().any(|e| e.0 == p),
                            _ => false,
                        })
                        .count();
                    exacts.extend(ls.iter().filter_map(|l| match *l {
                        Leaf::At { p, j } => Some((p, j)),
                        _ => None,
                    }));
                    leaves.extend(ls);
                }
                Ok(Step::Split(parts, points)) => {
                    found += points.len();
                    exacts.extend(points.iter().filter_map(|l| match *l {
                        Leaf::At { p, j } => Some((p, j)),
                        _ => None,
                    }));
                    leaves.extend(points);
                    for (l, h) in parts {
                        heap.push(item(l, h));
                    }
                }
                Err(s) => {
                    stopped = Some(s);
                    leaves.push(Leaf::Flag {
                        a,
                        b,
                        why: "budget",
                    });
                }
            }
        }
        leaves.sort_by(|x, y| {
            let (xa, xb) = x.span();
            let (ya, yb) = y.span();
            xa.total_cmp(&ya).then(xb.total_cmp(&yb))
        });
        Cover { leaves, stopped }
    }

    /// True when every leaf is decided, for a cover of boxes inside f's
    /// domain: a box where the evaluated tree is undefined contradicts the
    /// domain (a simplified form read differently), so it counts as
    /// undecided.
    pub fn complete(&self) -> bool {
        self.stopped.is_none()
            && !self
                .leaves
                .iter()
                .any(|l| matches!(l, Leaf::Flag { .. } | Leaf::Undefined { .. }))
    }
}

enum Step {
    Leaves(Vec<Leaf>),
    /// Boxes to look at again, and point leaves found on the way.
    Split(Vec<(f64, f64)>, Vec<Leaf>),
}

fn kth(s: &Series, k: usize) -> DecInterval {
    s.get(k).copied().unwrap_or_else(DecInterval::unknown)
}

/// h's coefficient at a point, if valid there.
pub fn point(fun: &Fun<'_>, t: &Target<'_>, p: f64) -> Result<Option<DecInterval>, Stop> {
    let s = fun.ser_of(t.expr, Interval::point(p), t.k)?;
    let ok = if t.k == 0 {
        !s[0].is_empty() && s[0].dec >= Dec::Def
    } else {
        usable(&s, t.k)
    };
    Ok(ok.then(|| kth(&s, t.k)))
}

fn step(
    fun: &Fun<'_>,
    t: &Target<'_>,
    cs: &[f64],
    a: f64,
    b: f64,
    exacts: &[(f64, usize)],
) -> Result<Step, Stop> {
    let k = t.k;
    let s = fun.ser_of(t.expr, Interval::new(a, b), k + 1)?;
    let finite = a.is_finite() && b.is_finite();
    // Beyond reach, an undecided box is left so: features out there are
    // past anything the panel shows, and splitting there (underflow,
    // overflow) rarely helps.
    // So too a box within 10⁻³⁰⁰ of 0 (subnormal x, where f′ of 1/x or
    // ln x overflows).
    // (A box from there up to 10⁻¹⁵⁰ too: splitting it would only approach
    // 10⁻³⁰⁰ ever more finely.)
    let near0 = a < b
        && ((a >= 0.0 && a <= NEAR && b <= NEAR_END) || (b <= 0.0 && b >= -NEAR && a >= -NEAR_END));
    let far = a >= REACH || b <= -REACH || near0;
    let split_or_flag = |why: &'static str| -> Result<Step, Stop> {
        if far {
            return Ok(Step::Leaves(vec![Leaf::Flag {
                a,
                b,
                why: "beyond reach",
            }]));
        }
        match split(a, b) {
            Some(m) if !atomic(a, b) => Ok(Step::Split(vec![(a, m), (m, b)], vec![])),
            _ => Ok(Step::Leaves(vec![Leaf::Flag { a, b, why }])),
        }
    };
    if s[0].is_empty() {
        return Ok(Step::Leaves(vec![Leaf::Undefined { a, b }]));
    }
    let valid = if k == 0 {
        s[0].dec >= Dec::Def
    } else {
        usable(&s, k)
    };
    // Splitting further toward a point where h isn't valid (an end of the
    // domain, a kink) or that h only touches without a proof can't make the
    // cover complete; stop at a narrow box instead of bisecting down
    // through the subnormals.
    let narrow = finite && b - a <= 1e-12 * a.abs().max(b.abs()).max(1.0);
    if !valid {
        if narrow {
            return Ok(Step::Leaves(vec![Leaf::Flag {
                a,
                b,
                why: "not defined or not smooth throughout",
            }]));
        }
        return split_or_flag("not defined or not smooth throughout");
    }
    let h = kth(&s, k);
    let cont = s[0].dec >= Dec::Dac && usable(&s, k);
    let touched = match place(&h, cs) {
        Place::Band(i) => {
            return Ok(Step::Leaves(vec![Leaf::Band {
                a,
                b,
                band: i,
                mono: None,
                chain: 0,
                factors: false,
                cont,
            }]));
        }
        Place::Exactly(j) if a == b => {
            return Ok(Step::Leaves(vec![Leaf::At { p: a, j }]));
        }
        Place::Exactly(j) => {
            return Ok(Step::Leaves(vec![Leaf::Equal { a, b, j, cont }]));
        }
        Place::Many => None,
        Place::Touches(j) => Some(j),
    };
    if !finite && (a.is_finite() || b.is_finite())
        && let Some(leaf) = tail_chain(fun, t, cs, a, b)?
    {
        return Ok(Step::Leaves(vec![leaf]));
    }
    let Some(j) = touched else {
        return split_with_points(fun, t, cs, a, b, far, &split_or_flag);
    };
    let c = cs[j];
    // Factor by factor (h's zeros are among its factors'): h continuous
    // with no factor reaching 0 keeps one sign, or with each that does
    // exactly 0 at an end and strictly monotone, is 0 only there.
    if k == 0 && c == 0.0 && cs.len() == 1 && cont {
        let fs = super::fun::zero_factors(t.expr);
        if fs.len() != 1 || fs[0] != *t.expr {
            let ends: Vec<f64> = exacts
                .iter()
                .filter(|e| e.1 == j && finite && (e.0 == a || e.0 == b))
                .map(|e| e.0)
                .collect();
            if let Some(leaf) = by_factors(fun, &fs, a, b, &ends, t)? {
                return Ok(Step::Leaves(vec![leaf]));
            }
        }
    }
    // A crossing of cⱼ: unique when h is continuous and strictly monotone
    // on the box.
    let d = kth(&s, k + 1);
    let mono = cont && usable(&s, k + 1) && d.ne0();
    let band_of = |above: bool| cs.iter().filter(|&&cc| cc < c).count() + usize::from(above);
    if !mono && c == 0.0 && h.iv.mig_mag().1 <= TINY {
        // Underflow: below anything a double can tell apart here.
        if !atomic(a, b) && finite {
            return Ok(Step::Leaves(vec![Leaf::Flag {
                a,
                b,
                why: "underflow",
            }]));
        }
    }
    if mono && finite {
        let rising = d.gt0();
        if let (Some(pa), Some(pb)) = (point(fun, t, a)?, point(fun, t, b)?) {
            let exact = |v: &DecInterval| v.lo() == c && v.hi() == c;
            if exact(&pa) || exact(&pb) {
                let p = if exact(&pa) { a } else { b };
                return Ok(Step::Leaves(vec![Leaf::Cross {
                    a,
                    b,
                    l: p,
                    r: p,
                    j,
                    exact: Some(p),
                    rising,
                }]));
            }
            match (side(&pa, c), side(&pb, c)) {
                (Some(sa), Some(sb)) if sa != sb => {
                    return refine(fun, t, c, j, a, b, sa, rising).map(|l| Step::Leaves(vec![l]));
                }
                (Some(sa), Some(_)) => {
                    // Monotone with both ends on one side of cⱼ: no
                    // crossing, and no other threshold is reached.
                    return Ok(Step::Leaves(vec![Leaf::Band {
                        a,
                        b,
                        band: band_of(sa),
                        mono: Some(j),
                        chain: 0,
                        factors: false,
                        cont: true,
                    }]));
                }
                _ => {}
            }
        }
    }
    // Touching cⱼ at an end where h is exactly cⱼ (a double root there).
    if finite {
        for &(p, pj) in exacts {
            if pj == j && (p == a || p == b) {
                if let Some(leaf) = touch(fun, t, c, j, a, b, p)? {
                    return Ok(Step::Leaves(vec![leaf]));
                }
                if narrow {
                    return Ok(Step::Leaves(vec![Leaf::Flag {
                        a,
                        b,
                        why: "touch not proven",
                    }]));
                }
            }
        }
    }
    split_with_points(fun, t, cs, a, b, far, &split_or_flag)
}

/// h (continuous on `[a, b]`) from its zero factors `fs`: each away from 0
/// on the box, or exactly 0 at one end `p` of `ends` and strictly
/// monotone there. Then h ≠ 0 on the box (but at p), of the sign it has at
/// a point inside: a band, or a touch at p.
fn by_factors(
    fun: &Fun<'_>,
    fs: &[Expr],
    a: f64,
    b: f64,
    ends: &[f64],
    t: &Target<'_>,
) -> Result<Option<Leaf>, Stop> {
    let iv = Interval::new(a, b);
    let mut at: Option<f64> = None;
    for h in fs {
        let s = fun.ser_of(h, iv, 1)?;
        if !s[0].is_empty() && s[0].ne0() {
            continue;
        }
        if !(usable(&s, 1) && s[1].ne0()) {
            return Ok(None);
        }
        let mut hit = None;
        for &p in ends {
            let v = fun.ser_of(h, Interval::point(p), 0)?[0];
            if v.lo() == 0.0 && v.hi() == 0.0 {
                hit = Some(p);
            }
        }
        match (hit, at) {
            (Some(p), None) => at = Some(p),
            (Some(p), Some(q)) if p == q => {}
            _ => return Ok(None),
        }
    }
    // The sign, at a point of the box other than the zero.
    let q = match (a.is_finite(), b.is_finite(), at) {
        (true, true, Some(p)) => if p == a { b } else { a },
        (true, true, None) => a / 2.0 + b / 2.0,
        (true, false, _) => a.abs().max(1.0) * 2.0 + a,
        (false, true, _) => b - b.abs().max(1.0) * 2.0,
        (false, false, _) => 0.0,
    };
    let v = fun.ser_of(t.expr, Interval::point(q), 0)?[0];
    if v.is_empty() || v.dec < Dec::Def || !v.ne0() {
        return Ok(None);
    }
    let above = v.gt0();
    Ok(Some(match at {
        Some(p) => Leaf::Touch {
            a,
            b,
            p,
            j: 0,
            order: 0,
            above,
        },
        None => Leaf::Band {
            a,
            b,
            band: usize::from(above),
            mono: None,
            chain: 0,
            factors: true,
            cont: true,
        },
    }))
}

/// The derivative orders a tail's sign chain looks up to.
const CHAIN_ORDER: usize = 3;

/// A tail `[a, ∞)` or `(−∞, b]` on which h moves away from every
/// threshold: h⁽ⁿ⁾ strictly of one sign σ there (n ≤ 3), and at the finite
/// end h⁽ⁱ⁾ for 1 ≤ i < n, and h − c for every threshold c, strictly of the
/// sign σ forces (σ on the right; σ·(−1)ⁿ⁻ⁱ on the left, where a positive
/// derivative means smaller values further out). Each then keeps its sign
/// out along the tail, one order at a time. This decides tails whose
/// enclosure suffers ∞ − ∞ (x⁴ − x²) once a higher derivative doesn't.
pub fn tail_chain(fun: &Fun<'_>, t: &Target<'_>, cs: &[f64], a: f64, b: f64) -> Result<Option<Leaf>, Stop> {
    let k = t.k;
    let right = a.is_finite();
    let m = if right { a } else { b };
    let over = fun.ser_of(t.expr, Interval::new(a, b), k + CHAIN_ORDER)?;
    let mut at: Option<Series> = None;
    for n in 1..=CHAIN_ORDER {
        let d = kth(&over, k + n);
        if !(usable(&over, k + n) && d.ne0()) {
            continue;
        }
        let sigma = d.gt0();
        let need = |i: usize| if right { sigma } else { sigma == (n - i).is_multiple_of(2) };
        if at.is_none() {
            at = Some(fun.ser_of(t.expr, Interval::point(m), k + CHAIN_ORDER - 1)?);
        }
        let at = at.as_ref().expect("just set");
        if !usable(at, k + n - 1) {
            return Ok(None);
        }
        if !(1..n).all(|i| side(&kth(at, k + i), 0.0) == Some(need(i))) {
            continue;
        }
        let h = kth(at, k);
        let above = need(0);
        if !cs.iter().all(|&c| side(&h, c) == Some(above)) {
            continue;
        }
        return Ok(Some(Leaf::Band {
            a,
            b,
            band: if above { cs.len() } else { 0 },
            mono: None,
            chain: n,
            factors: false,
            cont: true,
        }));
    }
    Ok(None)
}

/// The Taylor orders a touch is looked for up to.
const TOUCH_ORDER: usize = 3;

/// h = c exactly at the end p of `[a, b]`: is h − c of one strict sign on
/// the rest of the box? With h's Taylor coefficients at p of orders 1 to
/// n − 1 exactly 0 and the order-n coefficient over the box away from 0,
/// h(x) − c = h⁽ⁿ⁾(ξ)/n!·(x − p)ⁿ for some ξ in the box (Lagrange's
/// remainder), so its sign is that coefficient's, times (−1)ⁿ left of p.
fn touch(
    fun: &Fun<'_>,
    t: &Target<'_>,
    c: f64,
    j: usize,
    a: f64,
    b: f64,
    p: f64,
) -> Result<Option<Leaf>, Stop> {
    let k = t.k;
    let at = fun.ser_of(t.expr, Interval::point(p), k + TOUCH_ORDER)?;
    let over = fun.ser_of(t.expr, Interval::new(a, b), k + TOUCH_ORDER)?;
    let zero = |v: &DecInterval| v.dec >= Dec::Def && v.lo() == 0.0 && v.hi() == 0.0;
    let exact = kth(&at, k);
    if !(exact.dec >= Dec::Def && exact.lo() == c && exact.hi() == c) {
        return Ok(None);
    }
    for n in 1..=TOUCH_ORDER {
        if n > 1 && !zero(&kth(&at, k + n - 1)) {
            break;
        }
        let d = kth(&over, k + n);
        if !(usable(&over, k + n) && d.ne0()) {
            continue;
        }
        // The coefficients of h = f⁽ᵏ⁾ are positive multiples of f's.
        let left = p == b;
        let above = d.gt0() != (left && n % 2 == 1);
        return Ok(Some(Leaf::Touch {
            a,
            b,
            p,
            j,
            order: n,
            above,
        }));
    }
    Ok(None)
}

/// Splits the box, checking whether h sits exactly on a threshold at the
/// split point (exact zeros at integers and dyadic points are common).
fn split_with_points(
    fun: &Fun<'_>,
    t: &Target<'_>,
    cs: &[f64],
    a: f64,
    b: f64,
    far: bool,
    split_or_flag: &dyn Fn(&'static str) -> Result<Step, Stop>,
) -> Result<Step, Stop> {
    let Some(mut m) = split(a, b).filter(|_| !atomic(a, b) && !far) else {
        return split_or_flag("not decided");
    };
    let exact_at = |v: &Option<DecInterval>| {
        v.as_ref()
            .and_then(|v| cs.iter().position(|&c| v.lo() == c && v.hi() == c))
    };
    let decided = |v: &Option<DecInterval>| v.as_ref().is_some_and(|v| cs.iter().all(|&c| side(v, c).is_some()));
    let mut v = point(fun, t, m)?;
    if exact_at(&v).is_none() && !decided(&v) {
        // The cut sits where h's sign can't be told (on or next to a root):
        // both halves would end there undecided. Cut off-centre.
        let m2 = match (a.is_finite(), b.is_finite()) {
            (true, true) => a + 0.382 * (b - a),
            (true, false) => m + 0.618 * (m - a),
            (false, true) => m - 0.618 * (b - m),
            (false, false) => 0.382,
        };
        if m2 > a && m2 < b {
            let v2 = point(fun, t, m2)?;
            if exact_at(&v2).is_some() || decided(&v2) {
                (m, v) = (m2, v2);
            }
        }
    }
    // Both halves keep m (no real between two doubles is left out); h at
    // m itself is the point leaf.
    let points = match exact_at(&v) {
        Some(j) => vec![Leaf::At { p: m, j }],
        None => vec![],
    };
    Ok(Step::Split(vec![(a, m), (m, b)], points))
}

/// Narrows a proven crossing of `c` in `[a, b]` (monotone there, h − c of
/// sign `sa` at a) by bisection on point signs.
#[allow(clippy::too_many_arguments)]
fn refine(
    fun: &Fun<'_>,
    t: &Target<'_>,
    c: f64,
    j: usize,
    a: f64,
    b: f64,
    sa: bool,
    rising: bool,
) -> Result<Leaf, Stop> {
    let (mut l, mut r) = (a, b);
    let mut exact = None;
    for _ in 0..90 {
        let rel = 4e-16 * l.abs().max(r.abs());
        if r - l <= rel.max(f64::from_bits(1)) {
            break;
        }
        let Some(m) = split(l, r) else { break };
        let Some(v) = point(fun, t, m)? else { break };
        if v.lo() == c && v.hi() == c {
            exact = Some(m);
            break;
        }
        match side(&v, c) {
            Some(sm) if sm == sa => l = m,
            Some(_) => r = m,
            None => break,
        }
    }
    // A crossing a few doubles wide may be exactly at one of them (x − 2
    // at 2): try each, so it is known as a double, not an enclosure.
    if exact.is_none() {
        let mut p = l.next_up();
        for _ in 0..8 {
            if p >= r {
                break;
            }
            if let Some(v) = point(fun, t, p)?
                && v.lo() == c
                && v.hi() == c
            {
                exact = Some(p);
                break;
            }
            p = p.next_up();
        }
    }
    let (l, r) = match exact {
        Some(m) => (m, m),
        None => (l, r),
    };
    Ok(Leaf::Cross {
        a,
        b,
        l,
        r,
        j,
        exact,
        rising,
    })
}

/// "Strictly above or below `c` on `[a, b]`", as a box or a tail claim.
fn beyond(a: f64, b: f64, of: &Subject, c: f64, above: bool) -> Claim {
    if a.is_finite() && b.is_finite() {
        Claim::Beyond {
            x: XBox::new(a, b),
            of: of.clone(),
            c: R(c),
            above,
        }
    } else if !b.is_finite() {
        Claim::TailBeyond {
            side: Tail::Right,
            from: R(a),
            of: of.clone(),
            c: R(c),
            above,
        }
    } else {
        Claim::TailBeyond {
            side: Tail::Left,
            from: R(b),
            of: of.clone(),
            c: R(c),
            above,
        }
    }
}

/// The claims behind a cover of `of` against the thresholds `cs`, for a
/// certificate.
pub fn claims(cover: &Cover, of: &Subject, cs: &[f64]) -> Vec<Claim> {
    let mut out = Vec::new();
    for l in &cover.leaves {
        match *l {
            Leaf::Band {
                a,
                b,
                band,
                factors: true,
                ..
            } => out.push(Claim::Factors {
                x: XBox::new(a, b),
                of: of.clone(),
                c: R(cs[0]),
                above: band > 0,
            }),
            Leaf::Band {
                a,
                b,
                band,
                mono,
                chain,
                ..
            } => {
                // Above every threshold below the band, below every one
                // above it; the one reached by the enclosure (if any) by
                // monotonicity, or all of them by a tail's sign chain.
                for (j, &c) in cs.iter().enumerate() {
                    let above = j < band;
                    if mono == Some(j) && a.is_finite() && b.is_finite() {
                        out.push(Claim::NoCross {
                            x: XBox::new(a, b),
                            of: of.clone(),
                            c: R(c),
                            above,
                        });
                    } else if chain > 0 {
                        out.push(Claim::TailChain {
                            side: if a.is_finite() {
                                Tail::Right
                            } else {
                                Tail::Left
                            },
                            from: R(if a.is_finite() { a } else { b }),
                            of: of.clone(),
                            c: R(c),
                            above,
                            order: chain as u8,
                        });
                    } else {
                        out.push(beyond(a, b, of, c, above));
                    }
                }
            }
            Leaf::Cross {
                a,
                b,
                l,
                r,
                j,
                exact,
                ..
            } => match exact {
                Some(p) => out.push(Claim::ExactAt {
                    x: XBox::new(a, b),
                    of: of.clone(),
                    c: R(cs[j]),
                    at: R(p),
                }),
                None => {
                    out.push(Claim::OneCross {
                        x: XBox::new(a, b),
                        of: of.clone(),
                        c: R(cs[j]),
                    });
                    out.push(Claim::OneCross {
                        x: XBox::new(l, r),
                        of: of.clone(),
                        c: R(cs[j]),
                    });
                }
            },
            Leaf::At { p, j } => out.push(Claim::Value {
                x: XBox::point(p),
                of: of.clone(),
                lo: R(cs[j]),
                hi: R(cs[j]),
            }),
            Leaf::Equal { a, b, j, .. } => out.push(Claim::Value {
                x: XBox::new(a, b),
                of: of.clone(),
                lo: R(cs[j]),
                hi: R(cs[j]),
            }),
            Leaf::Undefined { a, b } => out.push(Claim::Undefined {
                x: XBox::new(a, b),
            }),
            Leaf::Touch {
                a,
                b,
                p,
                j,
                order,
                above,
            } => out.push(Claim::Touch {
                x: XBox::new(a, b),
                of: of.clone(),
                c: R(cs[j]),
                at: R(p),
                order: order as u8,
                above,
            }),
            Leaf::Flag { .. } => {}
        }
    }
    out
}
