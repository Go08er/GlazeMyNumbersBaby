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
    /// shown not to cross from monotonicity and the ends' signs; `cont`:
    /// the function is also continuous on the box (and so bounded on a
    /// finite one).
    Band {
        a: f64,
        b: f64,
        band: usize,
        mono: Option<usize>,
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
        self.key.total_cmp(&o.key) == Ordering::Equal
    }
}
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Item) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Item {
    // A max-heap: the nearest box (smallest key) first.
    fn cmp(&self, o: &Item) -> Ordering {
        o.key.total_cmp(&self.key)
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
            match step(fun, t, cs, a, b) {
                Ok(Step::Leaves(ls)) => {
                    found += ls
                        .iter()
                        .filter(|l| matches!(l, Leaf::Cross { .. } | Leaf::At { .. }))
                        .count();
                    leaves.extend(ls);
                }
                Ok(Step::Split(parts, points)) => {
                    found += points.len();
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

    /// True when every leaf is decided.
    pub fn complete(&self) -> bool {
        self.stopped.is_none() && !self.leaves.iter().any(|l| matches!(l, Leaf::Flag { .. }))
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

fn step(fun: &Fun<'_>, t: &Target<'_>, cs: &[f64], a: f64, b: f64) -> Result<Step, Stop> {
    let k = t.k;
    let s = fun.ser_of(t.expr, Interval::new(a, b), k + 1)?;
    let finite = a.is_finite() && b.is_finite();
    let tail_far = (!a.is_finite() && b <= -REACH) || (!b.is_finite() && a >= REACH);
    let split_or_flag = |why: &'static str| -> Result<Step, Stop> {
        if tail_far {
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
    if !valid {
        return split_or_flag("not defined or not smooth throughout");
    }
    let h = kth(&s, k);
    let cont = s[0].dec >= Dec::Dac && usable(&s, k);
    let j = match place(&h, cs) {
        Place::Band(i) => {
            return Ok(Step::Leaves(vec![Leaf::Band {
                a,
                b,
                band: i,
                mono: None,
                cont,
            }]));
        }
        Place::Exactly(j) if a == b => {
            return Ok(Step::Leaves(vec![Leaf::At { p: a, j }]));
        }
        Place::Exactly(j) => {
            return Ok(Step::Leaves(vec![Leaf::Equal { a, b, j, cont }]));
        }
        Place::Many => return split_with_points(fun, t, cs, a, b, &split_or_flag),
        Place::Touches(j) => j,
    };
    let c = cs[j];
    // A crossing of cⱼ: unique when h is continuous and strictly monotone
    // on the box.
    let d = kth(&s, k + 1);
    let mono = cont && usable(&s, k + 1) && d.ne0();
    let band_of = |above: bool| cs.iter().filter(|&&cc| cc < c).count() + usize::from(above);
    if mono && !finite && (a.is_finite() || b.is_finite()) {
        // A tail moving away from cⱼ from where it starts never reaches it.
        let rising = d.gt0();
        let start = if a.is_finite() { a } else { b };
        if let Some(p) = point(fun, t, start)?
            && let Some(above) = side(&p, c)
        {
            let away = if a.is_finite() {
                above == rising
            } else {
                above != rising
            };
            if away {
                return Ok(Step::Leaves(vec![Leaf::Band {
                    a,
                    b,
                    band: band_of(above),
                    mono: Some(j),
                    cont: true,
                }]));
            }
        }
    }
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
                        cont: true,
                    }]));
                }
                _ => {}
            }
        }
    }
    split_with_points(fun, t, cs, a, b, &split_or_flag)
}

/// Splits the box, checking whether h sits exactly on a threshold at the
/// split point (exact zeros at integers and dyadic points are common).
fn split_with_points(
    fun: &Fun<'_>,
    t: &Target<'_>,
    cs: &[f64],
    a: f64,
    b: f64,
    split_or_flag: &dyn Fn(&'static str) -> Result<Step, Stop>,
) -> Result<Step, Stop> {
    let Some(m) = split(a, b).filter(|_| !atomic(a, b)) else {
        return split_or_flag("not decided");
    };
    if let Some(v) = point(fun, t, m)? {
        for (j, &c) in cs.iter().enumerate() {
            if v.lo() == c && v.hi() == c {
                let mut parts = Vec::new();
                if a <= m.next_down() {
                    parts.push((a, m.next_down()));
                }
                if m.next_up() <= b {
                    parts.push((m.next_up(), b));
                }
                return Ok(Step::Split(parts, vec![Leaf::At { p: m, j }]));
            }
        }
    }
    Ok(Step::Split(vec![(a, m), (m, b)], vec![]))
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
                a, b, band, mono, ..
            } => {
                // Above every threshold below the band, below every one
                // above it; the one reached by the enclosure (if any) by
                // monotonicity.
                for (j, &c) in cs.iter().enumerate() {
                    let above = j < band;
                    if mono == Some(j) && a.is_finite() && b.is_finite() {
                        out.push(Claim::NoCross {
                            x: XBox::new(a, b),
                            of: of.clone(),
                            c: R(c),
                            above,
                        });
                    } else if mono == Some(j) {
                        out.push(Claim::TailNoCross {
                            side: if a.is_finite() {
                                Tail::Right
                            } else {
                                Tail::Left
                            },
                            from: R(if a.is_finite() { a } else { b }),
                            of: of.clone(),
                            c: R(c),
                            above,
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
            Leaf::Flag { .. } => {}
        }
    }
    out
}
