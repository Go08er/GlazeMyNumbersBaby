//! The rows, from the domain and the covers of f, f′ and f″.

use super::cert::*;
use super::cover::{Cover, Leaf, claims};
use super::fun::{Fun, Stop, usable};
use super::pole::{bounded_near, end_pole, pole, pole_free};
use super::side::{Domain, IBox};
use crate::analysis::format::Nice;
use crate::interval::{Dec, DecInterval, Interval};

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
    row_of(out, scope.region(complete, true), has, cl, "f's sign is not decided everywhere")
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
            let below = matches!(p.hi, Bound::At { x, closed } if x.hi.0 < 0.0 || (!closed && x == zero));
            let above = matches!(p.lo, Bound::At { x, closed } if x.lo.0 > 0.0 || (!closed && x == zero));
            below || above
        });
        if in_family || outside {
            let mut c = Certificate::new(Region::Points);
            c.extend(cert.claims.iter().cloned());
            return Ok(Row::Certified { value: None, cert: c });
        }
    }
    // The original tree: 0 may be a hole the simplified form fills.
    let v = f.ser_of(&f.expr, Interval::point(0.0), 0)?[0];
    let mut c = Certificate::new(Region::Points);
    if v.is_empty() {
        c.push(Claim::Undefined {
            x: XBox::point(0.0),
        });
        return Ok(Row::Certified { value: None, cert: c });
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
    /// Not decided.
    Unknown,
}

fn walk(cover: &Cover, ib: &IBox) -> Vec<Seg> {
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
    }
    // Merge runs of the same sign, and a zero seen from both sides (its
    // point leaf and the boxes ending there).
    out.dedup_by(|y, x| match (*x, *y) {
        (Seg::Sign(p), Seg::Sign(q)) => p == q,
        (Seg::Zero(p), Seg::Zero(q)) => p == q,
        _ => false,
    });
    out
}

/// Where f⁽ᵏ⁾ changes sign along a box: (point, from positive to
/// negative?), each proven by decided signs on both sides (the leaves tile
/// the box, so neighbours in the walk are neighbours in x); and whether
/// every zero and stretch was decided (no change missed).
fn changes(segs: &[Seg]) -> (Vec<(Enc, bool)>, bool) {
    let mut out = Vec::new();
    let mut complete = true;
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
                    _ => complete = false,
                }
            }
            Seg::Flat | Seg::Unknown => complete = false,
            Seg::Sign(_) => {}
        }
    }
    // ≡ 0 on the whole box: no strict change of sign anywhere in it.
    if segs == [Seg::Flat] {
        complete = true;
    }
    (out, complete)
}

// ---------------------------------------------------------- extrema

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
        let (ch, done) = changes(&segs);
        complete &= done;
        for (x, from_pos) in ch {
            if let Some(p) = scope.period
                && repeats(&out.iter().map(|e: &Extremum| e.x).collect::<Vec<_>>(), &x, &p)
            {
                continue;
            }
            let y = f.val(Interval::new(x.lo.0, x.hi.0))?;
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
                kind: if from_pos {
                    ExtKind::Max
                } else {
                    ExtKind::Min
                },
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
        let (ch, done) = changes(&segs);
        complete &= done;
        for (x, _) in ch {
            if let Some(p) = scope.period
                && repeats(&out.iter().map(|e: &Inflection| e.x).collect::<Vec<_>>(), &x, &p)
            {
                continue;
            }
            let y = f.val(Interval::new(x.lo.0, x.hi.0))?;
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
        if !changes(&segs).1 {
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

pub fn monotonicity(c1: &Cover, boxes: &[IBox], scope: &Scope, clear: bool) -> Row<Vec<Monotone>> {
    let (gaps, complete) = settle(c1, clear);
    let Some(pieces) = monotone_pieces(c1, boxes) else {
        return Row::unknown("f′'s sign is not decided everywhere");
    };
    let mut cl = claims(c1, &Subject::f(1), &[0.0]);
    cl.extend(gaps);
    let out: Vec<Monotone> = pieces.into_iter().map(|(m, _, _)| m).collect();
    let has = !out.is_empty();
    row_of(
        out,
        scope.region(complete, false),
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
        c0.leaves.iter().filter(inside).min_by(|x, y| x.span().0.total_cmp(&y.span().0))
    } else {
        c0.leaves.iter().filter(inside).max_by(|x, y| x.span().1.total_cmp(&y.span().1))
    }?;
    match *leaf {
        Leaf::Band { band, .. } => Some(band == 1),
        Leaf::Touch { above, .. } => Some(above),
        // Before the crossing h is on the side it rises from; after it, on
        // the side it rises to (the crossing is inside, or at the far end).
        Leaf::Cross { a, b, l, r, rising, .. } => {
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
fn classify(f: &Fun<'_>, x: Enc, boxes: &[IBox]) -> Result<(super::pole::Near, Option<Claim>), Stop> {
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
        let c = f.ser(iv, k)?[k];
        if !c.is_empty() && c.ne0() {
            continue;
        }
        // f⁽ᵏ⁾ ≡ 0 (a rational f of lower degree): no change of sign.
        if k > 0 && f.numerators.as_ref().is_some_and(|n| matches!(n[k], crate::ast::Expr::Num(v) if v == 0.0)) {
            continue;
        }
        if k > 0
            && use_derivs
            && let Some(d) = &f.derivs
        {
            let v = f.ser_of(&d[k - 1], iv, 0)?[0];
            if !v.is_empty() && v.ne0() {
                continue;
            }
        }
        // Factor by factor: each away from 0 over the gap, or exactly 0 at
        // one of its doubles and strictly monotone across it (so 0 nowhere
        // between the doubles).
        let tree = match (k, &f.numerators, &f.derivs) {
            (_, Some(n), _) => Some(&n[k]),
            (0, None, _) => Some(&f.eval),
            (_, None, Some(d)) if use_derivs => Some(&d[k - 1]),
            _ => None,
        };
        if let Some(t) = tree
            && factors_clear(f, t, iv)?
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
    for h in super::fun::zero_factors(e) {
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
            let prev_same = i > 0
                && matches!(boxes[i - 1].hi, Bound::At { x: y, closed: false } if y == x);
            if !prev_same {
                ends.push((x, true));
            }
        }
    }
    (inner, ends)
}

pub fn vertical(f: &Fun<'_>, dom: &Domain, c0: &Cover, boxes: &[IBox], scope: &Scope) -> Result<Row<Vec<Spot>>, Stop> {
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
                let ok = !v.is_empty() && (v.dec >= Dec::Dac || (v.dec >= Dec::Def && v.iv.is_bounded()));
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
                let sides = [side_limit(f, x.lo.0, false), side_limit(f, x.lo.0, true)];
                let (Some((l, lc)), Some((r, rc))) = (x.is_point().then_some(()).and(sides[0].clone()), sides[1].clone()) else {
                    return Ok(Row::unknown("an excluded point is not classified"));
                };
                match (l, r) {
                    (TailEnd::Infinite(_), _) | (_, TailEnd::Infinite(_)) => out.push(Spot::At(x)),
                    (TailEnd::Level(..), TailEnd::Level(..)) => {}
                    _ => return Ok(Row::unknown("an excluded point is not classified")),
                }
                c.extend([lc, rc]);
            }
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
        if x.is_point() && end_pole(f, &f.eval, n, p)? {
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
            match x.is_point().then(|| side_limit(f, p, from_right)).flatten() {
                Some((TailEnd::Infinite(_), claim)) => {
                    out.push(Spot::At(x));
                    c.push(claim);
                }
                Some((TailEnd::Level(..), claim)) => c.push(claim),
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


/// What f does on a tail: tends to ±∞, tends to a limit (enclosed, and
/// exactly when the simplifier proved it), or not decided.
#[derive(Clone, Debug)]
pub enum TailEnd {
    Infinite(bool),
    Level(Enc, Option<String>),
    Unknown,
}

/// How f ends on the right (`right`) or left tail beyond `from`, with the
/// claims that prove it: from intervals, and from the simplifier's limit
/// (dominant terms, exact rational functions) when intervals fall short
/// or to pin an enclosed limit down exactly.
pub fn tail_end(f: &Fun<'_>, right: bool, from: f64) -> Result<(TailEnd, Vec<Claim>), Stop> {
    let (end, mut claims) = tail_interval(f, right, from)?;
    if matches!(end, TailEnd::Infinite(_)) {
        return Ok((end, claims));
    }
    let Some((lim, fact)) = simplifier_limit(f, right) else {
        return Ok((end, claims));
    };
    let out = match (end, lim) {
        (TailEnd::Unknown, lim) => lim,
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

/// The simplifier's limit of f at ±∞, as a tail end with its claim.
fn simplifier_limit(f: &Fun<'_>, right: bool) -> Option<(TailEnd, Claim)> {
    let at = if right { "+∞" } else { "−∞" };
    limit_of(f, &f.expr, right, at)
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
            (TailEnd::Level(Enc::new(iv.lo(), iv.hi()), Some(text.clone())), text)
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
/// left, by the simplifier: the limit at +∞ of f(p ± 1/x).
pub fn side_limit(f: &Fun<'_>, p: f64, right: bool) -> Option<(TailEnd, Claim)> {
    use crate::ast::{BinOp, Expr};
    let pe = if p < 0.0 {
        Expr::Neg(Box::new(Expr::Num(-p)))
    } else {
        Expr::Num(p)
    };
    let inv = Expr::bin(BinOp::Div, Expr::Num(1.0), Expr::X);
    let shifted = Expr::bin(if right { BinOp::Add } else { BinOp::Sub }, pe, inv);
    let e = f.expr.map(&|n| matches!(n, Expr::X).then(|| shifted.clone()));
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

pub fn period(f: &Fun<'_>, dom: &Domain, mono: &Row<Vec<Monotone>>, w: f64) -> Result<Row<Option<Enc>>, Stop> {
    let line = super::side::line();
    if let Row::Certified { value: d, cert } = &dom.row
        && d.excluded.is_empty()
        && d.pieces != [line]
    {
        let mut c = Certificate::new(Region::Line);
        c.extend(cert.claims.iter().cloned());
        return Ok(Row::Certified { value: None, cert: c });
    }
    if let Row::Certified { value: m, cert } = mono
        && m.iter().any(|p| p.on.lo == Bound::NegInf || p.on.hi == Bound::PosInf)
    {
        let mut c = Certificate::new(Region::Line);
        c.extend(cert.claims.iter().cloned());
        return Ok(Row::Certified { value: None, cert: c });
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
                return Ok(Row::Certified { value: None, cert: c });
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
    if !(pv.lo() > 0.0) || !dom.row.is_certified() {
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
        if !(v > 0.0) || !(l > 0.0) {
            return Ok(Row::unknown("the period's minimality is not proven"));
        }
        // f′ is (at most) the coefficient bound; round the ratio up.
        (pv.hi() * l / v * (1.0 + 1e-12)).floor()
    } else if dom.pieces == [line] && dom.families.len() == 1 {
        let q = dom.families[0].period;
        if !(q.lo() > 0.0) {
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
    let tail = |m: f64| {
        if right {
            Interval::new(m, f64::INFINITY)
        } else {
            Interval::new(f64::NEG_INFINITY, -m)
        }
    };
    let start = from.abs().max(1.0);
    let from_r = R(if right { start } else { -start });
    // f′ of one sign and moving away from 0 out along the tail (a sign
    // chain through f″, f‴): |f′| ≥ |f′(start)| > 0, so f goes to ±∞.
    let t = super::cover::Target { expr: &f.eval, k: 1 };
    let (a, b) = if right { (start, f64::INFINITY) } else { (f64::NEG_INFINITY, -start) };
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
    let s = f.ser(tail(start), 1)?;
    if !usable(&s, 1) {
        return Ok((TailEnd::Unknown, Vec::new()));
    }
    let d = s[1];
    let rising = if d.ne0() {
        d.gt0()
    } else {
        // f′'s own tree: continuous on the tail with no factor reaching 0
        // there (−csch² for coth), so of the sign it has at the start.
        let Some(t) = f.derivs.as_ref().map(|d| &d[0]) else {
            return Ok((TailEnd::Unknown, Vec::new()));
        };
        let whole = f.ser_of(t, tail(start), 0)?[0];
        let mut nonzero = !whole.is_empty() && whole.dec >= Dec::Dac;
        for h in super::fun::zero_factors(t) {
            let v = f.ser_of(&h, tail(start), 0)?[0];
            nonzero &= !v.is_empty() && v.ne0();
        }
        let at = f.ser_of(t, Interval::point(if right { start } else { -start }), 0)?[0];
        if !(nonzero && !at.is_empty() && at.ne0()) {
            return Ok((TailEnd::Unknown, Vec::new()));
        }
        at.gt0()
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
    // Strictly monotone: a limit if bounded. Every enclosure of f far out
    // holds the limit; keep their intersection.
    let mut claims = vec![Claim::TailBeyond {
        side,
        from: from_r,
        of: Subject::f(1),
        c: R(0.0),
        above: rising,
    }];
    let mut lim: Option<Interval> = None;
    for m in [1e8_f64, 1e16, 1e32, 1e64, 1e150, 1e300] {
        let m = m.max(start);
        let v = f.val(tail(m))?;
        if v.is_empty() || !v.iv.is_bounded() {
            break;
        }
        claims.push(Claim::TailValue {
            side,
            from: R(if right { m } else { -m }),
            lo: R(v.lo()),
            hi: R(v.hi()),
        });
        lim = Some(lim.map_or(v.iv, |l| l.intersect(v.iv)));
    }
    match lim {
        Some(l) if !l.is_empty() && narrow(&DecInterval::new(l)) => {
            Ok((TailEnd::Level(Enc::new(l.lo(), l.hi()), None), claims))
        }
        _ => Ok((TailEnd::Unknown, Vec::new())),
    }
}

pub fn horizontal(f: &Fun<'_>, dom: &Domain, scope: &Scope) -> Result<Row<Vec<Horizontal>>, Stop> {
    if !dom.row.is_certified() {
        return Ok(Row::unknown("the domain's tails are not known"));
    }
    if scope.period.is_some() {
        // A periodic f with a limit at ±∞ would be constant: two values
        // apart show it has none.
        let (a, b) = scope.window;
        let mut vals = Vec::new();
        for i in 0..16 {
            let x = a + (b - a) * (i as f64 + 0.5) / 16.0;
            let v = f.val(Interval::point(x))?;
            if !v.is_empty() && v.dec >= Dec::Def && v.iv.is_bounded() {
                vals.push((x, v));
            }
        }
        let apart = vals.iter().find_map(|(x, v)| {
            vals.iter()
                .find(|(_, u)| u.hi() < v.lo() || v.hi() < u.lo())
                .map(|(y, u)| ((*x, *v), (*y, *u)))
        });
        let Some(((x, v), (y, u))) = apart else {
            return Ok(Row::unknown("a tail's limit is not decided"));
        };
        let mut c = Certificate::new(Region::Line);
        for (p, w) in [(x, v), (y, u)] {
            c.push(Claim::Value {
                x: XBox::point(p),
                of: Subject::f(0),
                lo: R(w.lo()),
                hi: R(w.hi()),
            });
        }
        return Ok(Row::Certified { value: Vec::new(), cert: c });
    }
    if !scope.whole {
        return Ok(Row::unknown("the domain's tails are not known"));
    }
    let w = scope.w;
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
            let Some((end, claim)) = side_limit(f, x.lo.0, is_lo) else {
                return Ok(None);
            };
            cl.push(claim);
            match end {
                TailEnd::Infinite(up) => Some(infinite(up)),
                TailEnd::Level(e, _) => Some(End {
                    v: e,
                    closed: false,
                    infinite: None,
                    at: None,
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
    let v = f.val(Interval::new(x.lo.0, x.hi.0))?;
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
    }))
}

/// f's value at a point of the domain (enclosed by `x`), attained, with
/// its claim added to `cl`.
fn attained(f: &Fun<'_>, x: Enc, cl: &mut Vec<Claim>) -> Result<Option<End>, Stop> {
    let v = f.val(Interval::new(x.lo.0, x.hi.0))?;
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
    }))
}

pub fn range(
    f: &Fun<'_>,
    dom: &Domain,
    c0: &Cover,
    c1: &Cover,
    boxes: &[IBox],
    scope: &Scope,
    clear: bool,
) -> Result<Row<Vec<Piece>>, Stop> {
    if !dom.row.is_certified() || !(scope.whole || scope.period.is_some()) {
        return Ok(Row::unknown("the domain is not known"));
    }
    let (_, complete) = settle(c1, clear);
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
        let (a, b) = (
            end_value(f, m.on.lo, *lc, true, boxes, c0, scope, &mut cl)?,
            end_value(f, m.on.hi, *hc, false, boxes, c0, scope, &mut cl)?,
        );
        if std::env::var_os("CERTIFY_DEBUG").is_some() {
            eprintln!("range piece {:?}: {a:?} .. {b:?}", m.on);
        }
        let (Some(a), Some(b)) = (a, b) else {
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
    let mut c = Certificate::new(scope.region(true, true).unwrap_or(Region::Line));
    c.extend(cl);
    Ok(Row::Certified { value: out, cert: c })
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
