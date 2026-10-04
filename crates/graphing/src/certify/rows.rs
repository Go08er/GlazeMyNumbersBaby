//! The rows, from the domain and the covers of f, f′ and f″.

use super::cert::*;
use super::cover::{Cover, Leaf, claims};
use super::fun::{Fun, Stop, usable};
use super::pole::{bounded_near, end_pole, pole};
use super::side::{Domain, IBox};
use crate::analysis::format::Nice;
use crate::interval::{DecInterval, Interval};

/// Whether every leaf of a cover is decided (an undecided box anywhere,
/// even the last few doubles beside a pole, leaves the row incomplete:
/// a zero or a turn could hide there). Returns no extra claims; kept as a
/// hook for gap policies.
pub fn settle(cover: &Cover, _boxes: &[IBox]) -> (Vec<Claim>, bool) {
    (Vec::new(), cover.complete())
}

fn is_gap(_l: &Leaf, _boxes: &[IBox]) -> bool {
    false
}

/// The region a cover's claims reach.
pub fn region(whole: bool, complete: bool, w: f64) -> Option<Region> {
    match (whole, complete) {
        (true, true) => Some(Region::Line),
        (false, true) => Some(Region::Window { a: R(-w), b: R(w) }),
        _ => None,
    }
}

fn row_of<T>(
    value: T,
    region: Option<Region>,
    has_items: bool,
    claims: Vec<Claim>,
    why: &str,
) -> Row<T> {
    match region {
        Some(Region::Line) => {
            let mut c = Certificate::new(Region::Line);
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

fn enc_of(l: f64, r: f64) -> Enc {
    Enc::new(l, r)
}

// ---------------------------------------------------------- x-intercepts

pub fn zeros(c0: &Cover, boxes: &[IBox], whole: bool, w: f64, extra: &[Claim]) -> Row<Vec<Spot>> {
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
    let (gaps, complete) = settle(c0, boxes);
    let mut cl = claims(c0, &Subject::f(0), &[0.0]);
    cl.extend(gaps);
    cl.extend(extra.iter().cloned());
    let has = !out.is_empty();
    row_of(out, region(whole, complete, w), has, cl, "f's sign is not decided everywhere")
}

// ---------------------------------------------------------- y-intercept

pub fn y_intercept(f: &Fun<'_>) -> Result<Row<Option<Enc>>, Stop> {
    let v = f.val(Interval::point(0.0))?;
    let mut c = Certificate::new(Region::Points);
    if v.is_empty() {
        c.push(Claim::Undefined {
            x: XBox::point(0.0),
        });
        return Ok(Row::Certified { value: None, cert: c });
    }
    if v.dec < crate::interval::Dec::Def || !v.iv.is_bounded() {
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
    /// Not decided.
    Unknown,
}

fn walk(cover: &Cover, ib: &IBox, boxes: &[IBox]) -> Vec<Seg> {
    let mut out = Vec::new();
    for l in cover.leaves.iter().filter(|l| {
        let (a, b) = l.span();
        a >= ib.a && b <= ib.b
    }) {
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
            Leaf::Equal { .. } => out.push(Seg::Flat),
            Leaf::Undefined { .. } => out.push(Seg::Unknown),
            Leaf::Flag { .. } => {
                if !is_gap(l, boxes) {
                    out.push(Seg::Unknown);
                }
            }
        }
    }
    // Merge runs of the same sign.
    out.dedup_by(|y, x| matches!((*x, *y), (Seg::Sign(p), Seg::Sign(q)) if p == q));
    out
}

/// Where f⁽ᵏ⁾ changes sign along a box: (point, from positive to
/// negative?). `None` if a zero's sides aren't both decided.
fn changes(segs: &[Seg]) -> Option<Vec<(Enc, bool)>> {
    let mut out = Vec::new();
    for (i, s) in segs.iter().enumerate() {
        match s {
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
                    _ => return None,
                }
            }
            Seg::Flat | Seg::Unknown => return None,
            Seg::Sign(_) => {}
        }
    }
    Some(out)
}

// ---------------------------------------------------------- extrema

pub fn extrema(
    f: &Fun<'_>,
    c1: &Cover,
    boxes: &[IBox],
    whole: bool,
    w: f64,
) -> Result<Row<Vec<Extremum>>, Stop> {
    let (gaps, mut complete) = settle(c1, boxes);
    let mut out = Vec::new();
    let mut cl = claims(c1, &Subject::f(1), &[0.0]);
    cl.extend(gaps);
    for ib in boxes {
        let segs = walk(c1, ib, boxes);
        let Some(ch) = changes(&segs) else {
            complete = false;
            continue;
        };
        for (x, from_pos) in ch {
            let y = f.val(Interval::new(x.lo.0, x.hi.0))?;
            if y.is_empty() || !y.iv.is_bounded() {
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
                kind: if from_pos {
                    ExtKind::Max
                } else {
                    ExtKind::Min
                },
            });
        }
    }
    let has = !out.is_empty();
    Ok(row_of(
        out,
        region(whole, complete, w),
        has,
        cl,
        "f′ is not decided everywhere",
    ))
}

pub fn inflections(
    f: &Fun<'_>,
    c2: &Cover,
    boxes: &[IBox],
    whole: bool,
    w: f64,
) -> Result<Row<Vec<Inflection>>, Stop> {
    let (gaps, mut complete) = settle(c2, boxes);
    let mut out = Vec::new();
    let mut cl = claims(c2, &Subject::f(2), &[0.0]);
    cl.extend(gaps);
    for ib in boxes {
        let segs = walk(c2, ib, boxes);
        let Some(ch) = changes(&segs) else {
            complete = false;
            continue;
        };
        for (x, _) in ch {
            let y = f.val(Interval::new(x.lo.0, x.hi.0))?;
            if y.is_empty() || !y.iv.is_bounded() {
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
            });
        }
    }
    let has = !out.is_empty();
    Ok(row_of(
        out,
        region(whole, complete, w),
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
        let segs = walk(c1, ib, boxes);
        changes(&segs)?;
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
                Seg::Zero(x) => {
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
                                dir: if d {
                                    Dir::Increasing
                                } else {
                                    Dir::Decreasing
                                },
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
                dir: if d {
                    Dir::Increasing
                } else {
                    Dir::Decreasing
                },
            },
            lo_clip,
            ib.hi_clipped,
        ));
    }
    Some(out)
}

pub fn monotonicity(c1: &Cover, boxes: &[IBox], whole: bool, w: f64) -> Row<Vec<Monotone>> {
    let (gaps, complete) = settle(c1, boxes);
    let Some(pieces) = monotone_pieces(c1, boxes) else {
        return Row::unknown("f′'s sign is not decided everywhere");
    };
    let mut cl = claims(c1, &Subject::f(1), &[0.0]);
    cl.extend(gaps);
    let out: Vec<Monotone> = pieces.into_iter().map(|(m, _, _)| m).collect();
    let has = !out.is_empty();
    row_of(
        out,
        region(whole, complete, w),
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
fn sign_beside(c0: &Cover, ib: &IBox, at_start: bool, boxes: &[IBox]) -> Option<bool> {
    let inside = |l: &&Leaf| {
        let (a, b) = l.span();
        a >= ib.a && b <= ib.b && !is_gap(l, boxes)
    };
    let leaf = if at_start {
        c0.leaves.iter().filter(inside).min_by(|x, y| x.span().0.total_cmp(&y.span().0))
    } else {
        c0.leaves.iter().filter(inside).max_by(|x, y| x.span().1.total_cmp(&y.span().1))
    }?;
    match *leaf {
        Leaf::Band { band, .. } => Some(band == 1),
        _ => None,
    }
}

/// How f behaves at an excluded point between two boxes, with the claim
/// that says so.
fn classify(f: &Fun<'_>, x: Enc, boxes: &[IBox]) -> Result<(super::pole::Near, Option<Claim>), Stop> {
    use super::pole::Near;
    let Some(n) = around(x, boxes) else {
        return Ok((Near::Unknown, None));
    };
    let near = XBox::new(n.lo(), n.hi());
    if pole(f, &f.expr, n)? {
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
            Some(Claim::Value {
                x: near,
                of: Subject::f(0),
                lo: R(v.lo()),
                hi: R(v.hi()),
            }),
        ));
    }
    Ok((Near::Unknown, None))
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
            let prev_same = i > 0
                && matches!(boxes[i - 1].hi, Bound::At { x: y, closed: false } if y == x);
            if !prev_same {
                ends.push((x, true));
            }
        }
    }
    (inner, ends)
}

pub fn vertical(f: &Fun<'_>, dom: &Domain, c0: &Cover, boxes: &[IBox], whole: bool) -> Result<Row<Vec<Spot>>, Stop> {
    use super::pole::Near;
    if !dom.row.is_certified() {
        return Ok(Row::unknown("the domain is not known"));
    }
    if !whole {
        return Ok(Row::unknown("excluded families"));
    }
    // Continuity everywhere inside: no asymptote but at the exclusions.
    let all_cont = c0.leaves.iter().all(|l| match *l {
        Leaf::Band { cont, .. } | Leaf::Equal { cont, .. } => cont,
        Leaf::Flag { .. } => is_gap(l, boxes),
        _ => true,
    }) && c0.stopped.is_none();
    if !all_cont {
        return Ok(Row::unknown("f is not proven continuous inside its domain"));
    }
    let (inner, ends) = excluded_points(boxes);
    let mut out = Vec::new();
    let mut c = Certificate::new(Region::Line);
    c.extend(claims(c0, &Subject::f(0), &[0.0]));
    for x in inner {
        let (near, claim) = classify(f, x, boxes)?;
        match near {
            Near::Pole => out.push(Spot::At(x)),
            Near::Bounded => {}
            Near::Unknown => return Ok(Row::unknown("an excluded point is not classified")),
        }
        c.extend(claim);
    }
    for (x, from_right) in ends {
        // The box on the side of the end the domain is on.
        let cut = |b: Bound| matches!(b, Bound::At { x: y, closed: false } if y == x);
        let Some(ib) = boxes.iter().find(|b| if from_right { cut(b.lo) } else { cut(b.hi) }) else {
            continue;
        };
        let p = if from_right { x.hi.0 } else { x.lo.0 };
        let n = beside(p, ib, from_right);
        let near = XBox::new(n.lo(), n.hi());
        if x.is_point() && end_pole(f, &f.expr, n, p)? {
            out.push(Spot::At(x));
            c.push(Claim::Unbounded {
                near,
                at: XBox { a: x.lo, b: x.hi },
            });
        } else if bounded_near(f, n)? {
            let v = f.val(n)?;
            c.push(Claim::Value {
                x: near,
                of: Subject::f(0),
                lo: R(v.lo()),
                hi: R(v.hi()),
            });
        } else {
            return Ok(Row::unknown("a domain end is not classified"));
        }
    }
    Ok(Row::Certified { value: out, cert: c })
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

/// What f does on a tail: tends to ±∞, tends to a limit (enclosed), or not
/// decided.
#[derive(Clone, Copy, Debug)]
pub enum TailEnd {
    Infinite(bool),
    Level(Enc),
    Unknown,
}

/// How f ends on the right (`right`) or left tail beyond `from`, with the
/// claims that prove it.
pub fn tail_end(f: &Fun<'_>, right: bool, from: f64) -> Result<(TailEnd, Vec<Claim>), Stop> {
    let side = if right { Tail::Right } else { Tail::Left };
    let tail = |m: f64| {
        if right {
            Interval::new(m, f64::INFINITY)
        } else {
            Interval::new(f64::NEG_INFINITY, -m)
        }
    };
    let start = from.abs().max(1.0);
    let s = f.ser(tail(start), 1)?;
    if !(usable(&s, 1) && s[1].ne0()) {
        return Ok((TailEnd::Unknown, Vec::new()));
    }
    let d = s[1];
    let rising = d.gt0();
    let from_r = R(if right { start } else { -start });
    // f′ bounded away from 0 (beyond half its bound): f goes to ±∞.
    let c = if rising { d.lo() / 2.0 } else { d.hi() / 2.0 };
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
    // Strictly monotone: a limit if bounded. Look far out for a narrow
    // enclosure; the limit lies in it.
    let mono = Claim::TailBeyond {
        side,
        from: from_r,
        of: Subject::f(1),
        c: R(0.0),
        above: rising,
    };
    for m in [1e8_f64, 1e16, 1e32, 1e64, 1e150, 1e300] {
        let m = m.max(start);
        let v = f.val(tail(m))?;
        if v.is_empty() || !v.iv.is_bounded() {
            break;
        }
        if narrow(&v) {
            let value = Claim::TailValue {
                side,
                from: R(if right { m } else { -m }),
                lo: R(v.lo()),
                hi: R(v.hi()),
            };
            return Ok((TailEnd::Level(Enc::new(v.lo(), v.hi())), vec![mono, value]));
        }
    }
    Ok((TailEnd::Unknown, Vec::new()))
}

pub fn horizontal(f: &Fun<'_>, dom: &Domain, whole: bool, w: f64) -> Result<Row<Vec<Horizontal>>, Stop> {
    if !dom.row.is_certified() || !whole {
        return Ok(Row::unknown("the domain's tails are not known"));
    }
    let mut out = Vec::new();
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
            TailEnd::Infinite(_) => {}
            TailEnd::Level(y) => out.push(Horizontal {
                side: if right { Tail::Right } else { Tail::Left },
                y,
                looks_like: looks_like(y),
            }),
            TailEnd::Unknown => return Ok(Row::unknown("a tail's limit is not decided")),
        }
        c.extend(claims);
    }
    Ok(Row::Certified { value: out, cert: c })
}

// ---------------------------------------------------------- range

/// One end of a monotone piece's image.
#[derive(Clone, Copy, Debug)]
struct End {
    v: Enc,
    closed: bool,
    infinite: Option<bool>,
    /// Where f takes this value, for an attained end: two ends taken at the
    /// same place are the same value, however wide their enclosures.
    at: Option<Enc>,
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
    w: f64,
    cl: &mut Vec<Claim>,
) -> Result<Option<End>, Stop> {
    if clipped {
        return Ok(None);
    }
    let infinite = |up: bool| End {
        v: Enc::point(if up { f64::INFINITY } else { f64::NEG_INFINITY }),
        closed: false,
        infinite: Some(up),
        at: None,
    };
    Ok(match b {
        Bound::NegInf | Bound::PosInf => {
            let (end, claims) = tail_end(f, b == Bound::PosInf, w)?;
            cl.extend(claims);
            match end {
                TailEnd::Infinite(up) => Some(infinite(up)),
                TailEnd::Level(e) => Some(End {
                    v: e,
                    closed: false,
                    infinite: None,
                    at: None,
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
                Some(n) => pole(f, &f.expr, n)?.then_some(n),
                None => {
                    let p = if is_lo { x.hi.0 } else { x.lo.0 };
                    let n = beside(p, ib, is_lo);
                    (x.is_point() && end_pole(f, &f.expr, n, p)?).then_some(n)
                }
            };
            let Some(n) = n else {
                return Ok(None);
            };
            cl.push(Claim::Unbounded {
                near: XBox::new(n.lo(), n.hi()),
                at: XBox { a: x.lo, b: x.hi },
            });
            sign_beside(c0, ib, is_lo, boxes).map(infinite)
        }
    })
}

/// f's value at a point of the domain (enclosed by `x`), attained, with
/// its claim added to `cl`.
fn attained(f: &Fun<'_>, x: Enc, cl: &mut Vec<Claim>) -> Result<Option<End>, Stop> {
    let v = f.val(Interval::new(x.lo.0, x.hi.0))?;
    if v.is_empty() || !v.iv.is_bounded() || v.dec < crate::interval::Dec::Def {
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
    }))
}

pub fn range(
    f: &Fun<'_>,
    dom: &Domain,
    c0: &Cover,
    c1: &Cover,
    boxes: &[IBox],
    whole: bool,
    w: f64,
) -> Result<Row<Vec<Piece>>, Stop> {
    if !dom.row.is_certified() || !whole {
        return Ok(Row::unknown("the domain is not known"));
    }
    let (_, complete) = settle(c1, boxes);
    if !complete {
        return Ok(Row::unknown("f′'s sign is not decided everywhere"));
    }
    let Some(pieces) = monotone_pieces(c1, boxes) else {
        return Ok(Row::unknown("f′'s sign is not decided everywhere"));
    };
    // Each strictly monotone piece's image, as (low end, high end).
    let mut cl = claims(c1, &Subject::f(1), &[0.0]);
    let mut images: Vec<(End, End)> = Vec::new();
    for (m, lc, hc) in &pieces {
        let (Some(a), Some(b)) = (
            end_value(f, m.on.lo, *lc, true, boxes, c0, w, &mut cl)?,
            end_value(f, m.on.hi, *hc, false, boxes, c0, w, &mut cl)?,
        ) else {
            return Ok(Row::unknown("an end of a monotone piece has no proven value"));
        };
        images.push(if m.dir == Dir::Increasing { (a, b) } else { (b, a) });
    }
    // Isolated points of the domain.
    for p in &dom.pieces {
        if p.lo == p.hi
            && let Bound::At { x, closed: true } = p.lo
        {
            let Some(e) = attained(f, x, &mut cl)? else {
                return Ok(Row::unknown("an isolated point's value"));
            };
            images.push((e, e));
        }
    }
    let Some(merged) = union(images) else {
        return Ok(Row::unknown("two ends of the range are not told apart"));
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
        .into_iter()
        .map(|(a, b)| Piece {
            lo: to_bound(a, true),
            hi: to_bound(b, false),
        })
        .collect();
    // f's sign beside the poles.
    if out.iter().any(|p| p.lo == Bound::NegInf || p.hi == Bound::PosInf) {
        cl.extend(claims(c0, &Subject::f(0), &[0.0]));
    }
    let mut c = Certificate::new(Region::Line);
    c.extend(cl);
    Ok(Row::Certified { value: out, cert: c })
}

// ---------------------------------------------------------- parity

pub fn parity(f: &Fun<'_>) -> Result<Row<Parity>, Stop> {
    let mut c = Certificate::new(Region::Points);
    let (mut not_even, mut not_odd) = (false, false);
    for x in [0.5, 1.0, 1.7, 2.3, 3.1, 4.6, 7.3, 11.9] {
        let (a, b) = (f.val(Interval::point(x))?, f.val(Interval::point(-x))?);
        for (p, v) in [(x, a), (-x, b)] {
            if v.is_empty() {
                c.push(Claim::Undefined { x: XBox::point(p) });
            } else if v.dec >= crate::interval::Dec::Def {
                c.push(Claim::Value {
                    x: XBox::point(p),
                    of: Subject::f(0),
                    lo: R(v.lo()),
                    hi: R(v.hi()),
                });
            }
        }
        let def = |v: &DecInterval| !v.is_empty() && v.dec >= crate::interval::Dec::Def;
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
