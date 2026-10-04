//! The rows, from the domain and the covers of f, f′ and f″.

use super::cert::*;
use super::cover::{Cover, Leaf, claims};
use super::fun::{Fun, Stop, usable};
use super::pole::{bounded_near, end_pole, pole, pole_free};
use super::side::{Domain, IBox};
use crate::analysis::format::Nice;
use crate::interval::{Dec, DecInterval, Interval};
use serde::{Deserialize, Serialize};

/// Whether every leaf of a cover is decided and the gaps beside excluded
/// points are clear (`clear`, from [`gaps_clear`]): an undecided box
/// anywhere leaves the row incomplete, as a zero or a turn could hide
/// there. Returns no extra claims (the gaps' go with every row).
pub fn settle(cover: &Cover, clear: bool) -> (Vec<Claim>, bool) {
    (Vec::new(), cover.complete() && clear)
}

/// What the boxes cover.
#[derive(Clone, Copy, Debug)]
pub struct Scope {
    /// The boxes reach the whole line.
    pub whole: bool,
    /// Otherwise the window they stop at.
    pub window: (f64, f64),
    /// The window is one period of f, proven periodic with this period.
    pub period: Option<Enc>,
    /// Where the tails start.
    pub w: f64,
}

impl Scope {
    /// The region a complete cover's claims reach; over one period of a
    /// periodic f, the line by periodicity if the row repeats with f
    /// (`repeats`).
    pub fn region(&self, complete: bool, repeats: bool) -> Option<Region> {
        let (a, b) = (R(self.window.0), R(self.window.1));
        complete.then(|| {
            if self.whole {
                Region::Line
            } else if repeats && self.period.is_some() {
                Region::Period { a, b }
            } else {
                Region::Window { a, b }
            }
        })
    }
}

/// Is `x` (enclosed) a repeat of one of `xs` by a multiple of the period?
fn repeats(xs: &[Enc], x: &Enc, period: &Enc) -> bool {
    xs.iter().any(|e| {
        let k = ((x.mid() - e.mid()) / period.mid()).round();
        k != 0.0 && {
            let (lo, hi) = (e.lo.0 + k * period.lo.0, e.hi.0 + k * period.hi.0);
            let (lo, hi) = (lo.min(hi), lo.max(hi));
            let slack = 1e-9 * x.mid().abs().max(1.0);
            lo - slack <= x.hi.0 && x.lo.0 <= hi + slack
        }
    })
}

fn row_of<T>(
    value: T,
    region: Option<Region>,
    has_items: bool,
    claims: Vec<Claim>,
    why: &str,
) -> Row<T> {
    match region {
        Some(r @ (Region::Line | Region::Period { .. })) => {
            let mut c = Certificate::new(r);
            c.extend(claims);
            Row::Certified { value, cert: c }
        }
        Some(r) => {
            let mut c = Certificate::new(r);
            c.extend(claims);
            Row::Partial { value, cert: c }
        }
        None if has_items => {
            let mut c = Certificate::new(Region::Points);
            c.extend(claims);
            Row::Partial { value, cert: c }
        }
        None => Row::unknown(why),
    }
}

/// Why a row is unknown when f's domain isn't decided.
pub const UNDECIDED: &str = "the domain is not decided";

/// With f's domain not decided, a row found on the boxes where f's own
/// tree is shown defined (`defined`, the boxes claimed Defined): only the
/// items inside one (a turn strictly inside, f defined on either side of
/// it), and never all of them.
pub fn scoped<T>(
    row: Row<Vec<T>>,
    defined: &[(f64, f64)],
    turn: bool,
    at: impl Fn(&T) -> Option<Enc>,
) -> Row<Vec<T>> {
    let (items, mut cert) = match row {
        Row::Certified { value, cert } | Row::Partial { value, cert } => (value, cert),
        u @ Row::Unknown { .. } => return u,
    };
    let inside = |x: Enc| {
        defined.iter().any(|&(a, b)| {
            if turn {
                a < x.lo.0 && x.hi.0 < b
            } else {
                a <= x.lo.0 && x.hi.0 <= b
            }
        })
    };
    let kept: Vec<T> = items
        .into_iter()
        .filter(|it| at(it).is_some_and(inside))
        .collect();
    if kept.is_empty() {
        return Row::unknown(UNDECIDED);
    }
    cert.covers = Region::Points;
    Row::Partial { value: kept, cert }
}

fn enc_of(l: f64, r: f64) -> Enc {
    Enc::new(l, r)
}

// ---------------------------------------------------------- x-intercepts

pub fn zeros(c0: &Cover, scope: &Scope, extra: &[Claim], clear: bool) -> Row<Vec<Spot>> {
    let mut out: Vec<Spot> = Vec::new();
    for l in &c0.leaves {
        match *l {
            Leaf::Cross { l, r, exact, .. } => out.push(Spot::At(match exact {
                Some(p) => Enc::point(p),
                None => enc_of(l, r),
            })),
            Leaf::At { p, .. } => out.push(Spot::At(Enc::point(p))),
            Leaf::Equal { .. } => return Row::unknown("f is 0 on a whole stretch"),
            _ => {}
        }
    }
    out.dedup();
    if let Some(period) = scope.period {
        // Over one period: each zero repeats.
        let mut seen: Vec<Enc> = Vec::new();
        out.retain(|s| match s {
            Spot::At(x) => {
                let keep = !repeats(&seen, x, &period);
                seen.push(*x);
                keep
            }
            Spot::Every(_) => true,
        });
        for s in out.iter_mut() {
            if let Spot::At(x) = *s {
                *s = Spot::Every(Family { x0: x, period });
            }
        }
    }
    let (gaps, complete) = settle(c0, clear);
    let mut cl = claims(c0, &Subject::f(0), &[0.0]);
    cl.extend(gaps);
    cl.extend(extra.iter().cloned());
    let has = !out.is_empty();
    row_of(
        out,
        scope.region(complete, true),
        has,
        cl,
        "f's sign is not decided everywhere",
    )
}

// ---------------------------------------------------------- y-intercept

pub fn y_intercept(f: &Fun<'_>, dom: &Domain) -> Result<Row<Option<Enc>>, Stop> {
    // 0 excluded by the certified domain (a member of an excluded family,
    // or outside every piece): no y-intercept.
    if let Row::Certified { cert, .. } = &dom.row {
        let zero = Enc::point(0.0);
        let in_family = dom.families.iter().any(|fam| {
            fam.members(0.0, 0.0)
                .iter()
                .any(|m| m.is_point() && m.lo() == 0.0)
        });
        let outside = !dom.pieces.iter().any(|p| {
            let lo_ok = match p.lo {
                Bound::NegInf => true,
                Bound::PosInf => false,
                Bound::At { x, closed } => x.hi.0 < 0.0 || (closed && x == zero),
            };
            let hi_ok = match p.hi {
                Bound::PosInf => true,
                Bound::NegInf => false,
                Bound::At { x, closed } => x.lo.0 > 0.0 || (closed && x == zero),
            };
            lo_ok && hi_ok
        }) && dom.pieces.iter().all(|p| {
            // Every piece provably misses 0.
            let below =
                matches!(p.hi, Bound::At { x, closed } if x.hi.0 < 0.0 || (!closed && x == zero));
            let above =
                matches!(p.lo, Bound::At { x, closed } if x.lo.0 > 0.0 || (!closed && x == zero));
            below || above
        });
        if in_family || outside {
            let mut c = Certificate::new(Region::Points);
            c.extend(cert.claims.iter().cloned());
            return Ok(Row::Certified {
                value: None,
                cert: c,
            });
        }
    }
    // The original tree: 0 may be a hole the simplified form fills.
    let v = f.ser_of(&f.expr, Interval::point(0.0), 0)?[0];
    let mut c = Certificate::new(Region::Points);
    if v.is_empty() {
        c.push(Claim::Undefined {
            x: XBox::point(0.0),
        });
        return Ok(Row::Certified {
            value: None,
            cert: c,
        });
    }
    if v.dec < Dec::Def || !v.iv.is_bounded() {
        return Ok(Row::unknown("f(0) is not decided"));
    }
    c.push(Claim::Value {
        x: XBox::point(0.0),
        of: Subject::f(0),
        lo: R(v.lo()),
        hi: R(v.hi()),
    });
    Ok(Row::Certified {
        value: Some(Enc::new(v.lo(), v.hi())),
        cert: c,
    })
}

// ---------------------------------------------------------- sign walks

/// f⁽ᵏ⁾ along one box: signs on stretches, zeros at points.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Seg {
    /// Strictly positive (true) or negative.
    Sign(bool),
    /// A zero at a point (enclosed).
    Zero(Enc),
    /// ≡ 0 on a stretch.
    Flat,
    /// A kink of f (enclosed): f′ and f″ undecided there, f continuous.
    Kink(Enc),
    /// Not decided.
    Unknown,
}

fn walk(cover: &Cover, ib: &IBox) -> Vec<Seg> {
    // Each leaf's segments, and each kink's, in x order.
    let mut parts: Vec<(f64, Vec<Seg>)> = Vec::new();
    for &(l, r) in cover.kinks.iter().filter(|(l, r)| *l >= ib.a && *r <= ib.b) {
        parts.push((l, vec![Seg::Kink(Enc::new(l, r))]));
    }
    for l in cover.leaves.iter().filter(|l| {
        let (a, b) = l.span();
        a >= ib.a && b <= ib.b
    }) {
        let mut out = Vec::new();
        match *l {
            Leaf::Band { band, .. } => out.push(Seg::Sign(band == 1)),
            Leaf::Cross {
                a,
                b,
                l,
                r,
                exact,
                rising,
                ..
            } => {
                let x = match exact {
                    Some(p) => Enc::point(p),
                    None => enc_of(l, r),
                };
                let (before, after) = match exact {
                    Some(p) => (a < p, p < b),
                    None => (true, true),
                };
                if before {
                    out.push(Seg::Sign(!rising));
                }
                out.push(Seg::Zero(x));
                if after {
                    out.push(Seg::Sign(rising));
                }
            }
            Leaf::At { p, .. } => out.push(Seg::Zero(Enc::point(p))),
            Leaf::Touch { a, p, above, .. } => {
                if p == a {
                    out.extend([Seg::Zero(Enc::point(p)), Seg::Sign(above)]);
                } else {
                    out.extend([Seg::Sign(above), Seg::Zero(Enc::point(p))]);
                }
            }
            Leaf::Equal { .. } => out.push(Seg::Flat),
            Leaf::Undefined { .. } | Leaf::Flag { .. } => out.push(Seg::Unknown),
        }
        parts.push((l.span().0, out));
    }
    parts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<Seg> = parts.into_iter().flat_map(|(_, s)| s).collect();
    // A kink placed exactly (`Claim::KinkAt`) whose one-sided signs carry
    // on the signs beside its box: the turn there is at that point.
    for i in 1..out.len().saturating_sub(1) {
        let Seg::Kink(x) = out[i] else { continue };
        let placed = cover
            .kinks
            .iter()
            .zip(&cover.kink_at)
            .find(|((l, r), _)| *l == x.lo.0 && *r == x.hi.0)
            .and_then(|(_, k)| *k);
        if let Some(k) = placed
            && out[i - 1] == Seg::Sign(k.left)
            && out[i + 1] == Seg::Sign(k.right)
        {
            out[i] = Seg::Kink(Enc::point(k.p));
        }
    }
    // Merge runs of the same sign, and a zero seen from both sides (its
    // point leaf and the boxes ending there).
    out.dedup_by(|y, x| match (*x, *y) {
        (Seg::Sign(p), Seg::Sign(q)) => p == q,
        (Seg::Zero(p), Seg::Zero(q)) => p == q,
        (Seg::Flat, Seg::Flat) => true,
        _ => false,
    });
    out
}

/// Where f⁽ᵏ⁾ changes sign along a box: (point, from positive to
/// negative?), each proven by decided signs on both sides (the leaves tile
/// the box, so neighbours in the walk are neighbours in x); and whether
/// every zero and stretch was decided (no change missed).
/// f⁽ᵏ⁾ ≡ 0 on the whole box (its stretches and the points between them).
fn flat(segs: &[Seg]) -> bool {
    segs.contains(&Seg::Flat)
        && segs
            .iter()
            .all(|s| matches!(s, Seg::Flat | Seg::Zero(_) | Seg::Kink(_)))
}

/// With `kink_turns` (f′), a kink between strict opposite signs is a
/// change (f, continuous there, turns); without it (f″), such a kink
/// leaves the walk incomplete (an inflection at a corner is not decided).
/// The same strict sign on both sides, or f⁽ᵏ⁾ ≡ 0 on both, is no change.
fn changes(segs: &[Seg], kink_turns: bool) -> (Vec<(Enc, bool)>, bool) {
    let mut out = Vec::new();
    let mut complete = true;
    for (i, s) in segs.iter().enumerate() {
        match s {
            Seg::Kink(x) => {
                let before = i.checked_sub(1).and_then(|j| segs.get(j));
                let after = segs.get(i + 1);
                match (before, after) {
                    (Some(Seg::Sign(p)), Some(Seg::Sign(q))) => {
                        if p != q {
                            if kink_turns {
                                out.push((*x, *p));
                            } else {
                                complete = false;
                            }
                        }
                    }
                    (Some(Seg::Flat), Some(Seg::Flat)) => {}
                    _ => complete = false,
                }
            }
            Seg::Zero(x) => {
                let before = i.checked_sub(1).and_then(|j| segs.get(j));
                let after = segs.get(i + 1);
                match (before, after) {
                    (Some(Seg::Sign(p)), Some(Seg::Sign(q))) => {
                        if p != q {
                            out.push((*x, *p));
                        }
                    }
                    // A zero at the very end of a box: the box's bound,
                    // not a change inside.
                    (None, Some(Seg::Sign(_))) | (Some(Seg::Sign(_)), None) => {}
                    _ => complete = false,
                }
            }
            Seg::Flat | Seg::Unknown => complete = false,
            Seg::Sign(_) => {}
        }
    }
    // ≡ 0 on the whole box: no strict change of sign anywhere in it.
    if flat(segs) {
        complete = true;
    }
    (out, complete)
}

// ---------------------------------------------------------- extrema

/// A local extremum at a closed end of the domain (a domain bound, not a
/// window edge), like √x's minimum at 0 or asin's at ±1: f is continuous
/// from the end to the first box f′ decides, and f′ keeps one strict sign
/// beside the end (a zero right at the end aside), so f is strictly
/// monotone beside it; rising away from a low end makes it a minimum.
/// `Ok(None)`: not decided; `Ok(Some(None))`: the end is no closed domain
/// end.
fn end_extremum(
    f: &Fun<'_>,
    c1: &Cover,
    ib: &IBox,
    low: bool,
    segs: &[Seg],
    cl: &mut Vec<Claim>,
) -> Result<Option<Option<Extremum>>, Stop> {
    let (bound, clipped) = if low {
        (ib.lo, ib.lo_clipped)
    } else {
        (ib.hi, ib.hi_clipped)
    };
    let Bound::At { x, closed: true } = bound else {
        return Ok(Some(None));
    };
    if clipped {
        return Ok(Some(None));
    }
    // f′'s sign beside the end, a zero at the end itself aside.
    let ordered: Vec<&Seg> = if low {
        segs.iter().collect()
    } else {
        segs.iter().rev().collect()
    };
    let mut sign = None;
    for s in ordered {
        match s {
            Seg::Zero(z) if (low && z.lo.0 <= x.hi.0) || (!low && z.hi.0 >= x.lo.0) => {}
            Seg::Sign(p) => {
                sign = Some(*p);
                break;
            }
            _ => return Ok(None),
        }
    }
    let Some(sign) = sign else {
        return Ok(None);
    };
    // The first leaf beside the end (an exact zero at the end aside).
    let mut leaves: Vec<(f64, f64)> = c1
        .leaves
        .iter()
        .filter(|l| !matches!(l, Leaf::At { .. }))
        .map(|l| l.span())
        .filter(|&(a, b)| a >= ib.a && b <= ib.b)
        .collect();
    leaves.sort_by(|p, q| p.0.total_cmp(&q.0));
    let near = if low { leaves.first() } else { leaves.last() };
    let Some(&(a, b)) = near else {
        return Ok(None);
    };
    let span = if low {
        Interval::new(x.lo.0, b)
    } else {
        Interval::new(a, x.hi.0)
    };
    let over = f.val(span)?;
    if over.is_empty() || over.dec < Dec::Dac {
        return Ok(None);
    }
    let y = f.val_tight(Interval::new(x.lo.0, x.hi.0))?;
    if y.is_empty() || !y.iv.is_bounded() || y.dec < Dec::Def {
        return Ok(None);
    }
    cl.push(Claim::Continuous {
        x: XBox::new(span.lo(), span.hi()),
    });
    cl.push(Claim::Value {
        x: XBox { a: x.lo, b: x.hi },
        of: Subject::f(0),
        lo: R(y.lo()),
        hi: R(y.hi()),
    });
    // Rising away from a low end, or up to a high end.
    let min = sign == low;
    Ok(Some(Some(Extremum {
        x,
        y: Enc::new(y.lo(), y.hi()),
        kind: if min { ExtKind::Min } else { ExtKind::Max },
        every: None,
    })))
}

pub fn extrema(
    f: &Fun<'_>,
    c1: &Cover,
    boxes: &[IBox],
    scope: &Scope,
    clear: bool,
) -> Result<Row<Vec<Extremum>>, Stop> {
    let (gaps, mut complete) = settle(c1, clear);
    let mut out = Vec::new();
    let mut cl = claims(c1, &Subject::f(1), &[0.0]);
    cl.extend(gaps);
    for ib in boxes {
        let segs = walk(c1, ib);
        let (ch, done) = changes(&segs, true);
        complete &= done;
        for low in [true, false] {
            if !low && ib.a == ib.b {
                // An isolated point: one end.
                continue;
            }
            match end_extremum(f, c1, ib, low, &segs, &mut cl)? {
                Some(Some(e)) => out.push(e),
                Some(None) => {}
                None => complete = false,
            }
        }
        for (x, from_pos) in ch {
            if let Some(p) = scope.period
                && repeats(
                    &out.iter().map(|e: &Extremum| e.x).collect::<Vec<_>>(),
                    &x,
                    &p,
                )
            {
                continue;
            }
            let y = f.val_tight(Interval::new(x.lo.0, x.hi.0))?;
            if y.is_empty() || !y.iv.is_bounded() || y.dec < Dec::Def {
                complete = false;
                continue;
            }
            cl.push(Claim::Value {
                x: XBox { a: x.lo, b: x.hi },
                of: Subject::f(0),
                lo: R(y.lo()),
                hi: R(y.hi()),
            });
            out.push(Extremum {
                x,
                y: Enc::new(y.lo(), y.hi()),
                kind: if from_pos { ExtKind::Max } else { ExtKind::Min },
                every: scope.period,
            });
        }
    }
    let has = !out.is_empty();
    Ok(row_of(
        out,
        scope.region(complete, true),
        has,
        cl,
        "f′ is not decided everywhere",
    ))
}

pub fn inflections(
    f: &Fun<'_>,
    c2: &Cover,
    boxes: &[IBox],
    scope: &Scope,
    clear: bool,
) -> Result<Row<Vec<Inflection>>, Stop> {
    let (gaps, mut complete) = settle(c2, clear);
    let mut out = Vec::new();
    let mut cl = claims(c2, &Subject::f(2), &[0.0]);
    cl.extend(gaps);
    for ib in boxes {
        let segs = walk(c2, ib);
        let (ch, done) = changes(&segs, false);
        complete &= done;
        for (x, _) in ch {
            if let Some(p) = scope.period
                && repeats(
                    &out.iter().map(|e: &Inflection| e.x).collect::<Vec<_>>(),
                    &x,
                    &p,
                )
            {
                continue;
            }
            let y = f.val_tight(Interval::new(x.lo.0, x.hi.0))?;
            if y.is_empty() || !y.iv.is_bounded() || y.dec < Dec::Def {
                complete = false;
                continue;
            }
            cl.push(Claim::Value {
                x: XBox { a: x.lo, b: x.hi },
                of: Subject::f(0),
                lo: R(y.lo()),
                hi: R(y.hi()),
            });
            out.push(Inflection {
                x,
                y: Enc::new(y.lo(), y.hi()),
                every: scope.period,
            });
        }
    }
    let has = !out.is_empty();
    Ok(row_of(
        out,
        scope.region(complete, true),
        has,
        cl,
        "f″ is not decided everywhere",
    ))
}

// ---------------------------------------------------------- monotonicity

/// The monotone pieces of each box, from f′'s sign walk.
fn monotone_pieces(c1: &Cover, boxes: &[IBox]) -> Option<Vec<(Monotone, bool, bool)>> {
    let mut out = Vec::new();
    for ib in boxes {
        let segs = walk(c1, ib);
        if flat(&segs) {
            // f′ ≡ 0 on the whole box: f is constant there.
            out.push((
                Monotone {
                    on: Piece {
                        lo: ib.lo,
                        hi: ib.hi,
                    },
                    dir: Dir::Constant,
                },
                ib.lo_clipped,
                ib.hi_clipped,
            ));
            continue;
        }
        if !changes(&segs, true).1 {
            return None;
        }
        let mut lo = ib.lo;
        let mut lo_clip = ib.lo_clipped;
        let mut dir: Option<bool> = None;
        for (i, s) in segs.iter().enumerate() {
            match *s {
                Seg::Sign(p) => {
                    if dir.is_none() {
                        dir = Some(p);
                    }
                }
                Seg::Zero(x) | Seg::Kink(x) => {
                    let after = segs.get(i + 1);
                    if let (Some(d), Some(Seg::Sign(q))) = (dir, after)
                        && *q != d
                    {
                        out.push((
                            Monotone {
                                on: Piece {
                                    lo,
                                    hi: Bound::At { x, closed: true },
                                },
                                dir: if d { Dir::Increasing } else { Dir::Decreasing },
                            },
                            lo_clip,
                            false,
                        ));
                        lo = Bound::At { x, closed: true };
                        lo_clip = false;
                        dir = Some(*q);
                    }
                }
                _ => return None,
            }
        }
        let d = dir?;
        out.push((
            Monotone {
                on: Piece { lo, hi: ib.hi },
                dir: if d { Dir::Increasing } else { Dir::Decreasing },
            },
            lo_clip,
            ib.hi_clipped,
        ));
    }
    Some(out)
}

pub fn monotonicity(c1: &Cover, boxes: &[IBox], scope: &Scope, clear: bool) -> Row<Vec<Monotone>> {
    let (gaps, complete) = settle(c1, clear);
    let Some(mut pieces) = monotone_pieces(c1, boxes) else {
        return Row::unknown("f′'s sign is not decided everywhere");
    };
    let mut cl = claims(c1, &Subject::f(1), &[0.0]);
    cl.extend(gaps);
    // Over one period [s, s + P] of a periodic f, the piece cut at the
    // window's end continues into the one cut at its start, shifted by P:
    // f′ has the same strict sign on both sides of s + P (s is chosen
    // where f, f′ and f″ are strictly signed), so f is monotone across it.
    // Joined, the pieces describe one period, repeated: the whole line.
    let mut repeats = false;
    if let Some(p) = scope.period
        && complete
        && pieces.len() >= 2
        && pieces[0].1
        && pieces[pieces.len() - 1].2
        && pieces[0].0.dir == pieces[pieces.len() - 1].0.dir
        && let Bound::At { x: first_hi, .. } = pieces[0].0.on.hi
    {
        let (first, _, _) = pieces.remove(0);
        let last = pieces.last_mut().expect("two or more pieces");
        let shifted = Interval::new(first_hi.lo.0, first_hi.hi.0) + Interval::new(p.lo.0, p.hi.0);
        last.0.on.hi = Bound::At {
            x: Enc::new(shifted.lo(), shifted.hi()),
            closed: matches!(first.on.hi, Bound::At { closed: true, .. }),
        };
        last.2 = false;
        repeats = true;
    }
    let out: Vec<Monotone> = pieces.into_iter().map(|(m, _, _)| m).collect();
    let has = !out.is_empty();
    row_of(
        out,
        scope.region(complete, repeats),
        has,
        cl,
        "f′'s sign is not decided everywhere",
    )
}

// ---------------------------------------------------------- poles

/// A neighbourhood of an excluded point (enclosed by `x`) inside the two
/// boxes cut at it, so it holds no other excluded point.
fn around(x: Enc, boxes: &[IBox]) -> Option<Interval> {
    let cut = |b: Bound| matches!(b, Bound::At { x: y, closed: false } if y == x);
    let left = boxes.iter().find(|b| cut(b.hi) && !b.hi_clipped)?;
    let right = boxes.iter().find(|b| cut(b.lo) && !b.lo_clipped)?;
    let p = x.mid();
    let d = 1e-6 * p.abs().max(1.0);
    let lo = (p - d).max(left.a);
    let hi = (p + d).min(right.b);
    (lo < x.lo.0 && hi > x.hi.0).then(|| Interval::new(lo, hi))
}

/// A one-sided neighbourhood of a domain end p, inside the box beside it.
fn beside(p: f64, ib: &IBox, from_right: bool) -> Interval {
    let d = 1e-6 * p.abs().max(1.0);
    if from_right {
        Interval::new(p, (p + d).min(ib.b))
    } else {
        Interval::new((p - d).max(ib.a), p)
    }
}

/// f's sign right beside a box end, from the cover of f (the first
/// decided leaf inward from that end).
fn sign_beside(c0: &Cover, ib: &IBox, at_start: bool) -> Option<bool> {
    let inside = |l: &&Leaf| {
        let (a, b) = l.span();
        a >= ib.a && b <= ib.b
    };
    let leaf = if at_start {
        c0.leaves
            .iter()
            .filter(inside)
            .min_by(|x, y| x.span().0.total_cmp(&y.span().0))
    } else {
        c0.leaves
            .iter()
            .filter(inside)
            .max_by(|x, y| x.span().1.total_cmp(&y.span().1))
    }?;
    match *leaf {
        Leaf::Band { band, .. } => Some(band == 1),
        Leaf::Touch { above, .. } => Some(above),
        // Before the crossing h is on the side it rises from; after it, on
        // the side it rises to (the crossing is inside, or at the far end).
        Leaf::Cross {
            a, b, l, r, rising, ..
        } => {
            if at_start && a < l {
                Some(!rising)
            } else if !at_start && r < b {
                Some(rising)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// How f behaves at an excluded point between two boxes, with the claim
/// that says so.
fn classify(
    f: &Fun<'_>,
    x: Enc,
    boxes: &[IBox],
) -> Result<(super::pole::Near, Option<Claim>), Stop> {
    use super::pole::Near;
    let Some(n) = around(x, boxes) else {
        return Ok((Near::Unknown, None));
    };
    let near = XBox::new(n.lo(), n.hi());
    if pole(f, &f.eval, n)? {
        return Ok((
            Near::Pole,
            Some(Claim::Unbounded {
                near,
                at: XBox { a: x.lo, b: x.hi },
            }),
        ));
    }
    if bounded_near(f, n)? {
        let v = f.val(n)?;
        return Ok((
            Near::Bounded,
            Some(Claim::Bounded {
                near,
                at: XBox { a: x.lo, b: x.hi },
                lo: R(v.lo()),
                hi: R(v.hi()),
            }),
        ));
    }
    Ok((Near::Unknown, None))
}

/// Whether f⁽ᵏ⁾ has no zero in any gap (the reals between an excluded
/// point or domain end and the first double a box starts from, the
/// excluded point included: f's enclosure there covers where f is
/// defined). A zero there would be a feature less than an ulp from the
/// excluded point, which no box decides (`ln(x) + 10⁶` has its root at
/// e^(−10⁶), below every double): a row is complete only if the gaps are
/// clear. For f itself a pole with no zero beside it (a quotient whose
/// numerator stays away from 0) also clears its gaps.
pub fn gaps_clear(
    f: &Fun<'_>,
    k: usize,
    gaps: &[XBox],
    boxes: &[IBox],
    use_derivs: bool,
) -> Result<bool, Stop> {
    let (inner, _) = excluded_points(boxes);
    for g in gaps {
        let iv = Interval::new(g.a.0, g.b.0);
        // The derivative trees, for a kinked f off its kinks: usable here
        // when no kink in the gap is a point of f's.
        let derivs = if !use_derivs {
            None
        } else if f.kinked {
            if f.no_kink_in(iv)? {
                f.derivs_off_kinks()
            } else {
                None
            }
        } else {
            f.derivs()
        };
        let c = f.ser(iv, k)?[k];
        if !c.is_empty() && c.ne0() {
            continue;
        }
        // f⁽ᵏ⁾ ≡ 0 (a rational f of lower degree): no change of sign.
        if k > 0
            && f.numerators
                .as_ref()
                .is_some_and(|n| matches!(n[k], crate::ast::Expr::Num(v) if v == 0.0))
        {
            continue;
        }
        if k > 0
            && let Some(d) = derivs
        {
            let v = f.ser_of(&d[k - 1], iv, 0)?[0];
            if !v.is_empty() && v.ne0() {
                continue;
            }
        }
        // Factor by factor: each away from 0 over the gap, or exactly 0 at
        // one of its doubles and strictly monotone across it (so 0 nowhere
        // between the doubles).
        let tree = match (k, &f.numerators, derivs) {
            (_, Some(n), _) => Some(&n[k]),
            (0, None, _) => Some(&f.eval),
            (_, None, Some(d)) => Some(&d[k - 1]),
            _ => None,
        };
        if let Some(t) = tree
            && (matches!(t, crate::ast::Expr::Num(v) if *v == 0.0) || factors_clear(f, t, iv)?)
        {
            continue;
        }
        if k == 0 {
            let near = inner
                .iter()
                .filter(|x| g.a.0 <= x.lo.0 && x.hi.0 <= g.b.0)
                .find_map(|x| around(*x, boxes));
            if let Some(n) = near
                && n.lo() <= iv.lo()
                && iv.hi() <= n.hi()
                && pole_free(f, &f.eval, n)?
            {
                continue;
            }
        }
        return Ok(false);
    }
    Ok(true)
}

/// `e` has no zero strictly between the doubles of the box `g`: each of
/// its zero factors ([`super::fun::zero_factors`]) is away from 0 over the
/// box, or is exactly 0 at one of its doubles and strictly monotone there.
fn factors_clear(f: &Fun<'_>, e: &crate::ast::Expr, g: Interval) -> Result<bool, Stop> {
    for h in f.factors(e) {
        let s = f.ser_of(&h, g, 1)?;
        if !s[0].is_empty() && s[0].ne0() {
            continue;
        }
        let mono = usable(&s, 1) && s[1].ne0();
        let mut exact = false;
        let mut q = g.lo();
        for _ in 0..8 {
            if !mono || q > g.hi() || exact {
                break;
            }
            let v = f.ser_of(&h, Interval::point(q), 0)?[0];
            exact = v.lo() == 0.0 && v.hi() == 0.0;
            q = q.next_up();
        }
        if !exact {
            return Ok(false);
        }
    }
    Ok(true)
}

/// f's sign right beside an excluded point or domain end (enclosed by
/// `x`), on the side of the box `ib` (`is_lo`: ib starts there): from f
/// over the gap between them, or from the first leaf when f has a pole
/// there and no zero beside it.
fn gap_sign(
    f: &Fun<'_>,
    x: Enc,
    ib: &IBox,
    is_lo: bool,
    c0: &Cover,
    boxes: &[IBox],
) -> Result<Option<bool>, Stop> {
    let g = if is_lo {
        Interval::new(x.lo.0, ib.a)
    } else {
        Interval::new(ib.b, x.hi.0)
    };
    let v = f.val(g)?;
    if !v.is_empty() && v.ne0() {
        return Ok(Some(v.gt0()));
    }
    if let Some(n) = around(x, boxes)
        && n.lo() <= g.lo()
        && g.hi() <= n.hi()
        && pole_free(f, &f.eval, n)?
    {
        return Ok(sign_beside(c0, ib, is_lo));
    }
    Ok(None)
}

/// The excluded points between boxes, and domain ends (one-sided).
fn excluded_points(boxes: &[IBox]) -> (Vec<Enc>, Vec<(Enc, bool)>) {
    let mut inner = Vec::new();
    let mut ends = Vec::new();
    for (i, ib) in boxes.iter().enumerate() {
        if let Bound::At { x, closed: false } = ib.hi
            && !ib.hi_clipped
        {
            let next_same = boxes
                .get(i + 1)
                .is_some_and(|nb| matches!(nb.lo, Bound::At { x: y, closed: false } if y == x));
            if next_same {
                inner.push(x);
            } else {
                ends.push((x, false));
            }
        }
        if let Bound::At { x, closed: false } = ib.lo
            && !ib.lo_clipped
        {
            let prev_same =
                i > 0 && matches!(boxes[i - 1].hi, Bound::At { x: y, closed: false } if y == x);
            if !prev_same {
                ends.push((x, true));
            }
        }
    }
    (inner, ends)
}

pub fn vertical(
    f: &Fun<'_>,
    dom: &Domain,
    c0: &Cover,
    boxes: &[IBox],
    scope: &Scope,
) -> Result<Row<Vec<Spot>>, Stop> {
    use super::pole::Near;
    if !dom.row.is_certified() {
        return Ok(Row::unknown("the domain is not known"));
    }
    if !scope.whole && scope.period.is_none() {
        return Ok(Row::unknown("excluded families"));
    }
    // Continuity (or boundedness) everywhere inside: no asymptote but at
    // the exclusions. A box the cover left undecided (its sign) is bounded
    // on its own.
    let mut cl = Vec::new();
    for l in &c0.leaves {
        let ok = match *l {
            Leaf::Band { cont, .. } | Leaf::Equal { cont, .. } => cont,
            Leaf::Flag { a, b, .. } => {
                let v = f.val(Interval::new(a, b))?;
                // Continuous there, or bounded: no asymptote inside.
                let ok = !v.is_empty()
                    && (v.dec >= Dec::Dac || (v.dec >= Dec::Def && v.iv.is_bounded()));
                if ok {
                    cl.push(Claim::Value {
                        x: XBox::new(a, b),
                        of: Subject::f(0),
                        lo: R(v.lo()),
                        hi: R(v.hi()),
                    });
                }
                ok
            }
            _ => true,
        };
        if !ok {
            return Ok(Row::unknown("f is not proven continuous inside its domain"));
        }
    }
    let (inner, ends) = excluded_points(boxes);
    let mut out = Vec::new();
    let mut c = Certificate::new(Region::Line);
    c.extend(claims(c0, &Subject::f(0), &[0.0]));
    c.extend(cl);
    for x in inner {
        let (near, claim) = classify(f, x, boxes)?;
        match near {
            Near::Pole => out.push(Spot::At(x)),
            Near::Bounded => {}
            Near::Unknown => {
                // The simplifier's one-sided limits: an infinite one is an
                // asymptote, two finite ones a hole or a jump.
                let (Some((l, lc)), Some((r, rc))) = (
                    if x.is_point() {
                        side_limit(f, x.lo.0, false)?
                    } else {
                        None
                    },
                    side_limit(f, x.lo.0, true)?,
                ) else {
                    return Ok(Row::unknown("an excluded point is not classified"));
                };
                match (l, r) {
                    (TailEnd::Infinite(_), _) | (_, TailEnd::Infinite(_)) => out.push(Spot::At(x)),
                    (TailEnd::Level(..), TailEnd::Level(..)) => {}
                    _ => return Ok(Row::unknown("an excluded point is not classified")),
                }
                c.extend(lc);
                c.extend(rc);
            }
        }
        c.extend(claim);
    }
    for (x, from_right) in ends {
        // The box on the side of the end the domain is on.
        let cut = |b: Bound| matches!(b, Bound::At { x: y, closed: false } if y == x);
        let Some(ib) = boxes
            .iter()
            .find(|b| if from_right { cut(b.lo) } else { cut(b.hi) })
        else {
            continue;
        };
        let p = if from_right { x.hi.0 } else { x.lo.0 };
        let n = beside(p, ib, from_right);
        // An end known only to an enclosure: bounded up to the end means
        // bounded over the enclosure too (where f is defined there).
        let n = if x.is_point() {
            n
        } else {
            n.hull(Interval::new(x.lo.0, x.hi.0))
        };
        let near = XBox::new(n.lo(), n.hi());
        if x.is_point() && end_pole(f, &f.eval, n, p)? {
            out.push(Spot::At(x));
            c.push(Claim::Unbounded {
                near,
                at: XBox { a: x.lo, b: x.hi },
            });
        } else if bounded_near(f, n)? {
            let v = f.val(n)?;
            c.push(Claim::Bounded {
                near,
                at: XBox { a: x.lo, b: x.hi },
                lo: R(v.lo()),
                hi: R(v.hi()),
            });
        } else {
            let side = if x.is_point() {
                side_limit(f, p, from_right)?
            } else {
                None
            };
            match side {
                Some((TailEnd::Infinite(_), claims)) => {
                    out.push(Spot::At(x));
                    c.extend(claims);
                }
                Some((TailEnd::Level(..), claims)) => c.extend(claims),
                _ => return Ok(Row::unknown("a domain end is not classified")),
            }
        }
    }
    if let Some(period) = scope.period {
        // Over one period: each asymptote repeats.
        let mut seen: Vec<Enc> = Vec::new();
        let mut fams = Vec::new();
        for s in out {
            if let Spot::At(x) = s
                && !repeats(&seen, &x, &period)
            {
                seen.push(x);
                fams.push(Spot::Every(Family { x0: x, period }));
            }
        }
        out = fams;
        c.covers = scope.region(true, true).unwrap_or(Region::Line);
    }
    Ok(Row::Certified {
        value: out,
        cert: c,
    })
}

// ---------------------------------------------------------- tails

/// An enclosure of f far out narrow enough to report a limit by: far
/// narrower than anything the panel shows (10⁻¹² of the value).
fn narrow(v: &DecInterval) -> bool {
    !v.is_empty() && v.iv.is_bounded() && v.hi() - v.lo() <= 1e-12 * v.iv.mid().abs().max(1.0)
}

/// A closed form the panel would recognise in a narrow enclosure of a
/// limit: shown as what the limit looks like, never as proven.
fn looks_like(y: Enc) -> Option<R> {
    let m = y.mid();
    if y.contains(0.0) {
        return Some(R(0.0));
    }
    let v = Nice::of(m).value();
    let slack = 2.0 * super::fun::ulp(v);
    (y.lo.0 - slack <= v && v <= y.hi.0 + slack).then_some(R(v))
}

/// What f does on a tail: tends to ±∞, tends to a limit (enclosed, and
/// exactly when the simplifier proved it), or not decided.
#[derive(Clone, Debug)]
pub enum TailEnd {
    Infinite(bool),
    Level(Enc, Option<String>),
    Unknown,
}

/// Where f's own enclosures on a tail [M, ∞) (or (−∞, −M]) are taken to
/// bear out a limit: M from 10⁸ out to 10³⁰⁰.
const FAR: [f64; 6] = [1e8, 1e16, 1e32, 1e64, 1e150, 1e300];

/// The tails' starts out from `start`: a few near it (before f overflows,
/// as eˣ does by 10³), then the far sequence.
fn out_along(start: f64) -> Vec<f64> {
    // Doublings from 1 and from the start (e^(x − 1000) is a double only
    // between 2⁸ and 2¹¹), then the far sequence.
    let mut ms: Vec<f64> = (0..=24)
        .map(|k| f64::from(1u32 << k))
        .chain((0..=12).map(|k| start * f64::from(1u32 << k)))
        .chain(FAR)
        .filter(|m| *m >= 1.0 && m.is_finite())
        .collect();
    ms.sort_by(f64::total_cmp);
    ms.dedup();
    ms
}

/// The tails' starts for f on one side: [`out_along`], and beside each
/// far centre of an exponential in f on that side (c + 2ᵏ: e^(x − 10⁹) is
/// a double only within 745 of 10⁹).
fn far_points(f: &Fun<'_>, right: bool, start: f64) -> Vec<f64> {
    let mut ms = out_along(start);
    if let Some(lits) = f.exact {
        for c in crate::simplify::limit::centres(&f.expr, lits) {
            if (c > 0.0) == right {
                ms.extend((0..=12).map(|k| c.abs() + f64::from(1u32 << k)));
            }
        }
        ms.sort_by(f64::total_cmp);
        ms.dedup();
    }
    ms
}

/// At most `k` of `v`, evenly spread, the first and the last kept (the
/// claims a run of bounds or bands needs, not every one computed).
fn thin<T: Clone>(v: &[T], k: usize) -> Vec<T> {
    let n = v.len();
    if n <= k || k < 2 {
        return v.to_vec();
    }
    let mut idx: Vec<usize> = (0..k).map(|i| i * (n - 1) / (k - 1)).collect();
    idx.dedup();
    idx.into_iter().map(|i| v[i].clone()).collect()
}

/// How many claims a run of tail bounds or bands keeps.
const KEEP: usize = 10;

/// The records of `bounds` (in order out along the tail, or in towards a
/// point): the indices of those beyond every one before them (in the
/// direction `up`), as the replay reads growth. (An overflowed bound
/// repeats; a weaker one adds nothing.)
fn records(bounds: &[f64], up: bool) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for (i, &b) in bounds.iter().enumerate() {
        let beyond = out
            .last()
            .is_none_or(|&j| if up { b > bounds[j] } else { b < bounds[j] });
        if beyond {
            out.push(i);
        }
    }
    out
}

/// Boxes beside p shrinking towards it: ½ … 10⁻³⁰⁰ of max(1, |p|), and
/// as much in plain units, while more than a few doubles wide.
fn toward(p: f64) -> Vec<f64> {
    let s = p.abs().max(1.0);
    let ulp = p.abs().next_up() - p.abs();
    let ks = [
        0.5, 0.25, 0.1, 0.05, 0.02, 1e-2, 1e-3, 1e-4, 1e-6, 1e-8, 1e-12, 1e-16, 1e-32, 1e-64,
        1e-150, 1e-300,
    ];
    // Relative to p, and (for a p far out, beside another feature) plain.
    let mut ds: Vec<f64> = ks
        .iter()
        .map(|k| k * s)
        .chain(ks)
        .filter(|d| *d > 4.0 * ulp)
        .collect();
    ds.sort_by(|a, b| b.total_cmp(a));
    ds.dedup();
    ds
}

/// Bands beside a point closing in on the limit `y`: the last three each
/// no farther from it than the one before (or all within 10⁻⁹ of it), the
/// last within 10⁻⁹ of it (or holding it).
fn closing_in(bands: &[Interval], y: Enc) -> bool {
    let apart = |b: &Interval| (y.lo.0 - b.hi()).max(b.lo() - y.hi.0).max(0.0);
    let d: Vec<f64> = bands.iter().map(apart).collect();
    let n = d.len();
    let tol = 1e-9 * y.lo.0.abs().max(y.hi.0.abs()).max(1.0);
    n >= 3
        && (d[n - 3..].windows(2).all(|p| p[1] <= p[0]) || d[n - 3..].iter().all(|v| *v <= tol))
        && d[n - 1] <= tol
}

/// Bands that settle on a limit: the last narrow (10⁻¹² of it), or three
/// or more, the last three none wider than the one before, the last a
/// hundredth as wide as the widest (x^−0.01 far out: 0.83, …, 0.001).
fn settles(bands: &[Interval]) -> bool {
    let Some(last) = bands.last() else {
        return false;
    };
    if narrow(&DecInterval::new(*last)) {
        return true;
    }
    let w: Vec<f64> = bands.iter().map(|b| b.hi() - b.lo()).collect();
    let n = w.len();
    n >= 3
        && w[n - 3..].windows(2).all(|p| p[1] <= p[0])
        && w[n - 1] <= 1e-2 * w.iter().copied().fold(0.0, f64::max)
}

/// The tail beyond `m` (> 0) on the right, or before −m on the left.
fn tail_box(right: bool, m: f64) -> Interval {
    if right {
        Interval::new(m, f64::INFINITY)
    } else {
        Interval::new(f64::NEG_INFINITY, -m)
    }
}

/// f′'s strict sign on the tail beyond `start` (rising: `true`), with
/// f′'s enclosure there ([`tail_sign_k`]).
fn tail_sign(f: &Fun<'_>, right: bool, start: f64) -> Result<Option<(bool, DecInterval)>, Stop> {
    tail_sign_k(f, 1, right, start)
}

/// f⁽ᵏ⁾'s strict sign on the tail beyond `start` (k = 1 or 2; positive:
/// `true`), with f⁽ᵏ⁾'s enclosure there: from that enclosure, or from
/// f⁽ᵏ⁾'s own tree, continuous on the tail with no factor reaching 0
/// there (−csch² for coth), so of the sign it has at the start.
fn tail_sign_k(
    f: &Fun<'_>,
    k: usize,
    right: bool,
    start: f64,
) -> Result<Option<(bool, DecInterval)>, Stop> {
    let s = f.ser(tail_box(right, start), k)?;
    // (f⁽ᵏ⁾'s enclosure, k!·s[k], or none: 0 · ∞ in x²·e⁻ˣ.)
    let d = if usable(&s, k) {
        DecInterval {
            iv: s[k].iv * Interval::point(if k == 2 { 2.0 } else { 1.0 }),
            ..s[k]
        }
    } else {
        DecInterval::new(Interval::ENTIRE)
    };
    if usable(&s, k) && d.ne0() {
        return Ok(Some((d.gt0(), d)));
    }
    // Without f's own series, its tree must be smooth (no jumps, kinks)
    // and f defined on the tail: its derivative's tree is f⁽ᵏ⁾ there.
    if !usable(&s, k) && !(f.smooth_tree && !s[0].is_empty() && s[0].dec >= Dec::Def) {
        return Ok(None);
    }
    let Some(t) = f.derivs().map(|d| &d[k - 1]) else {
        return Ok(None);
    };
    let whole = f.ser_of(t, tail_box(right, start), 0)?[0];
    let mut nonzero = !whole.is_empty() && whole.dec >= Dec::Dac;
    for h in f.factors(t) {
        let v = f.ser_of(&h, tail_box(right, start), 0)?[0];
        nonzero &= !v.is_empty() && v.ne0();
    }
    let at = f.ser_of(t, Interval::point(if right { start } else { -start }), 0)?[0];
    if !(nonzero && !at.is_empty() && at.ne0()) {
        return Ok(None);
    }
    Ok(Some((at.gt0(), d)))
}

/// f's bands on the tails [M, ∞) of the far sequence (M ≥ `start`), each
/// with its claims: its enclosure there where bounded (`TailValue`); else,
/// f monotone on the tail (`rising`), its value at M one way — f moves
/// away from f(M) out along the tail — and its enclosure's bound the other
/// (`Value` at M, `TailBeyond` on [M, ∞)): x·e⁻ˣ in (0, M·e⁻ᴹ].
fn far_bands(
    f: &Fun<'_>,
    right: bool,
    start: f64,
    rising: Option<bool>,
) -> Result<Vec<(Interval, Vec<Claim>)>, Stop> {
    let side = if right { Tail::Right } else { Tail::Left };
    let mut out = Vec::new();
    for m in far_points(f, right, start) {
        let x = if right { m } else { -m };
        let v = f.val(tail_box(right, m))?;
        if v.is_empty() || v.dec < Dec::Def {
            continue;
        }
        if v.iv.is_bounded() {
            out.push((
                v.iv,
                vec![Claim::TailValue {
                    side,
                    from: R(x),
                    lo: R(v.lo()),
                    hi: R(v.hi()),
                }],
            ));
            continue;
        }
        let Some(rising) = rising else { continue };
        let away_up = rising == right;
        let p = f.val(Interval::point(x))?;
        if p.is_empty() || p.dec < Dec::Def || !p.iv.is_bounded() {
            continue;
        }
        let (band, c) = if away_up {
            let c = v.hi().next_up();
            (Interval::new(p.lo(), c), c)
        } else {
            let c = v.lo().next_down();
            (Interval::new(c, p.hi()), c)
        };
        if !c.is_finite() || band.is_empty() {
            continue;
        }
        out.push((
            band,
            vec![
                Claim::Value {
                    x: XBox::point(x),
                    of: Subject::f(0),
                    lo: R(p.lo()),
                    hi: R(p.hi()),
                },
                Claim::TailBeyond {
                    side,
                    from: R(x),
                    of: Subject::f(0),
                    c: R(c),
                    above: !away_up,
                },
            ],
        ));
    }
    Ok(out)
}

/// Interval evidence for a finite limit `y` of f on a tail (the
/// simplifier's, whose dominant terms show it exists): f's bands on the
/// tails of the far sequence ([`far_bands`]) each hold y, two or more,
/// the last narrow. The limit lies in every band; returns their
/// intersection with y. `None` when a band misses y, or none is narrow:
/// the limit isn't borne out.
fn level_bands(
    f: &Fun<'_>,
    right: bool,
    start: f64,
    y: Enc,
) -> Result<Option<(Enc, Vec<Claim>)>, Stop> {
    let side = if right { Tail::Right } else { Tail::Left };
    let sign = tail_sign(f, right, start)?;
    let bands = thin(&far_bands(f, right, start, sign.map(|s| s.0))?, KEEP);
    let mut claims = Vec::new();
    if let Some((rising, _)) = sign {
        claims.push(Claim::TailBeyond {
            side,
            from: R(if right { start } else { -start }),
            of: Subject::f(1),
            c: R(0.0),
            above: rising,
        });
    }
    let mut lim = Interval::new(y.lo.0, y.hi.0);
    for (b, cl) in &bands {
        if b.hi() < y.lo.0 || y.hi.0 < b.lo() {
            return Ok(None);
        }
        lim = lim.intersect(*b);
        claims.extend(cl.iter().cloned());
    }
    let ivs: Vec<Interval> = bands.iter().map(|b| b.0).collect();
    if ivs.len() >= 2 && settles(&ivs) && !lim.is_empty() {
        Ok(Some((Enc::new(lim.lo(), lim.hi()), claims)))
    } else {
        Ok(None)
    }
}

/// Bounds that grow without settling, as the replay reads them: three or
/// more (each already beyond the last), gaining at least 1 (x^−0.00001 by
/// 0⁺ gains 3·10⁻³ by 10⁻³⁰⁰: no evidence of ∞; ln x + 10⁶ gains 6.9·10²).
fn grows_enough(cs: &[f64]) -> bool {
    cs.len() >= 3 && (cs[cs.len() - 1] - cs[0]).abs() >= 1.0
}

/// Interval evidence for f → +∞ (`up`) or −∞ on a tail (the simplifier's
/// limit): on each tail [M, ∞) of the far sequence f is defined and
/// beyond a bound (a `TailBeyond` claim on f), the bounds strictly growing
/// out along it. `None` when they don't grow.
fn infinite_bounds(
    f: &Fun<'_>,
    right: bool,
    start: f64,
    up: bool,
) -> Result<Option<Vec<Claim>>, Stop> {
    let side = if right { Tail::Right } else { Tail::Left };
    let mut got: Vec<(f64, f64)> = Vec::new();
    for m in far_points(f, right, start) {
        let v = f.val(tail_box(right, m))?;
        if v.is_empty() || v.dec < Dec::Def {
            continue;
        }
        // Strictly beyond: one double short of the enclosure's end.
        let c = if up {
            v.lo().next_down()
        } else {
            v.hi().next_up()
        };
        // (Past where f overflows, no more bounds.)
        if !c.is_finite() || c.abs() >= f64::MAX / 16.0 {
            break;
        }
        got.push((m, c));
    }
    let cs: Vec<f64> = got.iter().map(|g| g.1).collect();
    let rec = records(&cs, up);
    let rcs: Vec<f64> = rec.iter().map(|&i| cs[i]).collect();
    if grows_enough(&rcs) {
        let kept: Vec<(f64, f64)> = rec.iter().map(|&i| got[i]).collect();
        return Ok(Some(
            thin(&kept, KEEP)
                .iter()
                .map(|&(m, c)| Claim::TailBeyond {
                    side,
                    from: R(if right { m } else { -m }),
                    of: Subject::f(0),
                    c: R(c),
                    above: up,
                })
                .collect(),
        ));
    }
    // f monotone on the tail, moving that way: f stays beyond its value at
    // each M, and those values grow (x/ln x, whose enclosure on [M, ∞)
    // starts at M/∞ = 0).
    let Some((rising, _)) = tail_sign(f, right, start)? else {
        return Ok(None);
    };
    if (rising == right) != up {
        return Ok(None);
    }
    let mut pts: Vec<(f64, f64, f64)> = Vec::new();
    for m in far_points(f, right, start) {
        let x = if right { m } else { -m };
        let v = f.val(Interval::point(x))?;
        if v.is_empty() || v.dec < Dec::Def || !v.iv.is_bounded() {
            continue;
        }
        pts.push((x, v.lo(), v.hi()));
    }
    let cs: Vec<f64> = pts.iter().map(|p| if up { p.1 } else { p.2 }).collect();
    let rec = records(&cs, up);
    if !grows_enough(&rec.iter().map(|&i| cs[i]).collect::<Vec<_>>()) {
        return Ok(None);
    }
    let pts: Vec<(f64, f64, f64)> = rec.iter().map(|&i| pts[i]).collect();
    let mut claims = vec![Claim::TailBeyond {
        side,
        from: R(if right { start } else { -start }),
        of: Subject::f(1),
        c: R(0.0),
        above: rising,
    }];
    for &(x, lo, hi) in &thin(&pts, KEEP) {
        claims.push(Claim::Value {
            x: XBox::point(x),
            of: Subject::f(0),
            lo: R(lo),
            hi: R(hi),
        });
    }
    Ok(Some(claims))
}

/// How f ends on the right (`right`) or left tail beyond `from`, with the
/// claims that prove it: from intervals, and from the simplifier's limit
/// (dominant terms, exact rational functions) when intervals fall short
/// or to pin an enclosed limit down exactly. The simplifier's limit never
/// stands alone: f's own enclosures far out must bear it out
/// ([`level_bands`], [`infinite_bounds`]).
pub fn tail_end(f: &Fun<'_>, right: bool, from: f64) -> Result<(TailEnd, Vec<Claim>), Stop> {
    let (end, mut claims) = tail_interval(f, right, from)?;
    if matches!(end, TailEnd::Infinite(_)) {
        return Ok((end, claims));
    }
    // By the tree's structure: a proof however slowly f settles (1/ln x),
    // its exact value the simplifier's where that agrees.
    if matches!(end, TailEnd::Unknown)
        && let Some(s) = by_structure(
            f,
            if right {
                Toward::PosInf
            } else {
                Toward::NegInf
            },
        )
    {
        return Ok(with_exact(s, simplifier_limit(f, right)));
    }
    let Some((lim, fact)) = simplifier_limit(f, right) else {
        return Ok((end, claims));
    };
    let start = from.abs().max(1.0);
    // A rational f's limit is exact (from N/D's degrees and leading
    // coefficients), its fact checked so: it stands as it is.
    let rational = matches!(&fact, Claim::Simplifier { fact } if fact.starts_with(RATIONAL));
    let out = match (end, lim) {
        (TailEnd::Unknown, lim) if rational => lim,
        (TailEnd::Unknown, TailEnd::Level(y, exact)) => match level_bands(f, right, start, y)? {
            Some((l, bands)) => {
                claims = bands;
                TailEnd::Level(l, exact)
            }
            None => return Ok((TailEnd::Unknown, Vec::new())),
        },
        (TailEnd::Unknown, TailEnd::Infinite(up)) => match infinite_bounds(f, right, start, up)? {
            Some(bounds) => {
                claims = bounds;
                TailEnd::Infinite(up)
            }
            None => return Ok((TailEnd::Unknown, Vec::new())),
        },
        // Both enclose the same limit.
        (TailEnd::Level(e, _), TailEnd::Level(y, exact)) => {
            let (lo, hi) = (e.lo.0.max(y.lo.0), e.hi.0.min(y.hi.0));
            if lo > hi {
                return Ok((TailEnd::Unknown, Vec::new()));
            }
            TailEnd::Level(Enc::new(lo, hi), exact)
        }
        // A bounded tail can't go to ±∞: the two disagree, so neither is
        // trusted.
        _ => return Ok((TailEnd::Unknown, Vec::new())),
    };
    claims.push(fact);
    Ok((out, claims))
}

/// f's limit as x → `at` by the tree's structure ([`super::growth`]), with
/// its claim.
fn by_structure(f: &Fun<'_>, at: Toward) -> Option<(TailEnd, Claim)> {
    let to = super::growth::limit(f, at, false)?;
    let end = match to {
        To::PosInf => TailEnd::Infinite(true),
        To::NegInf => TailEnd::Infinite(false),
        To::In { lo, hi } => TailEnd::Level(Enc::new(lo.0, hi.0), None),
    };
    Some((
        end,
        Claim::Limit {
            at,
            over_x: false,
            to,
        },
    ))
}

/// A limit by structure, written as the simplifier's exact value where
/// that lies in its enclosure (the enclosure kept as proven: the exact
/// form is how it reads, not a narrower claim).
fn with_exact(
    (end, claim): (TailEnd, Claim),
    simplifier: Option<(TailEnd, Claim)>,
) -> (TailEnd, Vec<Claim>) {
    if let TailEnd::Level(y, _) = end
        && let Some((TailEnd::Level(z, Some(text)), _)) = simplifier
        && y.lo.0 <= z.lo.0
        && z.hi.0 <= y.hi.0
    {
        return (TailEnd::Level(y, Some(text)), vec![claim]);
    }
    (end, vec![claim])
}

/// How a limit fact of a rational f begins: the limit is N/D's, exactly.
pub const RATIONAL: &str = "f = N/D exactly: ";

/// The simplifier's limit of f at ±∞, as a tail end with its claim (a
/// rational f's marked so).
fn simplifier_limit(f: &Fun<'_>, right: bool) -> Option<(TailEnd, Claim)> {
    let at = if right { "+∞" } else { "−∞" };
    let (end, claim) = limit_of(f, &f.expr, right, at)?;
    let rational = f
        .exact
        .is_some_and(|x| crate::simplify::rational_form(&f.expr, x).is_some());
    match claim {
        Claim::Simplifier { fact } if rational => Some((
            end,
            Claim::Simplifier {
                fact: format!("{RATIONAL}{fact}"),
            },
        )),
        c => Some((end, c)),
    }
}

/// The simplifier's limit of `e` as x → +∞ (`right`) or −∞, as a tail end
/// with its claim (the limit of f as x → `at`).
fn limit_of(f: &Fun<'_>, e: &crate::ast::Expr, right: bool, at: &str) -> Option<(TailEnd, Claim)> {
    use crate::simplify::{Dir, Limit, limit_at};
    if f.cancelled() {
        return None;
    }
    let s = f.settings()?;
    let dir = if right { Dir::PosInf } else { Dir::NegInf };
    let (end, says) = match limit_at(e, dir, &s) {
        Limit::PosInf => (TailEnd::Infinite(true), "+∞".to_string()),
        Limit::NegInf => (TailEnd::Infinite(false), "−∞".to_string()),
        Limit::Exact(v) => {
            let iv = piq_interval(v)?;
            let text = pi_q(v);
            (
                TailEnd::Level(Enc::new(iv.lo(), iv.hi()), Some(text.clone())),
                text,
            )
        }
        Limit::Approx(iv) => {
            if iv.is_empty() || !iv.is_bounded() {
                return None;
            }
            (
                TailEnd::Level(Enc::new(iv.lo(), iv.hi()), None),
                format!("in [{:e}, {:e}]", iv.lo(), iv.hi()),
            )
        }
        Limit::Unknown => return None,
    };
    let fact = format!("f → {says} as x → {at}");
    Some((end, Claim::Simplifier { fact }))
}

/// f's one-sided limit at the double `p`, from the right (`right`) or the
/// left: the simplifier's (the limit at +∞ of f(p ± 1/x)), borne out by
/// f's own enclosures on ever smaller boxes beside p ([`side_bands`]);
/// the limit's claims with the fact.
pub fn side_limit(f: &Fun<'_>, p: f64, right: bool) -> Result<Option<(TailEnd, Vec<Claim>)>, Stop> {
    let at = if right {
        Toward::Right(R(p))
    } else {
        Toward::Left(R(p))
    };
    if let Some(s) = by_structure(f, at) {
        return Ok(Some(with_exact(s, side_limit_fact(f, p, right))));
    }
    let Some((end, fact)) = side_limit_fact(f, p, right) else {
        return Ok(None);
    };
    Ok(side_bands(f, p, right, &end)?.map(|(end, mut claims)| {
        claims.push(fact);
        (end, claims)
    }))
}

/// Interval evidence for a one-sided limit at `p`: on boxes beside p
/// (½ … 10⁻³⁰⁰ of max(1, |p|) wide, p left out), a finite limit lies in
/// f's band there (two `Beyond` claims, above and below, holding it), the
/// bands settling; an infinite one has f beyond bounds that grow as the
/// boxes shrink (`Beyond` claims). `None` when they don't bear it out.
fn side_bands(
    f: &Fun<'_>,
    p: f64,
    right: bool,
    end: &TailEnd,
) -> Result<Option<(TailEnd, Vec<Claim>)>, Stop> {
    let mut claims = Vec::new();
    match *end {
        TailEnd::Level(y, ref exact) => {
            let mut got: Vec<(Interval, Interval)> = Vec::new();
            for d in toward(p) {
                // The box beside p, p left out (f may be undefined there).
                let n = if right {
                    Interval::new(p.next_up(), p + d)
                } else {
                    Interval::new(p - d, p.next_down())
                };
                if n.is_empty() {
                    break;
                }
                let v = f.val(n)?;
                if v.is_empty() || v.dec < Dec::Def || !v.iv.is_bounded() {
                    continue;
                }
                // f's band there, as a bound on each side. (The limit may
                // lie just outside it, in the reals between p and the box:
                // x·ln x < 0 on [2⁻¹⁰⁷⁴, d], its limit 0.)
                got.push((n, Interval::new(v.lo().next_down(), v.hi().next_up())));
            }
            let got = thin(&got, KEEP);
            for (n, b) in &got {
                for (c, above) in [(b.lo(), true), (b.hi(), false)] {
                    claims.push(Claim::Beyond {
                        x: XBox::new(n.lo(), n.hi()),
                        of: Subject::f(0),
                        c: R(c),
                        above,
                    });
                }
            }
            let bands: Vec<Interval> = got.iter().map(|g| g.1).collect();
            Ok(
                (bands.len() >= 3 && settles(&bands) && closing_in(&bands, y)).then(|| {
                    (
                        TailEnd::Level(Enc::new(y.lo.0, y.hi.0), exact.clone()),
                        claims,
                    )
                }),
            )
        }
        TailEnd::Infinite(up) => {
            let mut got: Vec<(Interval, f64)> = Vec::new();
            for d in toward(p) {
                let n = if right {
                    Interval::new(p.next_up(), p + d)
                } else {
                    Interval::new(p - d, p.next_down())
                };
                if n.is_empty() {
                    break;
                }
                let v = f.val(n)?;
                if v.is_empty() || v.dec < Dec::Def {
                    continue;
                }
                let c = if up {
                    v.lo().next_down()
                } else {
                    v.hi().next_up()
                };
                if !c.is_finite() || c.abs() >= f64::MAX / 16.0 {
                    break;
                }
                got.push((n, c));
            }
            let cs: Vec<f64> = got.iter().map(|g| g.1).collect();
            let rec = records(&cs, up);
            if !grows_enough(&rec.iter().map(|&i| cs[i]).collect::<Vec<_>>()) {
                return Ok(None);
            }
            let kept: Vec<(Interval, f64)> = rec.iter().map(|&i| got[i]).collect();
            for &(n, c) in &thin(&kept, KEEP) {
                claims.push(Claim::Beyond {
                    x: XBox::new(n.lo(), n.hi()),
                    of: Subject::f(0),
                    c: R(c),
                    above: up,
                });
            }
            Ok(Some((TailEnd::Infinite(up), claims)))
        }
        TailEnd::Unknown => Ok(None),
    }
}

/// The simplifier's one-sided limit at `p`, with its fact.
fn side_limit_fact(f: &Fun<'_>, p: f64, right: bool) -> Option<(TailEnd, Claim)> {
    use crate::ast::{BinOp, Expr};
    let pe = if p < 0.0 {
        Expr::Neg(Box::new(Expr::Num(-p)))
    } else {
        Expr::Num(p)
    };
    let inv = Expr::bin(BinOp::Div, Expr::Num(1.0), Expr::X);
    let shifted = Expr::bin(if right { BinOp::Add } else { BinOp::Sub }, pe, inv);
    let e = f
        .expr
        .map(&|n| matches!(n, Expr::X).then(|| shifted.clone()));
    let at = format!("{p}{}", if right { "⁺" } else { "⁻" });
    limit_of(f, &e, true, &at)
}

/// An enclosure of q·πᵏ.
pub fn piq_interval(v: crate::simplify::PiQ) -> Option<Interval> {
    let mut iv = v.q.interval();
    for _ in 0..v.k.unsigned_abs() {
        iv = if v.k > 0 {
            iv * crate::interval::elem::pi()
        } else {
            iv / crate::interval::elem::pi()
        };
    }
    (!iv.is_empty() && iv.is_bounded()).then_some(iv)
}

// ---------------------------------------------------------- period

/// The most primes a least-period proof checks.
const MAX_PRIMES: u64 = 200;

fn primes_upto(n: u64) -> Vec<u64> {
    (2..=n)
        .filter(|&p| (2..).take_while(|d| d * d <= p).all(|d| p % d != 0))
        .collect()
}

/// The least period, or `None` (not periodic).
///
/// *Not periodic*: a certified domain that is a finite union of pieces
/// other than the whole line can't be invariant under a shift, nor can a
/// function strictly monotone on a piece reaching ±∞.
///
/// *Least period*: the simplifier proves P is a period. A smaller one T
/// would divide it, P = n·T, and then P/p is a period for each prime p
/// dividing n, so it is enough to show P/p is no period (two values of f
/// a shift P/p apart, enclosed apart) for every prime p up to a bound on
/// n: on a continuous f on ℝ, |f′| ≤ L everywhere (f′ has period P too,
/// bounded over one) and two values V apart are reached within any T, so
/// T ≥ V/L and n ≤ P·L/V; with one excluded family of period q, a period
/// maps the family onto itself, so q divides T and n ≤ P/q.
/// Two values of f at some of `xs`, enclosed apart (f is not constant),
/// as claims.
fn apart(f: &Fun<'_>, xs: &[f64]) -> Result<Option<Vec<Claim>>, Stop> {
    let mut vals = Vec::new();
    for &x in xs {
        let v = f.ser_of(&f.expr, Interval::point(x), 0)?[0];
        if !v.is_empty() && v.dec >= Dec::Def && v.iv.is_bounded() {
            vals.push((x, v));
        }
    }
    for (i, (x, v)) in vals.iter().enumerate() {
        for (y, u) in &vals[i + 1..] {
            if u.hi() < v.lo() || v.hi() < u.lo() {
                let claim = |p: f64, w: &DecInterval| Claim::Value {
                    x: XBox::point(p),
                    of: Subject::f(0),
                    lo: R(w.lo()),
                    hi: R(w.hi()),
                };
                return Ok(Some(vec![claim(*x, v), claim(*y, u)]));
            }
        }
    }
    Ok(None)
}

pub fn period(
    f: &Fun<'_>,
    dom: &Domain,
    mono: &Row<Vec<Monotone>>,
    w: f64,
) -> Result<Row<Option<Enc>>, Stop> {
    let line = super::side::line();
    if let Row::Certified { value: d, cert } = &dom.row
        && d.excluded.is_empty()
        && d.pieces != [line]
    {
        let mut c = Certificate::new(Region::Line);
        c.extend(cert.claims.iter().cloned());
        return Ok(Row::Certified {
            value: None,
            cert: c,
        });
    }
    if let Row::Certified { value: m, cert } = mono
        && m.iter()
            .any(|p| p.on.lo == Bound::NegInf || p.on.hi == Bound::PosInf)
    {
        let mut c = Certificate::new(Region::Line);
        c.extend(cert.claims.iter().cloned());
        return Ok(Row::Certified {
            value: None,
            cert: c,
        });
    }
    // A tail going to ±∞ can't repeat, nor can one with a limit unless f
    // is constant (then f(x₀ + kP) = f(x₀) for every k).
    if dom.row.is_certified() && dom.pieces == [line] {
        for right in [false, true] {
            let (end, claims) = tail_end(f, right, w)?;
            let witness = match end {
                TailEnd::Infinite(_) => Some(Vec::new()),
                TailEnd::Level(..) => apart(f, &[0.5, 1.0, 1.7, 2.3, 3.1, -0.7, -1.9])?,
                TailEnd::Unknown => None,
            };
            if let Some(w) = witness {
                let mut c = Certificate::new(Region::Line);
                c.extend(claims);
                c.extend(w);
                return Ok(Row::Certified {
                    value: None,
                    cert: c,
                });
            }
        }
    }
    let Some(s) = f.settings() else {
        return Ok(Row::unknown("no period proven"));
    };
    let Some(per) = crate::simplify::prove_period(&f.expr, &s) else {
        return Ok(Row::unknown("no period proven"));
    };
    let Some(pv) = piq_interval(per.value) else {
        return Ok(Row::unknown("the period is out of range"));
    };
    if pv.lo().is_nan() || pv.lo() <= 0.0 || !dom.row.is_certified() {
        return Ok(Row::unknown("the period's minimality is not proven"));
    }
    let mut c = Certificate::new(Region::Line);
    c.push(Claim::Simplifier {
        fact: format!("f(x + {}) = f(x) wherever f is defined", pi_q(per.value)),
    });
    // How many times a smaller period could fit in P.
    let n = if dom.pieces == [line] && dom.families.is_empty() {
        let k = 64;
        let w = pv.hi() / k as f64;
        let mut l: f64 = 0.0;
        let (mut top, mut bottom) = (f64::NEG_INFINITY, f64::INFINITY);
        for i in 0..k {
            let b = Interval::new(i as f64 * w, (i + 1) as f64 * w);
            let s = f.ser(b, 1)?;
            if !(usable(&s, 1) && s[1].iv.is_bounded()) {
                return Ok(Row::unknown("f′ is not bounded over a period"));
            }
            let (lo, hi) = s[1].iv.mig_mag();
            let _ = lo;
            l = l.max(hi);
            c.push(Claim::Value {
                x: XBox::new(b.lo(), b.hi()),
                of: Subject::f(1),
                lo: R(s[1].lo()),
                hi: R(s[1].hi()),
            });
            let v = f.val(Interval::point(b.lo()))?;
            if !v.is_empty() && v.iv.is_bounded() && v.dec >= Dec::Def {
                c.push(Claim::Value {
                    x: XBox::point(b.lo()),
                    of: Subject::f(0),
                    lo: R(v.lo()),
                    hi: R(v.hi()),
                });
                top = top.max(v.lo());
                bottom = bottom.min(v.hi());
            }
        }
        let v = top - bottom;
        if v.is_nan() || v <= 0.0 || l.is_nan() || l <= 0.0 {
            return Ok(Row::unknown("the period's minimality is not proven"));
        }
        // f′ is (at most) the coefficient bound; round the ratio up.
        (pv.hi() * l / v * (1.0 + 1e-12)).floor()
    } else if dom.pieces == [line] && dom.families.len() == 1 {
        let q = dom.families[0].period;
        if q.lo().is_nan() || q.lo() <= 0.0 {
            return Ok(Row::unknown("the period's minimality is not proven"));
        }
        (pv.hi() / q.lo() * (1.0 + 1e-12)).floor()
    } else {
        return Ok(Row::unknown("the period's minimality is not proven"));
    };
    if !(n.is_finite() && n <= 1e6) {
        return Ok(Row::unknown("the period's minimality is not proven"));
    }
    let primes = primes_upto(n as u64);
    if primes.len() as u64 > MAX_PRIMES {
        return Ok(Row::unknown("the period's minimality is not proven"));
    }
    // Each P/p shifts some sample's value away from itself.
    for p in primes {
        let shift = pv / Interval::point(p as f64);
        let mut found = false;
        for i in 0..24 {
            let x = 0.1 + i as f64 * pv.mid() / 24.0;
            let a = f.ser_of(&f.expr, Interval::point(x), 0)?[0];
            let xs = Interval::point(x) + shift;
            let b = f.ser_of(&f.expr, xs, 0)?[0];
            let def = |v: &DecInterval| !v.is_empty() && v.dec >= Dec::Def && v.iv.is_bounded();
            if def(&a) && def(&b) && (a.hi() < b.lo() || b.hi() < a.lo()) {
                for (bx, v) in [(Interval::point(x), a), (xs, b)] {
                    c.push(Claim::Value {
                        x: XBox::new(bx.lo(), bx.hi()),
                        of: Subject::f(0),
                        lo: R(v.lo()),
                        hi: R(v.hi()),
                    });
                }
                found = true;
                break;
            }
        }
        if !found {
            return Ok(Row::unknown(format!("P/{p} is not shown to be no period")));
        }
    }
    Ok(Row::Certified {
        value: Some(Enc::new(pv.lo(), pv.hi())),
        cert: c,
    })
}

/// q·πᵏ written out: `1/2·π`, `-3`, `2·π^-1`.
fn pi_q(v: crate::simplify::PiQ) -> String {
    let q = if v.q.is_int() {
        format!("{}", v.q.numer())
    } else {
        format!("{}/{}", v.q.numer(), v.q.denom())
    };
    match v.k {
        0 => q,
        1 => format!("{q}·π"),
        k => format!("{q}·π^{k}"),
    }
}

fn tail_interval(f: &Fun<'_>, right: bool, from: f64) -> Result<(TailEnd, Vec<Claim>), Stop> {
    let side = if right { Tail::Right } else { Tail::Left };
    let start = from.abs().max(1.0);
    let from_r = R(if right { start } else { -start });
    // f′ of one sign and moving away from 0 out along the tail (a sign
    // chain through f″, f‴): |f′| ≥ |f′(start)| > 0, so f goes to ±∞.
    let t = super::cover::Target {
        expr: &f.eval,
        k: 1,
        in_domain: true,
    };
    let (a, b) = if right {
        (start, f64::INFINITY)
    } else {
        (f64::NEG_INFINITY, -start)
    };
    if let Some(Leaf::Band { band, chain, .. }) = super::cover::tail_chain(f, &t, &[0.0], a, b)? {
        let rising = band == 1;
        let claim = Claim::TailChain {
            side,
            from: from_r,
            of: Subject::f(1),
            c: R(0.0),
            above: rising,
            order: chain as u8,
        };
        return Ok((TailEnd::Infinite(rising == right), vec![claim]));
    }
    let Some((rising, d)) = tail_sign(f, right, start)? else {
        return Ok((TailEnd::Unknown, Vec::new()));
    };
    // f′ bounded away from 0 (beyond half its bound): f goes to ±∞.
    let c = if !d.ne0() {
        0.0
    } else if rising {
        d.lo() / 2.0
    } else {
        d.hi() / 2.0
    };
    if c != 0.0 {
        let claim = Claim::TailBeyond {
            side,
            from: from_r,
            of: Subject::f(1),
            c: R(c),
            above: rising,
        };
        // Rising to the right goes to +∞; rising to the left to −∞.
        return Ok((TailEnd::Infinite(rising == right), vec![claim]));
    }
    // Strictly monotone: a limit if bounded. Every band of f far out
    // holds the limit; keep their intersection.
    let mut claims = vec![Claim::TailBeyond {
        side,
        from: from_r,
        of: Subject::f(1),
        c: R(0.0),
        above: rising,
    }];
    let bands = thin(&far_bands(f, right, start, Some(rising))?, KEEP);
    let mut lim: Option<Interval> = None;
    for (b, cl) in &bands {
        claims.extend(cl.iter().cloned());
        lim = Some(lim.map_or(*b, |l| l.intersect(*b)));
    }
    match (lim, bands.last()) {
        (Some(l), Some((last, _))) if !l.is_empty() && narrow(&DecInterval::new(*last)) => {
            Ok((TailEnd::Level(Enc::new(l.lo(), l.hi()), None), claims))
        }
        _ => Ok((TailEnd::Unknown, Vec::new())),
    }
}

/// f proven a line on its domain (a constant, m·x + b, holes aside): its
/// graph is the line itself, so it has no asymptote
/// (`docs/ti-conventions.md`). From f's exact rational form (N/D reduced
/// to a polynomial of degree ≤ 1), or the simplifier's form of f, an exact
/// a·x + b (`e^(ln x)` is x where defined).
fn own_line(f: &Fun<'_>) -> Option<Claim> {
    let exact = f.exact?;
    if let Some(rf) = crate::simplify::rational_form(&f.expr, exact) {
        let (n, d) = (&rf.reduced.num, &rf.reduced.den);
        if d.degree() != Some(0) || n.degree().unwrap_or(0) > 1 {
            return None;
        }
        let c = n.coefficients();
        let k = d.lead();
        let at = |i: usize| c.get(i).copied().unwrap_or(crate::simplify::Q::ZERO).div(k);
        let (m, b) = (at(1)?, at(0)?);
        return Some(Claim::Simplifier {
            fact: format!("{RATIONAL}the line {m}·x + {b} on its domain: no asymptote"),
        });
    }
    if f.eval == f.expr {
        return None;
    }
    let (m, b) = super::fun::affine(&f.eval, exact)?;
    Some(Claim::Simplifier {
        fact: format!("f = {m}·x + {b} on its domain (the simplifier's form): no asymptote"),
    })
}

/// Sample points of the window where f is defined, with its values.
fn samples(f: &Fun<'_>, scope: &Scope) -> Result<Vec<(f64, DecInterval)>, Stop> {
    let (a, b) = scope.window;
    let mut vals = Vec::new();
    for i in 0..16 {
        let x = a + (b - a) * (i as f64 + 0.5) / 16.0;
        let v = f.val(Interval::point(x))?;
        if !v.is_empty() && v.dec >= Dec::Def && v.iv.is_bounded() {
            vals.push((x, v));
        }
    }
    Ok(vals)
}

fn value_claim(x: f64, v: DecInterval) -> Claim {
    Claim::Value {
        x: XBox::point(x),
        of: Subject::f(0),
        lo: R(v.lo()),
        hi: R(v.hi()),
    }
}

/// Two values of f apart: f is no constant (its own line, with no
/// asymptote), and a periodic f has no limit at ±∞. At points of the
/// window, then out along both tails (e^(x − 10⁶) underflows in a window
/// about 0).
fn not_constant(f: &Fun<'_>, scope: &Scope) -> Result<Option<Vec<Claim>>, Stop> {
    let mut vals = samples(f, scope)?;
    if scope.period.is_none() {
        for right in [false, true] {
            for m in thin(&far_points(f, right, 1.0), 24) {
                let x = if right { m } else { -m };
                let v = f.val(Interval::point(x))?;
                if !v.is_empty() && v.dec >= Dec::Def && v.iv.is_bounded() {
                    vals.push((x, v));
                }
            }
        }
    }
    Ok(vals.iter().find_map(|(x, v)| {
        vals.iter()
            .find(|(_, u)| u.hi() < v.lo() || v.hi() < u.lo())
            .map(|(y, u)| vec![value_claim(*x, *v), value_claim(*y, *u)])
    }))
}

/// Three values of f off one line (the slopes between them apart): f is
/// no m·x + b, so a line it approaches is not its own graph.
fn not_affine(f: &Fun<'_>, scope: &Scope) -> Result<Option<Vec<Claim>>, Stop> {
    let vals = samples(f, scope)?;
    let slope = |(x, v): &(f64, DecInterval), (y, u): &(f64, DecInterval)| {
        (u.iv - v.iv) / (Interval::point(*y) - Interval::point(*x))
    };
    for w in vals.windows(3) {
        let (s, t) = (slope(&w[0], &w[1]), slope(&w[1], &w[2]));
        if s.hi() < t.lo() || t.hi() < s.lo() {
            return Ok(Some(w.iter().map(|(x, v)| value_claim(*x, *v)).collect()));
        }
    }
    Ok(None)
}

pub fn horizontal(f: &Fun<'_>, dom: &Domain, scope: &Scope) -> Result<Row<Vec<Horizontal>>, Stop> {
    if !dom.row.is_certified() {
        return Ok(Row::unknown("the domain's tails are not known"));
    }
    if let Some(fact) = own_line(f) {
        let mut c = Certificate::new(Region::Line);
        c.push(fact);
        return Ok(Row::Certified {
            value: Vec::new(),
            cert: c,
        });
    }
    if scope.period.is_some() {
        // A periodic f with a limit at ±∞ would be constant: two values
        // apart show it has none.
        let Some(apart) = not_constant(f, scope)? else {
            return Ok(Row::unknown("a tail's limit is not decided"));
        };
        let mut c = Certificate::new(Region::Line);
        c.extend(apart);
        return Ok(Row::Certified {
            value: Vec::new(),
            cert: c,
        });
    }
    if !scope.whole {
        return Ok(Row::unknown("the domain's tails are not known"));
    }
    let w = scope.w;
    let mut out = Vec::new();
    let mut unbounded = false;
    let mut c = Certificate::new(Region::Line);
    for right in [false, true] {
        // Does the domain reach this tail?
        let reaches = dom.pieces.iter().any(|p| {
            if right {
                p.hi == Bound::PosInf
            } else {
                p.lo == Bound::NegInf
            }
        });
        if !reaches {
            continue;
        }
        let (end, claims) = tail_end(f, right, w)?;
        match end {
            TailEnd::Infinite(_) => unbounded = true,
            TailEnd::Level(y, exact) => out.push(Horizontal {
                side: if right { Tail::Right } else { Tail::Left },
                y,
                looks_like: if exact.is_some() { None } else { looks_like(y) },
                exact,
            }),
            TailEnd::Unknown => return Ok(Row::unknown("a tail's limit is not decided")),
        }
        c.extend(claims);
    }
    // A constant has no asymptote: the line listed is not f's own graph
    // (f unbounded on the other tail, or two limits apart, show it too).
    let apart = out.len() == 2 && (out[0].y.hi.0 < out[1].y.lo.0 || out[1].y.hi.0 < out[0].y.lo.0);
    if !out.is_empty() && !unbounded && !apart {
        let Some(apart) = not_constant(f, scope)? else {
            return Ok(Row::unknown("f is not shown to be other than a constant"));
        };
        c.extend(apart);
    }
    Ok(Row::Certified {
        value: out,
        cert: c,
    })
}

// ---------------------------------------------------------- oblique

/// Interval evidence on f′ along a tail, for the simplifier's f/x → ±∞
/// (`grow`: f′ beyond bounds growing in size, so f′ → ±∞ and f/x with it)
/// or f/x → 0 (f′ within bands about 0 narrowing to 10⁻¹², so f′ → 0 and
/// f/x with it): `TailBeyond` claims on f′ at each tail of the far
/// sequence, or, f′ monotone on the tail, its values at points
/// ([`slope_monotone`]). `None` when neither bears it out.
fn slope_evidence(
    f: &Fun<'_>,
    right: bool,
    start: f64,
    grow: bool,
) -> Result<Option<Vec<Claim>>, Stop> {
    if let Some(ev) = slope_bounds(f, right, start, grow)? {
        return Ok(Some(ev));
    }
    slope_monotone(f, right, start, grow)
}

/// f′ monotone on the tail (f″ strictly signed there), moving away from
/// its value at each M out along it: for `grow`, those values growing in
/// size (e^x/x, whose f′ on [M, ∞) is e^x/x − e^x/x², ∞ − ∞); else its
/// bands between its value at M and its enclosure's bound on [M, ∞) the
/// other way, the farthest within 10⁻¹² of 0 (√x·ln x).
fn slope_monotone(
    f: &Fun<'_>,
    right: bool,
    start: f64,
    grow: bool,
) -> Result<Option<Vec<Claim>>, Stop> {
    let side = if right { Tail::Right } else { Tail::Left };
    let Some((rising, _)) = tail_sign_k(f, 2, right, start)? else {
        return Ok(None);
    };
    let away_up = rising == right;
    let mut claims = vec![Claim::TailBeyond {
        side,
        from: R(if right { start } else { -start }),
        of: Subject::f(2),
        c: R(0.0),
        above: rising,
    }];
    // f′ at the point x, valid there.
    let at = |x: f64| -> Result<Option<DecInterval>, Stop> {
        let s = f.ser(Interval::point(x), 1)?;
        Ok((usable(&s, 1) && s[1].iv.is_bounded()).then_some(s[1]))
    };
    if grow {
        let mut pts: Vec<(f64, f64, f64)> = Vec::new();
        for m in far_points(f, right, start) {
            let x = if right { m } else { -m };
            if let Some(v) = at(x)? {
                pts.push((x, v.lo(), v.hi()));
            }
        }
        let cs: Vec<f64> = pts
            .iter()
            .map(|p| if away_up { p.1 } else { p.2 })
            .collect();
        let rec = records(&cs, away_up);
        if !grows_enough(&rec.iter().map(|&i| cs[i]).collect::<Vec<_>>()) {
            return Ok(None);
        }
        let kept: Vec<(f64, f64, f64)> = rec.iter().map(|&i| pts[i]).collect();
        for &(x, lo, hi) in &thin(&kept, KEEP) {
            claims.push(Claim::Value {
                x: XBox::point(x),
                of: Subject::f(1),
                lo: R(lo),
                hi: R(hi),
            });
        }
        return Ok(Some(claims));
    }
    // Bands about 0: f′(M) one way, f′'s enclosure on [M, ∞) the other.
    let mut last = f64::INFINITY;
    let ms = far_points(f, right, start);
    for &m in &ms[ms.len().saturating_sub(3)..] {
        let x = if right { m } else { -m };
        let Some(p) = at(x)? else { continue };
        let s = f.ser(tail_box(right, m), 1)?;
        if !usable(&s, 1) {
            continue;
        }
        let (c, band) = if away_up {
            let c = s[1].hi().next_up();
            (c, (p.lo(), c))
        } else {
            let c = s[1].lo().next_down();
            (c, (c, p.hi()))
        };
        if !c.is_finite() {
            continue;
        }
        last = band.0.abs().max(band.1.abs());
        claims.push(Claim::Value {
            x: XBox::point(x),
            of: Subject::f(1),
            lo: R(p.lo()),
            hi: R(p.hi()),
        });
        claims.push(Claim::TailBeyond {
            side,
            from: R(x),
            of: Subject::f(1),
            c: R(c),
            above: !away_up,
        });
    }
    Ok((last <= 1e-12).then_some(claims))
}

/// f″ beyond a nonzero bound c on a tail (a `TailBeyond` on f″): f′ moves
/// past f′(M) + c·(x − M) out along it, so |f′| → ∞ and |f/x| with it,
/// and no line is approached (x·|x|, xˣ, 2ˣ). The first tail of the far
/// sequence where f″'s enclosure is away from 0.
fn bends_away(f: &Fun<'_>, right: bool, start: f64) -> Result<Option<Claim>, Stop> {
    let side = if right { Tail::Right } else { Tail::Left };
    for m in thin(&far_points(f, right, start), 12) {
        let s = f.ser(tail_box(right, m), 2)?;
        // s[2] = f″/2: f″ is beyond s[2]'s bound when that is past 0.
        // Else (xˣ's series: ∞/∞) f″'s own tree, f smooth and continuous
        // on the tail: f″ beyond half its bound.
        let d = if usable(&s, 2) && (s[2].lo() > 0.0 || s[2].hi() < 0.0) {
            s[2]
        } else if f.smooth_tree
            && !s[0].is_empty()
            && s[0].dec >= Dec::Dac
            && let Some(t) = f.derivs().map(|d| d[1].clone())
        {
            let v = f.ser_of(&t, tail_box(right, m), 0)?[0];
            if v.is_empty() || v.dec < Dec::Dac {
                continue;
            }
            DecInterval {
                iv: v.iv * Interval::point(0.5),
                ..v
            }
        } else {
            continue;
        };
        let c = if d.lo() > 0.0 {
            d.lo()
        } else if d.hi() < 0.0 {
            d.hi()
        } else {
            continue;
        };
        return Ok(Some(Claim::TailBeyond {
            side,
            from: R(if right { m } else { -m }),
            of: Subject::f(2),
            c: R(c),
            above: c > 0.0,
        }));
    }
    Ok(None)
}

/// f′ beyond bounds on the tails of the far sequence ([`slope_evidence`]).
fn slope_bounds(
    f: &Fun<'_>,
    right: bool,
    start: f64,
    grow: bool,
) -> Result<Option<Vec<Claim>>, Stop> {
    let side = if right { Tail::Right } else { Tail::Left };
    let mut claims = Vec::new();
    let mut prev: Option<f64> = None;
    let mut last = f64::INFINITY;
    for m in far_points(f, right, start) {
        let s = f.ser(tail_box(right, m), 1)?;
        // f′ on the tail: its Taylor coefficient (unbounded will do for a
        // bound one way: 2|x| on [M, ∞)), or where that is ∞/∞ (x^0.9:
        // 0.9·x^0.9/x) its own tree (f smooth and defined there).
        let d = if usable(&s, 1) && (grow || s[1].iv.is_bounded()) {
            s[1]
        } else if f.smooth_tree
            && !s[0].is_empty()
            && s[0].dec >= Dec::Dac
            && let Some(t) = f.derivs().map(|d| d[0].clone())
        {
            let v = f.ser_of(&t, tail_box(right, m), 0)?[0];
            if v.is_empty() || v.dec < Dec::Def {
                continue;
            }
            v
        } else {
            continue;
        };
        let from = R(if right { m } else { -m });
        if grow {
            let (c, above) = if d.gt0() {
                (d.lo().next_down(), true)
            } else if d.lt0() {
                (d.hi().next_up(), false)
            } else {
                continue;
            };
            // (Past where f′ overflows, no more bounds.)
            if !c.is_finite() || c.abs() >= f64::MAX / 16.0 {
                break;
            }
            // A bound of 0 or past it (f′ underflowing) says nothing.
            if (above && c <= 0.0) || (!above && c >= 0.0) {
                continue;
            }
            if prev.is_some_and(|q: f64| (c > 0.0) != (q > 0.0)) {
                return Ok(None);
            }
            prev = Some(c);
            claims.push(Claim::TailBeyond {
                side,
                from,
                of: Subject::f(1),
                c: R(c),
                above,
            });
        } else {
            if !d.iv.is_bounded() {
                continue;
            }
            let (lo, hi) = (d.lo().next_down(), d.hi().next_up());
            last = lo.abs().max(hi.abs());
            for (c, above) in [(lo, true), (hi, false)] {
                claims.push(Claim::TailBeyond {
                    side,
                    from,
                    of: Subject::f(1),
                    c: R(c),
                    above,
                });
            }
        }
    }
    if grow {
        // Growing in size: the run at the end that does, as claims.
        let cs: Vec<f64> = claims
            .iter()
            .filter_map(|c| match c {
                Claim::TailBeyond { c, .. } => Some(c.0.abs()),
                _ => None,
            })
            .collect();
        let rec = records(&cs, true);
        if !grows_enough(&rec.iter().map(|&i| cs[i]).collect::<Vec<_>>()) {
            return Ok(None);
        }
        let kept: Vec<Claim> = rec.iter().map(|&i| claims[i].clone()).collect();
        return Ok(Some(thin(&kept, KEEP)));
    }
    Ok((last <= 1e-12).then_some(claims))
}

/// Oblique asymptotes y = m·x + b (m ≠ 0) at each tail the domain reaches.
/// A line on its domain has none (its graph is the line). A rational f has
/// one exactly when deg N = deg D + 1 (from its exact rational form).
/// Otherwise, per tail: a horizontal asymptote there rules one out, so do
/// f″ away from 0 and f/x → ±∞ or exactly 0 by the tree's structure; f's
/// expansion in 1/x gives a line with f − (m·x + b) enclosed on the tail;
/// f′'s evidence far out bears out the simplifier's f/x → ±∞ or → 0. A
/// periodic f has none (f − (m·x + b) can't tend to 0). A line listed
/// comes with three values of f off one line.
pub fn oblique(
    f: &Fun<'_>,
    dom: &Domain,
    horizontal: &Row<Vec<Horizontal>>,
    scope: &Scope,
) -> Result<Row<Vec<Oblique>>, Stop> {
    use crate::ast::{BinOp, Expr};
    use crate::simplify::{Dir as LDir, Limit, limit_at};
    if !dom.row.is_certified() {
        return Ok(Row::unknown("the domain's tails are not known"));
    }
    let reaches = |right: bool| {
        dom.pieces.iter().any(|p| {
            if right {
                p.hi == Bound::PosInf
            } else {
                p.lo == Bound::NegInf
            }
        })
    };
    let mut c = Certificate::new(Region::Line);
    if let Some(fact) = own_line(f) {
        c.push(fact);
        return Ok(Row::Certified {
            value: Vec::new(),
            cert: c,
        });
    }
    if scope.period.is_some() {
        c.push(Claim::Simplifier {
            fact: "f is periodic: f − (m·x + b) does not tend to 0 for m ≠ 0".into(),
        });
        return Ok(Row::Certified {
            value: Vec::new(),
            cert: c,
        });
    }
    let enc = |q: crate::simplify::Q| {
        let iv = q.interval();
        Enc::new(iv.lo(), iv.hi())
    };
    if let Some(exact) = f.exact
        && let Some(rf) = crate::simplify::rational_form(&f.expr, exact)
    {
        let mut out = Vec::new();
        match rf.reduced.oblique() {
            Some((m, b)) => {
                c.push(Claim::Simplifier {
                    fact: format!("f = N/D exactly and N/D − ({m}·x + {b}) → 0 as x → ±∞"),
                });
                // (Not f's own graph: f is no m·x + b.)
                let Some(off) = not_affine(f, scope)? else {
                    return Ok(Row::unknown("f is not shown to be other than a line"));
                };
                c.extend(off);
                for right in [false, true] {
                    if reaches(right) {
                        out.push(Oblique {
                            side: if right { Tail::Right } else { Tail::Left },
                            m: enc(m),
                            b: enc(b),
                        });
                    }
                }
            }
            None => c.push(Claim::Simplifier {
                fact: "f = N/D exactly with deg N ≠ deg D + 1: no oblique asymptote".into(),
            }),
        }
        return Ok(Row::Certified {
            value: out,
            cert: c,
        });
    }
    let Row::Certified { value: hs, .. } = horizontal else {
        return Ok(Row::unknown("the tails are not decided"));
    };
    let Some(settings) = f.settings() else {
        return Ok(Row::unknown("the tails are not decided"));
    };
    let mut lines = Vec::new();
    for right in [false, true] {
        if !reaches(right) {
            continue;
        }
        let side = if right { Tail::Right } else { Tail::Left };
        if hs.iter().any(|h| h.side == side) {
            // A horizontal asymptote on this side: no oblique one.
            continue;
        }
        if f.cancelled() {
            return Err(Stop::Cancelled);
        }
        let dir = if right { LDir::PosInf } else { LDir::NegInf };
        let at = if right { "+∞" } else { "−∞" };
        let over_x = Expr::bin(BinOp::Div, f.expr.clone(), Expr::X);
        let start = scope.w.abs().max(1.0);
        // f″ away from 0 on the tail: no line, by intervals alone.
        if let Some(bend) = bends_away(f, right, start)? {
            c.push(bend);
            continue;
        }
        // f/x → ±∞ or exactly 0 by the tree's structure: no line (a line
        // has f/x → m ≠ 0).
        let toward = if right {
            Toward::PosInf
        } else {
            Toward::NegInf
        };
        match super::growth::limit(f, toward, true) {
            Some(to @ (To::PosInf | To::NegInf)) => {
                c.push(Claim::Limit {
                    at: toward,
                    over_x: true,
                    to,
                });
                continue;
            }
            Some(to @ To::In { lo, hi }) if lo.0 == 0.0 && hi.0 == 0.0 => {
                c.push(Claim::Limit {
                    at: toward,
                    over_x: true,
                    to,
                });
                continue;
            }
            _ => {}
        }
        // A line from f's expansion in 1/x, f − (m·x + b) enclosed on the
        // tails of the far sequence.
        if let Some((line, claims)) = expanded_line(f, right, start)? {
            c.extend(claims);
            lines.push(line);
            continue;
        }
        match limit_at(&over_x, dir, &settings) {
            // (Each with f′'s enclosures far out bearing it out.)
            Limit::PosInf | Limit::NegInf => {
                let Some(ev) = slope_evidence(f, right, start, true)? else {
                    return Ok(Row::unknown("a tail's slope is not decided"));
                };
                c.extend(ev);
                c.push(Claim::Simplifier {
                    fact: format!("f/x → ±∞ as x → {at}: no oblique asymptote"),
                });
            }
            Limit::Exact(m) if m.q.is_zero() => {
                let Some(ev) = slope_evidence(f, right, start, false)? else {
                    return Ok(Row::unknown("a tail's slope is not decided"));
                };
                c.extend(ev);
                c.push(Claim::Simplifier {
                    fact: format!("f/x → 0 as x → {at} and f has no horizontal asymptote there"),
                });
            }
            // A slope the simplifier finds with no line from f's expansion:
            // none at all where f − m·x is periodic and not constant.
            Limit::Exact(m) => match periodic_rest(f, m, scope)? {
                // (Both tails at once.)
                Some(claims) => {
                    if !claims.iter().all(|x| c.claims.contains(x)) {
                        c.extend(claims);
                    }
                }
                None => return Ok(Row::unknown("a tail's line is not borne out")),
            },
            _ => return Ok(Row::unknown("a tail's slope is not decided")),
        }
    }
    // A line listed is not f's own graph: f is no m·x + b.
    if !lines.is_empty() {
        let Some(off) = not_affine(f, scope)? else {
            return Ok(Row::unknown("f is not shown to be other than a line"));
        };
        c.extend(off);
    }
    Ok(Row::Certified {
        value: lines,
        cert: c,
    })
}

/// No oblique asymptote at all, for f − m·x periodic and not constant
/// (x − sin x, sin x + x/10, with m the simplifier's limit of f/x): a
/// periodic G = f − m·x with a limit would be constant, and with a line of
/// another slope m′ G would grow like (m′ − m)·x, which a periodic G can't.
/// The simplifier's form of f − m·x, its proven period, and two values of
/// f with f − m·x apart.
fn periodic_rest(
    f: &Fun<'_>,
    m: crate::simplify::PiQ,
    scope: &Scope,
) -> Result<Option<Vec<Claim>>, Stop> {
    use crate::ast::{BinOp, Constant, Expr};
    let Some(s) = f.settings() else {
        return Ok(None);
    };
    let Some(mv) = piq_interval(m) else {
        return Ok(None);
    };
    let mut me = crate::simplify::rational::q_expr(m.q);
    for _ in 0..m.k.unsigned_abs() {
        let op = if m.k > 0 { BinOp::Mul } else { BinOp::Div };
        me = Expr::bin(op, me, Expr::Const(Constant::Pi));
    }
    let g = Expr::bin(
        BinOp::Sub,
        f.expr.clone(),
        Expr::bin(BinOp::Mul, me, Expr::X),
    );
    let Ok(simple) = crate::simplify::simplify(&g, &s) else {
        return Ok(None);
    };
    let Some(per) = crate::simplify::prove_period(&simple.expr, &s) else {
        return Ok(None);
    };
    // Two values of f − m·x apart.
    let vals = samples(f, scope)?;
    let rest = |(x, v): &(f64, DecInterval)| v.iv - mv * Interval::point(*x);
    let apart = vals.iter().find_map(|a| {
        vals.iter()
            .find(|b| rest(b).hi() < rest(a).lo())
            .map(|b| vec![value_claim(a.0, a.1), value_claim(b.0, b.1)])
    });
    let Some(mut claims) = apart else {
        return Ok(None);
    };
    claims.push(Claim::Simplifier {
        fact: format!(
            "f − {}·x = {} has period {}: no oblique asymptote",
            pi_q(m),
            simple.expr.formula(),
            pi_q(per.value)
        ),
    });
    Ok(Some(claims))
}

/// An oblique line on one tail from f's expansions in t = 1/x
/// ([`super::expand`]) over tails of the far sequence: f = m·x + b + O(1/x)
/// with the same m (≠ 0) and b on each, f − (m·x + b)'s range on each a
/// `TailLine` claim — bands around 0, settling.
fn expanded_line(
    f: &Fun<'_>,
    right: bool,
    start: f64,
) -> Result<Option<(Oblique, Vec<Claim>)>, Stop> {
    let side = if right { Tail::Right } else { Tail::Left };
    let mut claims = Vec::new();
    let mut mb: Option<(Interval, Interval)> = None;
    for m in thin(&far_points(f, right, start), 8) {
        if f.cancelled() {
            return Err(Stop::Cancelled);
        }
        let Some((slope, b, band)) = super::expand::line(f, right, m) else {
            continue;
        };
        mb = Some(match mb {
            None => (slope, b),
            Some((s, c)) => (s.intersect(slope), c.intersect(b)),
        });
        claims.push(Claim::TailLine {
            side,
            from: R(if right { m } else { -m }),
            m: Enc::new(slope.lo(), slope.hi()),
            b: Enc::new(b.lo(), b.hi()),
            lo: R(band.lo()),
            hi: R(band.hi()),
        });
    }
    let Some((slope, b)) = mb else {
        return Ok(None);
    };
    if slope.is_empty() || b.is_empty() {
        return Ok(None);
    }
    Ok(Some((
        Oblique {
            side,
            m: Enc::new(slope.lo(), slope.hi()),
            b: Enc::new(b.lo(), b.hi()),
        },
        claims,
    )))
}

// ---------------------------------------------------------- range

/// Where a finite end of the range comes from, when it is one value (not
/// the hull of two ends that couldn't be told apart): what the panel
/// needs to write it exactly.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum EndSrc {
    /// f's value at the point enclosed (attained).
    At(Enc),
    /// f's limit as x → ±∞ on this side.
    Tail(Tail),
    /// f's limit at the excluded point enclosed: a removable hole.
    Hole(Enc),
    /// The simplifier's one-sided limit at the double `at`, from the right
    /// when `right`.
    Side { at: R, right: bool },
    /// Infinite, or more than one value's hull.
    Other,
}

/// Where an end comes from: one source, or those of the ends whose hull
/// it is (it is one of their values).
#[derive(Clone, Copy, Debug)]
struct Srcs {
    s: [EndSrc; 8],
    n: usize,
}

impl Srcs {
    fn one(e: EndSrc) -> Srcs {
        let mut s = [EndSrc::Other; 8];
        s[0] = e;
        Srcs { s, n: 1 }
    }

    fn join(a: Srcs, b: Srcs) -> Srcs {
        if a.n + b.n > a.s.len() {
            return Srcs::one(EndSrc::Other);
        }
        let mut s = a.s;
        s[a.n..a.n + b.n].copy_from_slice(&b.s[..b.n]);
        Srcs { s, n: a.n + b.n }
    }

    fn to_vec(self) -> Vec<EndSrc> {
        self.s[..self.n].to_vec()
    }
}

/// One end of a monotone piece's image.
#[derive(Clone, Copy, Debug)]
struct End {
    v: Enc,
    closed: bool,
    infinite: Option<bool>,
    /// Where f takes this value, for an attained end: two ends taken at the
    /// same place are the same value, however wide their enclosures.
    at: Option<Enc>,
    src: Srcs,
}

impl End {
    /// Provably the same value.
    fn same(&self, o: &End) -> bool {
        (self.v.is_point() && self.v == o.v) || (self.at.is_some() && self.at == o.at)
    }
}

/// The low (`low`) or high end of the union of two images with these ends;
/// None if it can't be told which, nor bounded soundly.
fn outer(a: End, b: End, low: bool) -> Option<End> {
    if a.v.hi.0 < b.v.lo.0 {
        return Some(if low { a } else { b });
    }
    if b.v.hi.0 < a.v.lo.0 {
        return Some(if low { b } else { a });
    }
    if a.same(&b) {
        return Some(End {
            closed: a.closed || b.closed,
            ..a
        });
    }
    // The extreme of two values lies in the hull of their enclosures, and is
    // attained if both are, missed if neither is.
    (a.closed == b.closed && a.infinite.is_none() && b.infinite.is_none()).then(|| End {
        v: Enc::new(a.v.lo.0.min(b.v.lo.0), a.v.hi.0.max(b.v.hi.0)),
        closed: a.closed,
        infinite: None,
        at: None,
        src: Srcs::join(a.src, b.src),
    })
}

/// Does an image starting at `lo` join one ending at `hi` (a single stretch
/// of values)? None if it can't be told.
fn joins(hi: End, lo: End) -> Option<bool> {
    if lo.v.lo.0 > hi.v.hi.0 {
        Some(false)
    } else if lo.v.hi.0 < hi.v.lo.0 {
        Some(true)
    } else if hi.same(&lo) {
        // Meeting at one value: left out only if neither image takes it.
        Some(hi.closed || lo.closed)
    } else if lo.v.lo.0 >= hi.v.hi.0 && !hi.closed && !lo.closed {
        // Everything in the one is below hi.v.hi, everything in the other
        // above it.
        Some(false)
    } else {
        None
    }
}

/// The union of images, as disjoint stretches in increasing order; None if
/// two of them can't be told apart or joined.
fn union(mut images: Vec<(End, End)>) -> Option<Vec<(End, End)>> {
    loop {
        images.sort_by(|a, b| a.0.v.lo.0.total_cmp(&b.0.v.lo.0));
        let n = images.len();
        let mut merged: Vec<(End, End)> = Vec::new();
        for (lo, hi) in images {
            match merged.last_mut() {
                Some(last) if joins(last.1, lo)? => {
                    *last = (outer(last.0, lo, true)?, outer(last.1, hi, false)?);
                }
                _ => merged.push((lo, hi)),
            }
        }
        if merged.len() == n {
            return Some(merged);
        }
        images = merged;
    }
}

/// The value f approaches or takes at one end of a monotone piece, with
/// the claims that prove it added to `cl`.
#[allow(clippy::too_many_arguments)]
fn end_value(
    f: &Fun<'_>,
    b: Bound,
    clipped: bool,
    is_lo: bool,
    boxes: &[IBox],
    c0: &Cover,
    scope: &Scope,
    cl: &mut Vec<Claim>,
) -> Result<Option<End>, Stop> {
    let w = scope.w;
    if clipped {
        // A window edge: over one period of a periodic f a point like any
        // other, its value taken; otherwise not an end of f's range.
        return match (scope.period, b) {
            (Some(_), Bound::At { x, .. }) => attained(f, x, cl),
            _ => Ok(None),
        };
    }
    let infinite = |up: bool| End {
        v: Enc::point(if up { f64::INFINITY } else { f64::NEG_INFINITY }),
        closed: false,
        infinite: Some(up),
        at: None,
        src: Srcs::one(EndSrc::Other),
    };
    Ok(match b {
        Bound::NegInf | Bound::PosInf => {
            let (end, claims) = tail_end(f, b == Bound::PosInf, w)?;
            cl.extend(claims);
            match end {
                TailEnd::Infinite(up) => Some(infinite(up)),
                TailEnd::Level(e, _) => Some(End {
                    v: e,
                    closed: false,
                    infinite: None,
                    at: None,
                    src: Srcs::one(EndSrc::Tail(if b == Bound::PosInf {
                        Tail::Right
                    } else {
                        Tail::Left
                    })),
                }),
                TailEnd::Unknown => None,
            }
        }
        Bound::At { x, closed: true } => attained(f, x, cl)?,
        Bound::At { x, closed: false } => {
            // The box this end belongs to.
            let Some(ib) = boxes
                .iter()
                .find(|bx| if is_lo { bx.lo == b } else { bx.hi == b })
            else {
                return Ok(None);
            };
            // A pole (two-sided, or at a domain end): ±∞ with f's sign
            // beside it.
            let n = match around(x, boxes) {
                Some(n) => pole(f, &f.eval, n)?.then_some(n),
                None => {
                    let p = if is_lo { x.hi.0 } else { x.lo.0 };
                    let n = beside(p, ib, is_lo);
                    (x.is_point() && end_pole(f, &f.eval, n, p)?).then_some(n)
                }
            };
            if let Some(n) = n
                && let Some(up) = gap_sign(f, x, ib, is_lo, c0, boxes)?
            {
                cl.push(Claim::Unbounded {
                    near: XBox::new(n.lo(), n.hi()),
                    at: XBox { a: x.lo, b: x.hi },
                });
                return Ok(Some(infinite(up)));
            }
            let n = around(x, boxes).unwrap_or_else(|| {
                let p = if is_lo { x.hi.0 } else { x.lo.0 };
                beside(p, ib, is_lo).hull(Interval::new(x.lo.0, x.hi.0))
            });
            if let Some(e) = removable(f, x, n, cl)? {
                return Ok(Some(e));
            }
            // The simplifier's one-sided limit (the piece lies right of x
            // when x is its low end).
            if !x.is_point() {
                return Ok(None);
            }
            let Some((end, claims)) = side_limit(f, x.lo.0, is_lo)? else {
                return Ok(None);
            };
            cl.extend(claims);
            match end {
                TailEnd::Infinite(up) => Some(infinite(up)),
                TailEnd::Level(e, _) => Some(End {
                    v: e,
                    closed: false,
                    infinite: None,
                    at: None,
                    src: Srcs::one(EndSrc::Side {
                        at: x.lo,
                        right: is_lo,
                    }),
                }),
                TailEnd::Unknown => None,
            }
        }
    })
}

/// f's limit at an excluded point (enclosed by `x`) the evaluated tree
/// fills in (a removable hole): the tree, equal to f wherever f is
/// defined, is defined and continuous on `n` around it, so the limit is
/// the tree's value there. Not attained (an open end).
fn removable(f: &Fun<'_>, x: Enc, n: Interval, cl: &mut Vec<Claim>) -> Result<Option<End>, Stop> {
    if f.eval == f.expr {
        return Ok(None);
    }
    let over = f.val(n)?;
    if over.is_empty() || over.dec < Dec::Dac || !over.iv.is_bounded() {
        return Ok(None);
    }
    let v = f.val_tight(Interval::new(x.lo.0, x.hi.0))?;
    if v.is_empty() || !v.iv.is_bounded() {
        return Ok(None);
    }
    cl.push(Claim::Removable {
        near: XBox::new(n.lo(), n.hi()),
        at: XBox { a: x.lo, b: x.hi },
        lo: R(v.lo()),
        hi: R(v.hi()),
    });
    Ok(Some(End {
        v: Enc::new(v.lo(), v.hi()),
        closed: false,
        infinite: None,
        at: None,
        src: Srcs::one(EndSrc::Hole(x)),
    }))
}

/// f's value at a point of the domain (enclosed by `x`), attained, with
/// its claim added to `cl`.
fn attained(f: &Fun<'_>, x: Enc, cl: &mut Vec<Claim>) -> Result<Option<End>, Stop> {
    let v = f.val_tight(Interval::new(x.lo.0, x.hi.0))?;
    if v.is_empty() || !v.iv.is_bounded() || v.dec < Dec::Def {
        return Ok(None);
    }
    cl.push(Claim::Value {
        x: XBox { a: x.lo, b: x.hi },
        of: Subject::f(0),
        lo: R(v.lo()),
        hi: R(v.hi()),
    });
    Ok(Some(End {
        v: Enc::new(v.lo(), v.hi()),
        closed: true,
        infinite: None,
        at: Some(x),
        src: Srcs::one(EndSrc::At(x)),
    }))
}

/// The range, and where each of its pieces' finite ends comes from.
pub type RangeRow = (Row<Vec<Piece>>, Vec<[Vec<EndSrc>; 2]>);

pub fn range(
    f: &Fun<'_>,
    dom: &Domain,
    c0: &Cover,
    c1: &Cover,
    boxes: &[IBox],
    scope: &Scope,
    clear: bool,
) -> Result<RangeRow, Stop> {
    let unknown = |why: &str| Ok((Row::unknown(why), Vec::new()));
    if !dom.row.is_certified() || !(scope.whole || scope.period.is_some()) {
        return unknown("the domain is not known");
    }
    let (_, complete) = settle(c1, clear);
    if !complete {
        return unknown("f′'s sign is not decided everywhere");
    }
    let Some(pieces) = monotone_pieces(c1, boxes) else {
        return unknown("f′'s sign is not decided everywhere");
    };
    // Each strictly monotone piece's image, as (low end, high end).
    let mut cl = claims(c1, &Subject::f(1), &[0.0]);
    let mut images: Vec<(End, End)> = Vec::new();
    for (m, lc, hc) in &pieces {
        if m.dir == Dir::Constant {
            // One value, taken at a point of the box.
            let ib = boxes.iter().find(|ib| ib.lo == m.on.lo && ib.hi == m.on.hi);
            let Some(ib) = ib else {
                return unknown("an end of a monotone piece has no proven value");
            };
            let x = match (ib.a.is_finite(), ib.b.is_finite()) {
                (true, true) => ib.a / 2.0 + ib.b / 2.0,
                (true, false) => ib.a + 1.0,
                (false, true) => ib.b - 1.0,
                (false, false) => 0.0,
            };
            let Some(e) = attained(f, Enc::point(x), &mut cl)? else {
                return unknown("an end of a monotone piece has no proven value");
            };
            images.push((e, e));
            continue;
        }
        let (a, b) = (
            end_value(f, m.on.lo, *lc, true, boxes, c0, scope, &mut cl)?,
            end_value(f, m.on.hi, *hc, false, boxes, c0, scope, &mut cl)?,
        );
        let (Some(a), Some(b)) = (a, b) else {
            return unknown("an end of a monotone piece has no proven value");
        };
        images.push(if m.dir == Dir::Increasing {
            (a, b)
        } else {
            (b, a)
        });
    }
    // Isolated points of the domain.
    for p in &dom.pieces {
        if p.lo == p.hi
            && let Bound::At { x, closed: true } = p.lo
        {
            let Some(e) = attained(f, x, &mut cl)? else {
                return unknown("an isolated point's value");
            };
            images.push((e, e));
        }
    }
    let Some(merged) = union(images) else {
        return unknown("two ends of the range are not told apart");
    };
    let to_bound = |e: End, low: bool| -> Bound {
        match e.infinite {
            Some(false) if low => Bound::NegInf,
            Some(true) if !low => Bound::PosInf,
            _ => Bound::At {
                x: e.v,
                closed: e.closed,
            },
        }
    };
    let out: Vec<Piece> = merged
        .iter()
        .map(|&(a, b)| Piece {
            lo: to_bound(a, true),
            hi: to_bound(b, false),
        })
        .collect();
    let srcs: Vec<[Vec<EndSrc>; 2]> = merged
        .iter()
        .map(|(a, b)| [a.src.to_vec(), b.src.to_vec()])
        .collect();
    // f's sign beside the poles.
    if out
        .iter()
        .any(|p| p.lo == Bound::NegInf || p.hi == Bound::PosInf)
    {
        cl.extend(claims(c0, &Subject::f(0), &[0.0]));
    }
    let mut c = Certificate::new(scope.region(true, true).unwrap_or(Region::Line));
    c.extend(cl);
    Ok((
        Row::Certified {
            value: out,
            cert: c,
        },
        srcs,
    ))
}

// ---------------------------------------------------------- parity

/// Even or odd from the simplifier's proof (f(−x) and ±f(x) in one
/// e-class, on a symmetric domain); neither from two interval witnesses
/// (one against each); otherwise unknown.
/// Parity from the tree's structure: `Some(true)` even, `Some(false)` odd.
/// x is odd, a constant even; sums keep a shared parity, products and
/// quotients multiply them, an integer (or odd-root) power of an odd
/// base is odd or even with the exponent, any function of an even
/// argument is even, an odd (even) function of an odd argument is odd
/// (even). Each step maps f(−x) to ±f(x), and defined to defined.
fn structural_parity(e: &crate::ast::Expr) -> Option<bool> {
    use crate::ast::{BinOp, Expr, Func};
    use crate::compile::syntactic_rational;
    if !e.contains_x() {
        return (!e.contains_y()).then_some(true);
    }
    Some(match e {
        Expr::X => false,
        Expr::Neg(a) | Expr::Degrees(a) => structural_parity(a)?,
        Expr::Bin(BinOp::Add | BinOp::Sub, a, b) => {
            let (pa, pb) = (structural_parity(a)?, structural_parity(b)?);
            (pa == pb).then_some(pa)?
        }
        Expr::Bin(BinOp::Mul | BinOp::Div, a, b) => structural_parity(a)? == structural_parity(b)?,
        Expr::Bin(BinOp::Pow, a, b) => {
            let pa = structural_parity(a)?;
            if b.contains_x() {
                (pa && structural_parity(b)?).then_some(true)?
            } else if pa {
                true
            } else {
                match syntactic_rational(b)? {
                    (p, q) if q % 2 == 1 => p % 2 == 0,
                    _ => return None,
                }
            }
        }
        Expr::Call(Func::Root, args) if args.len() == 2 && !args[1].contains_x() => {
            let pa = structural_parity(&args[0])?;
            match syntactic_rational(&args[1])? {
                (n, 1) if pa || n % 2 == 1 => pa,
                _ => return None,
            }
        }
        Expr::Call(f, args) if args.len() == 1 => {
            if structural_parity(&args[0])? {
                true
            } else {
                match f {
                    Func::Sin
                    | Func::Tan
                    | Func::Cot
                    | Func::Csc
                    | Func::Sinh
                    | Func::Tanh
                    | Func::Coth
                    | Func::Csch
                    | Func::Asin
                    | Func::Atan
                    | Func::Acsc
                    | Func::Asinh
                    | Func::Atanh
                    | Func::Acsch
                    | Func::Acoth
                    | Func::Cbrt => false,
                    Func::Cos | Func::Sec | Func::Cosh | Func::Sech | Func::Abs => true,
                    _ => return None,
                }
            }
        }
        Expr::Call(_, args) => {
            // min, max, … of even arguments.
            args.iter()
                .all(|a| structural_parity(a) == Some(true))
                .then_some(true)?
        }
        _ => return None,
    })
}

pub fn parity(f: &Fun<'_>, dom: &Domain) -> Result<Row<Parity>, Stop> {
    use crate::simplify::{Parity as P, prove_parity};
    let proven = |even: bool, by: &str| {
        let word = if even { "even" } else { "odd" };
        let mut c = Certificate::new(Region::Line);
        c.push(Claim::Simplifier {
            fact: format!(
                "f is {word} ({by}): f(−x) = {}f(x), on a domain symmetric about 0",
                if even { "" } else { "−" }
            ),
        });
        Row::Certified {
            value: if even { Parity::Even } else { Parity::Odd },
            cert: c,
        }
    };
    if let Some(s) = f.settings()
        && let Some(p) = prove_parity(&f.expr, &s)
    {
        return Ok(proven(p == P::Even, "simplifier"));
    }
    if let Some(even) = structural_parity(&f.expr) {
        return Ok(proven(even, "its tree is built of even and odd parts"));
    }
    let mut c = Certificate::new(Region::Points);
    let (mut not_even, mut not_odd) = (false, false);
    // Fixed points, and one inside each piece of the domain (an asymmetric
    // domain shows at its own points).
    let mut xs = vec![0.5, 1.0, 1.7, 2.3, 3.1, 4.6, 7.3, 11.9];
    for p in &dom.pieces {
        let (lo, hi) = super::side::piece_box(p);
        let x = match (lo.is_finite(), hi.is_finite()) {
            (true, true) => lo / 2.0 + hi / 2.0,
            (true, false) => lo + 1.0 + lo.abs() / 2.0,
            (false, true) => hi - 1.0 - hi.abs() / 2.0,
            (false, false) => continue,
        };
        if x.is_finite() && x != 0.0 {
            xs.push(x.abs());
        }
    }
    for x in xs {
        let at = |p: f64| f.ser_of(&f.expr, Interval::point(p), 0).map(|s| s[0]);
        let (a, b) = (at(x)?, at(-x)?);
        for (p, v) in [(x, a), (-x, b)] {
            if v.is_empty() {
                c.push(Claim::Undefined { x: XBox::point(p) });
            } else if v.dec >= Dec::Def {
                c.push(Claim::Value {
                    x: XBox::point(p),
                    of: Subject::f(0),
                    lo: R(v.lo()),
                    hi: R(v.hi()),
                });
            }
        }
        let def = |v: &DecInterval| !v.is_empty() && v.dec >= Dec::Def;
        if def(&a) != def(&b) && (a.is_empty() || b.is_empty()) {
            not_even = true;
            not_odd = true;
        }
        if def(&a) && def(&b) {
            if a.hi() < b.lo() || b.hi() < a.lo() {
                not_even = true;
            }
            if a.hi() < -b.hi() || -b.lo() < a.lo() {
                not_odd = true;
            }
        }
        if not_even && not_odd {
            return Ok(Row::Certified {
                value: Parity::Neither,
                cert: c,
            });
        }
    }
    Ok(Row::unknown("parity is proven only by simplification"))
}
