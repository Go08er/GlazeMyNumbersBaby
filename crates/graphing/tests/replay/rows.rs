//! Rows: do a row's claims cover the region it declares, and does its
//! value follow from them?
//!
//! For the rows built on a sign walk (x-intercepts on f, extrema and
//! monotonicity on f′, inflections on f″) the claims are laid out along x
//! as the certifier's walk would: stretches of one strict sign, zeros
//! (exact points or enclosures of a unique crossing), stretches ≡ 0, and
//! holes (gaps, undefined points). Their boxes must cover the part of the
//! region where f is defined, and the zeros (with or without a change of
//! sign) must be the row's items, one to one. The domain is checked
//! directly with the replay's own evaluation; the other rows from the
//! facts their claims state.
//!
//! What a row lists (a zero, an end, a limit, a slope, a period) is an
//! enclosure [lo, hi] of a number, and the number is in it. So a listing
//! *holds* what proves it: all of the box or enclosure a claim proves the
//! number in, or the number itself where it is known exactly (a rational
//! a fact writes, N/D's own limit or line by the replay's algebra). To
//! meet the proof, or to lie inside it, is not enough: a point at a double
//! beside the number does both (1/3 at 0.333…3148, 2⁵³ + 1 at 2⁵³, π at
//! 3.14159…793). Review 18's follow-up went through every row for that;
//! [`Stated`] and `listing_follows` are the rule for limits and range
//! ends, `match_items` for the items of the sign walks, `families_follow`
//! and `one_period` for what repeats, the anchors in `monotonicity` for
//! the pieces' ends, and the ends beside their enclosures in `domain`.

use super::claims::{self, Subj, Tree, split};
use super::growth::At;
use super::iv::{self, Iv};
use super::{
    B, Claim, ClaimResult, Class, Fx, Region, RowCert, RowResult, Side, Subject, Toward, r,
};
use serde_json::Value;

// ------------------------------------------------------------ values

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Enc {
    pub lo: f64,
    pub hi: f64,
}

impl Enc {
    fn contains(&self, o: &Enc) -> bool {
        self.lo <= o.lo && o.hi <= self.hi
    }

    fn meets(&self, o: &Enc) -> bool {
        self.lo <= o.hi && o.lo <= self.hi
    }

    fn mid(&self) -> f64 {
        if self.lo == self.hi {
            self.lo
        } else {
            self.lo / 2.0 + self.hi / 2.0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bound {
    NegInf,
    PosInf,
    At { x: Enc, closed: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub lo: Bound,
    pub hi: Bound,
}

fn enc(v: &Value) -> Result<Enc, String> {
    Ok(Enc {
        lo: r(v.get("lo").ok_or("enc lo")?)?,
        hi: r(v.get("hi").ok_or("enc hi")?)?,
    })
}

fn bound(v: &Value) -> Result<Bound, String> {
    match v.as_str() {
        Some("NegInf") => return Ok(Bound::NegInf),
        Some("PosInf") => return Ok(Bound::PosInf),
        _ => {}
    }
    let at = v.get("At").ok_or_else(|| format!("bad bound {v}"))?;
    Ok(Bound::At {
        x: enc(at.get("x").ok_or("bound x")?)?,
        closed: at
            .get("closed")
            .and_then(Value::as_bool)
            .ok_or("bound closed")?,
    })
}

fn piece(v: &Value) -> Result<Piece, String> {
    Ok(Piece {
        lo: bound(v.get("lo").ok_or("piece lo")?)?,
        hi: bound(v.get("hi").ok_or("piece hi")?)?,
    })
}

fn list(v: &Value) -> Result<&Vec<Value>, String> {
    v.as_array().ok_or_else(|| format!("not a list: {v}"))
}

/// A spot: one point (x0, None) or a family (x0, Some(period)).
fn spot(v: &Value) -> Result<(Enc, Option<Enc>), String> {
    if let Some(a) = v.get("At") {
        return Ok((enc(a)?, None));
    }
    let f = v.get("Every").ok_or_else(|| format!("bad spot {v}"))?;
    Ok((
        enc(f.get("x0").ok_or("x0")?)?,
        Some(enc(f.get("period").ok_or("period")?)?),
    ))
}

/// The reals a piece covers, ends' enclosures included.
fn extent(p: &Piece) -> B {
    let lo = match p.lo {
        Bound::NegInf => f64::NEG_INFINITY,
        Bound::PosInf => f64::INFINITY,
        Bound::At { x, .. } => x.lo,
    };
    let hi = match p.hi {
        Bound::NegInf => f64::NEG_INFINITY,
        Bound::PosInf => f64::INFINITY,
        Bound::At { x, .. } => x.hi,
    };
    (lo, hi)
}

// ------------------------------------------------------------ coverage

/// The first stretch of `target` no box covers (closed boxes; touching
/// ends join).
fn uncovered(target: B, boxes: &[B]) -> Option<B> {
    let (ta, tb) = target;
    if ta > tb {
        return None;
    }
    let mut bs: Vec<B> = boxes
        .iter()
        .copied()
        .filter(|b| b.0 <= b.1 && b.1 >= ta && b.0 <= tb)
        .collect();
    bs.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Everything in [ta, reach] is covered.
    let mut reach: Option<f64> = None;
    for b in bs {
        match reach {
            None if b.0 > ta => return Some((ta, b.0)),
            None => reach = Some(b.1),
            Some(r) if b.0 > r => return Some((r, b.0)),
            Some(r) => reach = Some(r.max(b.1)),
        }
        if reach.is_some_and(|r| r >= tb) {
            return None;
        }
    }
    match reach {
        None => Some((ta, tb)),
        Some(r) if r < tb => Some((r, tb)),
        _ => None,
    }
}

fn region_box(rg: &Region) -> Option<B> {
    match rg {
        Region::Line => Some((f64::NEG_INFINITY, f64::INFINITY)),
        Region::Window(a, b) | Region::Period(a, b) => Some((*a, *b)),
        Region::Points => None,
    }
}

/// Where the row must be decided: the region within the domain's pieces
/// (from the domain row, which [`domain`] checks on its own).
fn targets(rg: &Region, all: &[RowCert]) -> Vec<B> {
    let Some((a, b)) = region_box(rg) else {
        return Vec::new();
    };
    let pieces: Vec<Piece> = all
        .iter()
        .find(|r| r.name == "domain" && r.status == "Certified")
        .and_then(|r| r.value.get("pieces"))
        .and_then(|p| p.as_array())
        .map(|ps| ps.iter().filter_map(|p| piece(p).ok()).collect())
        .unwrap_or_else(|| {
            vec![Piece {
                lo: Bound::NegInf,
                hi: Bound::PosInf,
            }]
        });
    pieces
        .iter()
        .map(extent)
        .map(|(lo, hi)| (lo.max(a), hi.min(b)))
        .filter(|(lo, hi)| lo <= hi)
        .collect()
}

// ------------------------------------------------------------ sign walks

/// A zero of f⁽ᵏ⁾ (enclosed), with the signs right beside it (None: a
/// hole, a region end or nothing decided there).
#[derive(Clone, Debug)]
struct Zero {
    at: Enc,
    left: Option<bool>,
    right: Option<bool>,
    /// A kink of f (a Kink claim), not a zero: f⁽ᵏ⁾ isn't defined there.
    kink: bool,
}

#[derive(Clone, Debug, Default)]
struct Walk {
    /// (a, b, sign) stretches; a stretch beside a zero ends at its
    /// enclosure.
    signs: Vec<(f64, f64, bool)>,
    zeros: Vec<Zero>,
    flat: Vec<B>,
    holes: Vec<B>,
    /// Every box a claim decides (for coverage).
    boxes: Vec<B>,
    /// The gaps among them (`Gap` claims), which decide nothing.
    gaps: Vec<B>,
    /// The boxes of every other claim: what is decided.
    solid: Vec<B>,
    problems: Vec<String>,
}

/// f⁽ᵏ⁾'s sign at a double, from the replay's own evaluation (on the
/// certifier's tree only where f's own is shown defined).
fn sign_at(fx: &Fx, k: usize, x: f64) -> Option<bool> {
    for tree in [Tree::Orig, Tree::Eval] {
        if tree == Tree::Eval && !claims::orig_defined(fx, (x, x)) {
            break;
        }
        let Some(s) = Subj::new(fx, &Subject::F(k), tree) else {
            continue;
        };
        for p in [160, 640] {
            iv::set_prec(p);
            let (v, _) = s.at(x);
            if v.gt(0.0) || v.lt(0.0) {
                iv::set_prec(160);
                return Some(v.gt(0.0));
            }
        }
        iv::set_prec(160);
    }
    None
}

fn tail(side: Side, from: f64) -> B {
    match side {
        Side::Right => (from, f64::INFINITY),
        Side::Left => (f64::NEG_INFINITY, from),
    }
}

fn walk(fx: &Fx, claims: &[Claim], k: usize) -> Walk {
    let mut w = Walk::default();
    let of = Subject::F(k);
    // Unique crossings: the narrowest box of each.
    let mut crossings: Vec<B> = Vec::new();
    for c in claims {
        match c {
            Claim::Beyond { x, of: o, c, above }
            | Claim::NoCross { x, of: o, c, above }
            | Claim::Factors { x, of: o, c, above }
                if *o == of && *c == 0.0 =>
            {
                w.signs.push((x.0, x.1, *above));
                w.boxes.push(*x);
            }
            Claim::TailBeyond {
                side,
                from,
                of: o,
                c,
                above,
            }
            | Claim::TailChain {
                side,
                from,
                of: o,
                c,
                above,
                ..
            } if *o == of && *c == 0.0 => {
                let t = tail(*side, *from);
                w.signs.push((t.0, t.1, *above));
                w.boxes.push(t);
            }
            Claim::Value { x, of: o, lo, hi } if *o == of => {
                if *lo == 0.0 && *hi == 0.0 {
                    if x.0 == x.1 {
                        w.zeros.push(Zero {
                            at: Enc { lo: x.0, hi: x.0 },
                            left: None,
                            right: None,
                            kink: false,
                        });
                    } else {
                        w.flat.push(*x);
                    }
                    w.boxes.push(*x);
                } else if *lo > 0.0 || *hi < 0.0 {
                    w.signs.push((x.0, x.1, *lo > 0.0));
                    w.boxes.push(*x);
                }
            }
            Claim::OneCross { x, of: o, c } if *o == of && *c == 0.0 => {
                crossings.push(*x);
                w.boxes.push(*x);
            }
            Claim::ExactAt { x, of: o, c, at } if *o == of && *c == 0.0 => {
                w.boxes.push(*x);
                let before = if x.0 < *at { sign_at(fx, k, x.0) } else { None };
                let after = if *at < x.1 { sign_at(fx, k, x.1) } else { None };
                if x.0 < *at {
                    match before {
                        Some(s) => w.signs.push((x.0, *at, s)),
                        None => w
                            .problems
                            .push(format!("the sign before the zero at {at:e} is not decided")),
                    }
                }
                if *at < x.1 {
                    match after {
                        Some(s) => w.signs.push((*at, x.1, s)),
                        None => w
                            .problems
                            .push(format!("the sign after the zero at {at:e} is not decided")),
                    }
                }
                w.zeros.push(Zero {
                    at: Enc { lo: *at, hi: *at },
                    left: None,
                    right: None,
                    kink: false,
                });
            }
            Claim::Touch {
                x,
                of: o,
                c,
                at,
                above,
                ..
            } if *o == of && *c == 0.0 => {
                w.boxes.push(*x);
                if x.0 == *at {
                    w.signs.push((*at, x.1, *above));
                } else {
                    w.signs.push((x.0, *at, *above));
                }
                w.zeros.push(Zero {
                    at: Enc { lo: *at, hi: *at },
                    left: None,
                    right: None,
                    kink: false,
                });
            }
            Claim::Undefined(x) | Claim::Gap(x) => {
                w.holes.push(*x);
                w.boxes.push(*x);
                if matches!(c, Claim::Gap(_)) {
                    w.gaps.push(*x);
                }
            }
            // f′, f″ undefined at a kink: a point the signs either side
            // decide (for f, its values are claimed elsewhere).
            // A kink placed exactly: f′'s strict sign on each side (shown
            // on the one-sided forms) up to the point itself.
            Claim::KinkAt { x, at, left, right } if k == 1 => {
                w.signs.push((x.0, *at, *left));
                w.signs.push((*at, x.1, *right));
                w.zeros.push(Zero {
                    at: Enc { lo: *at, hi: *at },
                    left: Some(*left),
                    right: Some(*right),
                    kink: true,
                });
                w.boxes.push(*x);
            }
            // What f⁽ᵏ⁾ does on each side of a kink (its claim checked so):
            // a strict sign wherever it exists is a stretch of that sign (f
            // continuous, so strictly monotone there for f′), 0 a flat
            // stretch. Placed at a double: the two stretches meet there,
            // a zero of the walk; a change of sign of f″ across it is an
            // inflection only when f′ is continuous there (`joined`), else
            // a corner, which no complete row of inflections gets past.
            // Not placed: one sign either side is one stretch over the box;
            // two are a zero enclosed by it (a turn somewhere inside).
            Claim::KinkSides {
                x,
                at,
                order,
                left,
                right,
                joined,
            } if *order == k => {
                use super::KinkSign;
                let strict = |s: &KinkSign| match s {
                    KinkSign::Pos => Some(true),
                    KinkSign::Neg => Some(false),
                    KinkSign::Zero => None,
                };
                match at {
                    Some(p) => {
                        for (a, b, side) in [(x.0, *p, left), (*p, x.1, right)] {
                            match strict(side) {
                                Some(pos) => w.signs.push((a, b, pos)),
                                None => w.flat.push((a, b)),
                            }
                        }
                        w.zeros.push(Zero {
                            at: Enc { lo: *p, hi: *p },
                            left: strict(left),
                            right: strict(right),
                            kink: !(k == 2 && *joined),
                        });
                        w.boxes.push(*x);
                    }
                    None => match (strict(left), strict(right)) {
                        (Some(a), Some(b)) if a == b => {
                            w.signs.push((x.0, x.1, a));
                            w.boxes.push(*x);
                        }
                        (Some(a), Some(b)) => {
                            w.zeros.push(Zero {
                                at: Enc { lo: x.0, hi: x.1 },
                                left: Some(a),
                                right: Some(b),
                                kink: true,
                            });
                            w.boxes.push(*x);
                        }
                        (None, None) => {
                            w.flat.push(*x);
                            w.boxes.push(*x);
                        }
                        // (0 on one side, a sign on the other, the point
                        // unknown: nothing to place. The box stays
                        // undecided.)
                        _ => {}
                    },
                }
            }
            Claim::Kink(x) if k > 0 => {
                // Continuity on a box says nothing about derivative events
                // inside it. Only a placed, one-sided kink or the original
                // derivative's own sign/flat enclosure decides this box.
                if k == 1
                    && claims
                        .iter()
                        .any(|c| matches!(c, Claim::KinkAt { x: at, .. } if at == x))
                {
                    continue;
                }
                let Some(subject) = Subj::new(fx, &of, Tree::Orig) else {
                    continue;
                };
                let (v, valid) = subject.coeffs(x.0, x.1, 0);
                if valid && v[0].ne0() {
                    w.signs.push((x.0, x.1, v[0].gt(0.0)));
                    w.boxes.push(*x);
                } else if valid && v[0].is_exactly(0.0) {
                    w.flat.push(*x);
                    w.boxes.push(*x);
                }
            }
            _ => {}
        }
    }
    // (What the claims other than gaps decide: every box but one of each
    // gap's.)
    let mut gaps_left = w.gaps.clone();
    for b in &w.boxes {
        match gaps_left.iter().position(|g| g == b) {
            Some(i) => {
                gaps_left.swap_remove(i);
            }
            None => w.solid.push(*b),
        }
    }
    // Each crossing: its innermost box is the zero, the outer ones carry
    // the signs on either side of it.
    crossings.sort_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)));
    let mut inner: Vec<B> = Vec::new();
    for x in &crossings {
        if !inner.iter().any(|i| x.0 <= i.0 && i.1 <= x.1) {
            inner.push(*x);
        }
    }
    for i in &inner {
        let outer = crossings
            .iter()
            .filter(|x| x.0 <= i.0 && i.1 <= x.1)
            .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
            .copied()
            .unwrap_or(*i);
        match (sign_at(fx, k, outer.0), sign_at(fx, k, outer.1)) {
            (Some(l), Some(r)) if l != r => {
                if outer.0 < i.0 {
                    w.signs.push((outer.0, i.0, l));
                }
                if i.1 < outer.1 {
                    w.signs.push((i.1, outer.1, r));
                }
                w.zeros.push(Zero {
                    at: Enc { lo: i.0, hi: i.1 },
                    // Strictly monotone through the crossing.
                    left: Some(l),
                    right: Some(r),
                    kink: false,
                });
            }
            // (Not decided is not "the same sign": said apart.)
            (Some(_), Some(_)) => w.problems.push(format!(
                "a crossing in [{:e}, {:e}] with one sign at both its ends",
                outer.0, outer.1
            )),
            _ => w.problems.push(format!(
                "a crossing in [{:e}, {:e}] whose signs at its ends are not decided",
                outer.0, outer.1
            )),
        }
    }
    // One zero per place (a point leaf, the box ending there, ...).
    w.zeros.sort_by(|a, b| a.at.lo.total_cmp(&b.at.lo));
    let mut merged: Vec<Zero> = Vec::new();
    for z in w.zeros.drain(..) {
        match merged.last_mut() {
            Some(m)
                if m.at.meets(&z.at)
                    && (m.at.lo == m.at.hi || z.at.lo == z.at.hi || m.at == z.at) =>
            {
                // Keep the narrower enclosure, and the signs known.
                if (z.at.hi - z.at.lo) < (m.at.hi - m.at.lo) {
                    m.at = z.at;
                }
                m.left = m.left.or(z.left);
                m.right = m.right.or(z.right);
                m.kink |= z.kink;
            }
            _ => merged.push(z),
        }
    }
    // The signs beside each zero: a stretch ending at its enclosure (or
    // running into it) on the left, one starting at it on the right; a
    // hole there leaves the side undecided.
    for z in merged.iter_mut() {
        if z.left.is_none() {
            let s: Vec<bool> = w
                .signs
                .iter()
                .filter(|(a, b, _)| *a < z.at.lo && *b >= z.at.lo && *b <= z.at.hi.max(z.at.lo))
                .map(|(_, _, s)| *s)
                .collect();
            if !s.is_empty() && s.iter().all(|v| *v == s[0]) {
                z.left = Some(s[0]);
            }
        }
        if z.right.is_none() {
            let s: Vec<bool> = w
                .signs
                .iter()
                .filter(|(a, b, _)| *b > z.at.hi && *a <= z.at.hi && *a >= z.at.lo.min(z.at.hi))
                .map(|(_, _, s)| *s)
                .collect();
            if !s.is_empty() && s.iter().all(|v| *v == s[0]) {
                z.right = Some(s[0]);
            }
        }
    }
    w.zeros = merged;
    w
}

/// Changes of sign: (where, from positive, at a kink).
fn changes(w: &Walk) -> Vec<(Enc, bool, bool)> {
    w.zeros
        .iter()
        .filter_map(|z| match (z.left, z.right) {
            (Some(l), Some(r)) if l != r => Some((z.at, l, z.kink)),
            _ => None,
        })
        .collect()
}

/// A row on f⁽ᵏ⁾ that lists its features (zeros, turns, monotone pieces,
/// the range) is complete inside a gap only if f⁽ᵏ⁾ has no zero there: a
/// `GapClear` claim for it (checked with the claims). A complete row with
/// a gap that has none is a problem; a partial one is noted.
pub const GAPS_NOT_IN_CERT: &str = "clearness is not in the certificate";

fn holes(rc: &RowCert, k: usize, complete: bool, out: &mut RowResult) {
    let cleared = |g: &B| {
        rc.claims.iter().any(|c| {
            matches!(c, Claim::GapClear { x, of: Subject::F(j) }
                if *j == k && x.0 <= g.0 && g.1 <= x.1)
        })
    };
    let open: Vec<B> = rc
        .claims
        .iter()
        .filter_map(|c| match c {
            Claim::Gap(x) if !cleared(x) => Some(*x),
            _ => None,
        })
        .collect();
    let Some(first) = open.first() else { return };
    let msg = format!(
        "{} gaps without f{} free of zeros in them ({GAPS_NOT_IN_CERT}; first [{:e}, {:e}])",
        open.len(),
        ["", "′", "″"][k],
        first.0,
        first.1
    );
    if complete {
        out.problems.push(format!("the row is complete, yet {msg}"));
    } else {
        out.notes.push(msg);
    }
}

/// Coverage of the row's region by its walk's boxes.
fn coverage(rc: &RowCert, all: &[RowCert], boxes: &[B], out: &mut RowResult) {
    let Some(rg) = &rc.region else { return };
    for t in targets(rg, all) {
        if let Some((a, b)) = uncovered(t, boxes) {
            out.problems.push(format!(
                "its claims leave [{a:e}, {b:e}] of the region {rg:?} undecided"
            ));
            return;
        }
    }
}

/// f has no value at the double x (shown by the replay's own evaluation).
fn no_value(fx: &Fx, x: f64) -> bool {
    iv::set_prec(160);
    fx.series(&fx.f, x, x, 0)[0].empty
}

/// A gap decides nothing and stands for the reals strictly inside it
/// (`claims::gap_clear` clears those): each of its two ends is a double
/// some other claim of the row decides (a sign, a zero, a kink, f
/// undefined there) or one strictly inside another gap, unless there is
/// nothing to decide at it: f has no value there, or it lies outside the
/// row's region, or (for f′ and f″) it is an end of the region, where no
/// derivative is asked. Counted as covered like any box, two gaps meeting
/// at a double left it out of the row: x²'s f′ "had no zero" and x² "no
/// extremum", 0 between the gaps (−5·10⁻³²⁴, 0) and (0, 5·10⁻³²⁴) (the
/// third pass's audit).
fn gap_ends(fx: &Fx, rc: &RowCert, all: &[RowCert], w: &Walk, k: usize, out: &mut RowResult) {
    let Some(rg) = &rc.region else { return };
    let ts = targets(rg, all);
    for g in &w.gaps {
        for e in [g.0, g.1] {
            // (Or strictly inside another gap, a real that one stands
            // for: the certifier's gaps about one excluded point nest.)
            let decided = w.solid.iter().any(|b| b.0 <= e && e <= b.1)
                || w.gaps.iter().any(|h| h.0 < e && e < h.1);
            let asked = ts
                .iter()
                .any(|t| (t.0 < e && e < t.1) || (k == 0 && (t.0 == e || t.1 == e)));
            if decided || !asked || !e.is_finite() || no_value(fx, e) {
                continue;
            }
            out.problems.push(format!(
                "no claim decides f{} at {e:e}, an end of the gap [{:e}, {:e}]",
                ["", "′", "″"][k],
                g.0,
                g.1
            ));
            return;
        }
    }
}

/// In a complete row on f′ or f″, each zero's two sides are decided: a
/// zero with a side no claim signs (a gap beside it) is no "no change of
/// sign". Nothing is asked of a side outside the row's region, where f has
/// no value, or where f⁽ᵏ⁾ ≡ 0 on a stretch. (x²'s f′, its zero at 0
/// between two gaps, changed sign nowhere: the third pass's audit.)
fn sides_decided(fx: &Fx, rc: &RowCert, all: &[RowCert], w: &Walk, k: usize, out: &mut RowResult) {
    let Some(rg) = &rc.region else { return };
    if !complete(rc) {
        return;
    }
    let ts = targets(rg, all);
    for z in &w.zeros {
        let sides = [
            ("before", z.left, z.at.lo.next_down()),
            ("after", z.right, z.at.hi.next_up()),
        ];
        for (side, sign, beside) in sides {
            let asked = ts.iter().any(|t| t.0 <= beside && beside <= t.1);
            let flat = w.flat.iter().any(|b| b.0 <= beside && beside <= b.1);
            if sign.is_some() || !asked || flat || no_value(fx, beside) {
                continue;
            }
            out.problems.push(format!(
                "f{}'s sign {side} its zero at {:e} is not decided, yet the row is complete",
                ["", "′", "″"][k],
                z.at.mid()
            ));
            return;
        }
    }
}

/// Does `d` (enclosed) meet the members x₀ + k·p of a family, for x₀ and
/// p anywhere in their enclosures and k the whole number nearest (or
/// either beside it)? Exactly, in rationals: no tolerance but the
/// enclosures' own widths.
fn meets_member(x0: &Enc, p: &Enc, d: &Enc) -> bool {
    use rug::{Integer, Rational};
    let q = |v: f64| Rational::from_f64(v);
    let (Some(x0lo), Some(x0hi), Some(plo), Some(phi), Some(dlo), Some(dhi)) =
        (q(x0.lo), q(x0.hi), q(p.lo), q(p.hi), q(d.lo), q(d.hi))
    else {
        return false;
    };
    if plo <= 0 {
        return false;
    }
    // k nearest: (d − x₀)/p at the middles, rounded.
    let guess =
        (Rational::from(&dlo + &dhi) - Rational::from(&x0lo + &x0hi)) / Rational::from(&plo + &phi);
    let k0 = guess.round().numer().clone();
    [Integer::from(&k0 - 1), k0.clone(), Integer::from(&k0 + 1)]
        .into_iter()
        .any(|k| {
            let k = Rational::from(k);
            // x₀ + k·p over the enclosures: k·p's ends swap for k < 0.
            let (kp_lo, kp_hi) = if k >= 0 {
                (Rational::from(&k * &plo), Rational::from(&k * &phi))
            } else {
                (Rational::from(&k * &phi), Rational::from(&k * &plo))
            };
            let lo = Rational::from(&x0lo + &kp_lo);
            let hi = Rational::from(&x0hi + &kp_hi);
            lo <= dhi && dlo <= hi
        })
}

/// Does `x` hold the enclosure `a` moved on k periods, a + k·p for every
/// a and p in their enclosures? Exactly, in rationals.
fn holds_shifted(x: &Enc, a: &Enc, p: &Enc, k: i32) -> bool {
    use rug::Rational;
    let q = Rational::from_f64;
    let (Some(alo), Some(ahi), Some(plo), Some(phi)) = (q(a.lo), q(a.hi), q(p.lo), q(p.hi)) else {
        return false;
    };
    let k = Rational::from(k);
    // k·p's ends swap for k < 0.
    let (kp_lo, kp_hi) = if k >= 0 {
        (Rational::from(&k * &plo), Rational::from(&k * &phi))
    } else {
        (Rational::from(&k * &phi), Rational::from(&k * &plo))
    };
    alo + kp_lo >= x.lo && ahi + kp_hi <= x.hi
}

/// Is `x` (enclosed) a repeat of one of `xs` by a whole number of periods?
fn repeat_of(xs: &[Enc], x: &Enc, period: &Enc) -> bool {
    xs.iter().any(|e| meets_member(e, period, x))
}

/// The derived items against the listed ones: every listed item is a
/// derived one; with `complete`, every derived one is listed (over one
/// period of a periodic row, up to repeats).
fn match_items(
    what: &str,
    derived: &[Enc],
    listed: &[(Enc, Option<Enc>)],
    complete: bool,
    out: &mut RowResult,
) {
    for (x, _) in listed {
        if !derived.iter().any(|d| x.contains(d)) {
            out.problems.push(format!(
                "{what} at [{:e}, {:e}] listed but not proven by the claims",
                x.lo, x.hi
            ));
        }
    }
    if !complete {
        return;
    }
    let xs: Vec<Enc> = listed.iter().map(|(x, _)| *x).collect();
    for d in derived {
        let listed_here = listed.iter().any(|(x, _)| x.contains(d));
        let repeat = listed
            .iter()
            .any(|(_, p)| p.is_some_and(|p| repeat_of(&xs, d, &p)));
        if !listed_here && !repeat {
            out.problems.push(format!(
                "{what} at [{:e}, {:e}] proven by the claims but not listed",
                d.lo, d.hi
            ));
        }
    }
}

fn complete(rc: &RowCert) -> bool {
    matches!(
        rc.region,
        Some(Region::Line | Region::Window(..) | Region::Period(..))
    )
}

// ------------------------------------------------------------ rows

/// A row's status and scope are part of what it says, and each kind of row
/// says it one way:
///
/// * a list (the zeros, the turns, the inflections, the monotone pieces)
///   is **Certified** when it is all of them: over the line, or over one
///   period of a periodic f (and so over the line); **Partial** when it is
///   all of them on a window only, or some of them (points);
/// * the vertical asymptotes and the range are Certified, over the line or
///   one period: a part of either is not a row;
/// * the domain, the period and the horizontal and oblique asymptotes are
///   statements about the whole line, Certified over it;
/// * the y-intercept is a value at one point; so is "neither even nor
///   odd" (two witnesses), where "even" and "odd" are about the line.
///
/// Anything else says what its scope can't bear: "all of f's zeros"
/// (Certified) over points, where nothing shows there are no others, was
/// let through for every list; a y-intercept, a parity or a period under
/// any scope at all. (Round 19's mutation tester.) Why the row is off its
/// form, or None.
fn off_form(rc: &RowCert) -> Option<String> {
    let certified = rc.status == "Certified";
    let whole = matches!(rc.region, Some(Region::Line | Region::Period(..)));
    let line = matches!(rc.region, Some(Region::Line));
    let points = matches!(rc.region, Some(Region::Points));
    let ok = match rc.name.as_str() {
        "x_intercepts" | "extrema" | "inflections" | "monotonicity" => certified == whole,
        "vertical" | "range" => certified && whole,
        "domain" | "period" | "horizontal" | "oblique" => certified && line,
        "y_intercept" => certified && points,
        "parity" => {
            certified
                && if rc.value.as_str() == Some("Neither") {
                    points
                } else {
                    line
                }
        }
        _ => true,
    };
    (!ok).then(|| {
        let scope = match rc.region {
            Some(Region::Line) => "the line",
            Some(Region::Period(..)) => "one period",
            Some(Region::Window(..)) => "a window",
            Some(Region::Points) => "points",
            None => "no scope",
        };
        format!(
            "a {} row {} over {scope}: its status and scope say what this kind of row can't",
            rc.name, rc.status
        )
    })
}

pub fn check(fx: &Fx, rc: &RowCert, all: &[RowCert], results: &[ClaimResult]) -> RowResult {
    let mut out = RowResult {
        name: rc.name.clone(),
        status: rc.status.clone(),
        problems: Vec::new(),
        notes: Vec::new(),
    };
    if rc.status == "Unknown" {
        return out;
    }
    // The language refuses f outright (° outside degrees mode): no row of
    // it is certified, whatever its claims (review 17, R17-M-02).
    if let Some(why) = &fx.refused {
        out.problems.push(format!(
            "{why}: the language refuses f, so no row of it is certified"
        ));
        return out;
    }
    if let Some(why) = off_form(rc) {
        out.problems.push(why);
    }
    // The claims' own outcomes (looked up by key: a row has thousands).
    let outcomes: std::collections::HashMap<String, &ClaimResult> = results
        .iter()
        .map(|r| (super::claim_key(&r.claim), r))
        .collect();
    let mut weak = 0;
    let mut open = 0;
    let mut refuted = 0;
    for c in &rc.claims {
        if let Some(res) = outcomes.get(&super::claim_key(c)) {
            match res.outcome.class {
                Class::Weak => weak += 1,
                Class::Unconfirmed | Class::Unsupported => open += 1,
                Class::Refuted => refuted += 1,
                Class::Strong => {}
            }
        }
    }
    if weak > 0 {
        out.notes
            .push(format!("{weak} claims are weak (rest on the simplifier)"));
    }
    if open > 0 {
        out.notes.push(format!("{open} claims are unconfirmed"));
    }
    if refuted > 0 {
        out.problems
            .push(format!("it rests on {refuted} refuted claims"));
    }
    if let Err(e) = in_domain(rc, all, results, &mut out) {
        out.problems.push(format!("unreadable value: {e}"));
    }
    one_period(rc, &mut out);
    let r = match rc.name.as_str() {
        "domain" => domain(fx, rc, &mut out),
        "x_intercepts" => zeros_row(fx, rc, all, &mut out),
        "extrema" => turns(fx, rc, all, 1, &mut out),
        "inflections" => turns(fx, rc, all, 2, &mut out),
        "monotonicity" => monotonicity(fx, rc, all, &mut out),
        "y_intercept" => y_intercept(fx, rc, &mut out),
        "parity" => parity(fx, rc, all, results, &mut out),
        "period" => period(fx, rc, all, &mut out),
        "vertical" => vertical(fx, rc, all, &mut out),
        "horizontal" => horizontal(fx, rc, all, &mut out),
        "oblique" => oblique(fx, rc, all, &mut out),
        "range" => range(fx, rc, all, &mut out),
        _ => Ok(()),
    };
    if let Err(e) = r {
        out.problems.push(format!("unreadable value: {e}"));
    }
    out
}

/// The period a row rests on: its fact "f(x + P) = f(x) …" (checked as a
/// claim), P enclosed: the bracket the fact puts it in, or the q·πᵏ it
/// writes, at 160 bits, rounded out to the doubles about it.
fn row_period(rc: &RowCert) -> Option<Enc> {
    rc.claims.iter().find_map(|c| {
        let Claim::Simplifier(f) = c else {
            return None;
        };
        let (p, rest) = f.strip_prefix("f(x + ")?.split_once(") = f(x)")?;
        if p == "P" {
            let (a, b) = rest
                .split_once("P ∈ [")?
                .1
                .split_once(']')?
                .0
                .split_once(", ")?;
            return Some(Enc {
                lo: a.trim().parse().ok()?,
                hi: b.trim().parse().ok()?,
            });
        }
        iv::set_prec(160);
        let (lo, hi) = claims::outward(&claims::pi_q(p)?);
        Some(Enc { lo, hi })
    })
}

/// A row decided over one period of f (`Region::Period`) speaks for the
/// whole line only if f has that period, by a fact of the row, and the
/// window is as wide as it: a window cut short leaves out what repeats
/// beyond it (sin's zeros over a window ending at 1, the one at π
/// dropped, were "every zero": review 18's follow-up).
fn one_period(rc: &RowCert, out: &mut RowResult) {
    let Some(Region::Period(a, b)) = rc.region else {
        return;
    };
    let Some(p) = row_period(rc) else {
        out.problems
            .push("the row covers one period, with no fact that f has one".into());
        return;
    };
    let q = rug::Rational::from_f64;
    let wide = match (q(a), q(b), q(p.hi)) {
        (Some(a), Some(b), Some(p)) => p > 0 && b - a >= p,
        _ => false,
    };
    if !wide {
        out.problems.push(format!(
            "the window [{a:e}, {b:e}] is not as wide as the period, in [{:e}, {:e}]",
            p.lo, p.hi
        ));
    }
}

/// A row complete over one period speaks for the line by f's period:
/// each thing it lists repeats with f, so each is listed as a family. One
/// listed alone says there is no other like it, which is false of every
/// repeat (sin's zero at 0 "alone", in a row complete over a period,
/// passed: round 19's mutation tester).
fn repeats_listed(what: &str, rc: &RowCert, listed: &[(Enc, Option<Enc>)], out: &mut RowResult) {
    if !matches!(rc.region, Some(Region::Period(..))) || rc.status != "Certified" {
        return;
    }
    if let Some((x, _)) = listed.iter().find(|(_, p)| p.is_none()) {
        out.problems.push(format!(
            "{what} at {:e} listed alone, in a row complete over one period of f",
            x.mid()
        ));
    }
}

/// The families a row lists (each item "every `period`") repeat by the
/// period of the row's fact: each listed period holds it. A family listed
/// with another period (2π at the double below it), or with no fact of a
/// period beside it, says where infinitely many items are with nothing
/// behind it (review 18's follow-up).
fn families_follow(what: &str, rc: &RowCert, periods: &[Enc], out: &mut RowResult) {
    if periods.is_empty() {
        return;
    }
    let Some(p) = row_period(rc) else {
        out.problems.push(format!(
            "{what} listed as repeating, with no fact that f has a period"
        ));
        return;
    };
    for per in periods {
        if !per.contains(&p) {
            out.problems.push(format!(
                "{what} listed as repeating every [{:e}, {:e}], not the period in [{:e}, {:e}]",
                per.lo, per.hi, p.lo, p.hi
            ));
        }
    }
}

/// Items only where f is defined. With the domain row certified, each
/// x-intercept, extremum and inflection lies in its pieces. Without it,
/// nothing says where f is defined but what the row's own claims show: no
/// monotone pieces or range, no complete list, and each item inside a box
/// claimed Defined and shown so on f's own tree (strictly inside for a
/// turn: f defined on either side of it).
fn in_domain(
    rc: &RowCert,
    all: &[RowCert],
    results: &[ClaimResult],
    out: &mut RowResult,
) -> Result<(), String> {
    let decided = all
        .iter()
        .any(|r| r.name == "domain" && r.status == "Certified");
    let turn = match rc.name.as_str() {
        "x_intercepts" => false,
        "extrema" | "inflections" => true,
        "monotonicity" | "range" if !decided => {
            out.problems.push(format!(
                "the domain is not decided, yet the row is {}",
                rc.status
            ));
            return Ok(());
        }
        _ => return Ok(()),
    };
    let mut items: Vec<(Enc, Option<Enc>)> = Vec::new();
    for it in list(&rc.value)? {
        items.push(if turn {
            let every = it
                .get("every")
                .filter(|v| !v.is_null())
                .map(enc)
                .transpose()?;
            (enc(it.get("x").ok_or("x")?)?, every)
        } else {
            spot(it)?
        });
    }
    if decided {
        let pieces = domain_pieces(all).ok_or("the domain's pieces")?;
        for (x, _) in &items {
            let inside = pieces.iter().any(|p| {
                let (a, b) = extent(p);
                a <= x.lo && x.hi <= b
            });
            if !inside {
                out.problems.push(format!(
                    "the item at [{:e}, {:e}] is outside the domain",
                    x.lo, x.hi
                ));
            }
        }
        return Ok(());
    }
    if complete(rc) {
        out.problems
            .push("the domain is not decided, yet the row is complete".into());
    }
    let shown: Vec<B> = rc
        .claims
        .iter()
        .filter_map(|c| match c {
            Claim::Defined(x)
                if results
                    .iter()
                    .any(|r| r.claim == *c && r.outcome.class == Class::Strong) =>
            {
                Some(*x)
            }
            _ => None,
        })
        .collect();
    for (x, every) in &items {
        if every.is_some() {
            out.problems.push(format!(
                "a family through {:e} listed, yet the domain is not decided",
                x.mid()
            ));
        }
        let inside = shown.iter().any(|d| {
            if turn {
                d.0 < x.lo && x.hi < d.1
            } else {
                d.0 <= x.lo && x.hi <= d.1
            }
        });
        if !inside {
            out.problems.push(format!(
                "the item at [{:e}, {:e}] is not shown where f is defined",
                x.lo, x.hi
            ));
        }
    }
    Ok(())
}

fn zeros_row(fx: &Fx, rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    let w = walk(fx, &rc.claims, 0);
    out.problems.extend(w.problems.iter().cloned());
    coverage(rc, all, &w.boxes, out);
    gap_ends(fx, rc, all, &w, 0, out);
    holes(rc, 0, complete(rc), out);
    if !w.flat.is_empty() {
        out.problems
            .push("f ≡ 0 on a stretch, yet the row lists points".into());
    }
    let listed: Vec<(Enc, Option<Enc>)> = list(&rc.value)?
        .iter()
        .map(spot)
        .collect::<Result<_, _>>()?;
    let derived: Vec<Enc> = w.zeros.iter().map(|z| z.at).collect();
    match_items("an x-intercept", &derived, &listed, complete(rc), out);
    let periods: Vec<Enc> = listed.iter().filter_map(|(_, p)| *p).collect();
    families_follow("an x-intercept", rc, &periods, out);
    repeats_listed("an x-intercept", rc, &listed, out);
    Ok(())
}

/// Extrema (k = 1) or inflections (k = 2): changes of sign of f⁽ᵏ⁾, with
/// f's value there from a claim.
fn turns(
    fx: &Fx,
    rc: &RowCert,
    all: &[RowCert],
    k: usize,
    out: &mut RowResult,
) -> Result<(), String> {
    let w = walk(fx, &rc.claims, k);
    out.problems.extend(w.problems.iter().cloned());
    coverage(rc, all, &w.boxes, out);
    gap_ends(fx, rc, all, &w, k, out);
    sides_decided(fx, rc, all, &w, k, out);
    holes(rc, k, complete(rc), out);
    let mut ch: Vec<(Enc, bool)> = Vec::new();
    for (x, from_pos, kink) in changes(&w) {
        if k == 2 && kink {
            // f″ changes sign across a corner: no inflection is decided
            // there, so the row can't be complete.
            if complete(rc) {
                out.problems.push(format!(
                    "f″ changes sign across the kink at {:e}, yet the row is complete",
                    x.mid()
                ));
            }
            continue;
        }
        ch.push((x, from_pos));
    }
    if k == 1 {
        // Extrema at closed domain ends (a maximum is "from positive").
        let (ends, open) = end_extrema(fx, rc, all, &w);
        for (x, min) in ends {
            ch.push((x, !min));
        }
        // (An end no claim spoke of was no extremum: √x's "no extrema",
        // complete, with the claims of its other rows. Round 19's
        // mutation tester.)
        if let Some(x) = open.first()
            && complete(rc)
        {
            out.problems.push(format!(
                "f at the closed end [{:e}, {:e}] of its domain is not decided (an extremum, or \
                 f constant beside it), yet the row is complete",
                x.lo, x.hi
            ));
        }
    }
    let mut listed = Vec::new();
    for item in list(&rc.value)? {
        let x = enc(item.get("x").ok_or("x")?)?;
        let y = enc(item.get("y").ok_or("y")?)?;
        let every = item
            .get("every")
            .filter(|v| !v.is_null())
            .map(enc)
            .transpose()?;
        if k == 1 {
            let kind = item.get("kind").and_then(Value::as_str).unwrap_or("");
            if let Some((_, from_pos)) = ch.iter().find(|(c, _)| x.contains(c)) {
                let want = if *from_pos { "Max" } else { "Min" };
                if kind != want {
                    out.problems.push(format!(
                        "the extremum at {:e} is a {want}, listed as {kind}",
                        x.mid()
                    ));
                }
            }
        }
        // f's value there: a claim on the x box within y.
        let has_y = rc.claims.iter().any(|c| {
            matches!(c, Claim::Value { x: bx, of: Subject::F(0), lo, hi }
                if bx.0 == x.lo && bx.1 == x.hi && y.lo <= *lo && *hi <= y.hi)
        });
        if !has_y {
            out.problems.push(format!(
                "no claim gives f's value at the item at {:e}",
                x.mid()
            ));
        }
        listed.push((x, every));
    }
    let derived: Vec<Enc> = ch.iter().map(|(x, _)| *x).collect();
    let what = if k == 1 {
        "an extremum"
    } else {
        "an inflection"
    };
    match_items(what, &derived, &listed, complete(rc), out);
    let periods: Vec<Enc> = listed.iter().filter_map(|(_, p)| *p).collect();
    families_follow(what, rc, &periods, out);
    repeats_listed(what, rc, &listed, out);
    Ok(())
}

/// The domain's pieces, from its row (when certified).
fn domain_pieces(all: &[RowCert]) -> Option<Vec<Piece>> {
    let d = all
        .iter()
        .find(|r| r.name == "domain" && r.status == "Certified")?;
    let ps = d.value.get("pieces")?.as_array()?;
    ps.iter().map(|p| piece(p).ok()).collect()
}

/// Local extrema at closed ends of the domain (√x's minimum at 0): f is
/// continuous from the end over the first stretch where f′ has a strict
/// sign (a Continuous claim), and nothing else lies between; rising away
/// from a low end (or falling toward a high one) makes it a minimum.
/// (f′ must also have no zero in the gap beside the end: its GapClear
/// claim, which [`holes`] asks of a complete row.) Returns (where, is a
/// minimum), and the closed ends left undecided: f not shown continuous
/// from the end into the stretch (by a claim, or by the replay itself),
/// nor constant beside it.
fn end_extrema(
    fx: &Fx,
    rc: &RowCert,
    all: &[RowCert],
    w: &Walk,
) -> (Vec<(Enc, bool)>, Vec<Enc>) {
    let Some(pieces) = domain_pieces(all) else {
        return (Vec::new(), Vec::new());
    };
    let conts: Vec<B> = rc
        .claims
        .iter()
        .filter_map(|c| match c {
            Claim::Continuous(x) => Some(*x),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    let mut open = Vec::new();
    for p in &pieces {
        if p.lo == p.hi {
            continue;
        }
        for (b, low) in [(p.lo, true), (p.hi, false)] {
            let Bound::At { x, closed: true } = b else {
                continue;
            };
            // Outside the row's region: not asked.
            if let Some((a, b)) = rc.region.as_ref().and_then(region_box)
                && (x.hi < a || x.lo > b)
            {
                continue;
            }
            // f constant from the end: no extremum of its own there.
            let flat = w.flat.iter().any(|f| {
                if low {
                    f.0 <= x.hi.next_up() && f.1 > x.hi
                } else {
                    f.1 >= x.lo.next_down() && f.0 < x.lo
                }
            });
            if flat {
                continue;
            }
            // The first strict sign of f′ inward from the end.
            let stretch = if low {
                w.signs
                    .iter()
                    .filter(|s| s.0 >= x.lo)
                    .min_by(|a, b| a.0.total_cmp(&b.0))
            } else {
                w.signs
                    .iter()
                    .filter(|s| s.1 <= x.hi)
                    .max_by(|a, b| a.1.total_cmp(&b.1))
            };
            let Some(&(sa, sb, sign)) = stretch else {
                open.push(x);
                continue;
            };
            // f continuous from the end into that stretch: a claim's, or
            // the replay's own.
            let joined = conts.iter().any(|c| {
                if low {
                    c.0 <= x.lo && c.1 >= sa
                } else {
                    c.1 >= x.hi && c.0 <= sb
                }
            }) || {
                let span = if low { (x.lo, sa) } else { (sb, x.hi) };
                span.0 <= span.1
                    && claims::check(fx, &Claim::Continuous(span), &[]).class == Class::Strong
            };
            // Nothing between but a zero at the end itself.
            let between = |z: &Zero| {
                if low {
                    z.at.lo > x.hi && z.at.hi < sa
                } else {
                    z.at.hi < x.lo && z.at.lo > sb
                }
            };
            if joined && !w.zeros.iter().any(between) {
                out.push((x, sign == low));
            } else if !joined {
                open.push(x);
            }
        }
    }
    (out, open)
}

/// A box and nearby translations guaranteed for every value of the
/// proven period. The largest possible low end and smallest possible high
/// end shrink a translated proof; outward sums could conceal a tiny gap.
fn repeated_boxes(b: B, period: Option<Enc>) -> Vec<B> {
    let Some(p) = period else { return vec![b] };
    iv::set_prec(160);
    (-2..=2)
        .filter_map(|k| {
            if k == 0 {
                return Some(b);
            }
            let shift = iv::scale(&Iv::of2(p.lo, p.hi), f64::from(k));
            let lo = iv::add(&Iv::of(b.0), &Iv::point(shift.hi))
                .hi
                .to_f64_round(rug::float::Round::Up);
            let hi = iv::add(&Iv::of(b.1), &Iv::point(shift.lo))
                .lo
                .to_f64_round(rug::float::Round::Down);
            (lo <= hi).then_some((lo, hi))
        })
        .collect()
}

/// No excluded periodic point lies strictly inside this span. Work with
/// enclosures and integer indices; an uncertain member is never discarded.
fn families_clear_span(all: &[RowCert], x: B) -> bool {
    let Some(domain) = all
        .iter()
        .find(|r| r.name == "domain" && r.status == "Certified")
    else {
        return false;
    };
    let Some(families) = domain.value.get("excluded").and_then(Value::as_array) else {
        return false;
    };
    for f in families {
        let (Some(origin), Some(period)) = (
            f.get("x0").and_then(|v| enc(v).ok()),
            f.get("period").and_then(|v| enc(v).ok()),
        ) else {
            return false;
        };
        if period.lo <= 0.0 || !period.hi.is_finite() {
            return false;
        }
        let k = iv::div(
            &iv::sub(&Iv::of2(x.0, x.1), &Iv::of2(origin.lo, origin.hi)),
            &Iv::of2(period.lo, period.hi),
        );
        let (Some(mut a), Some(b)) = (k.lo.ceil().to_integer(), k.hi.floor().to_integer()) else {
            return false;
        };
        if rug::Integer::from(&b - &a) > 4 {
            return false;
        }
        while a <= b {
            let member = iv::add(
                &Iv::of2(origin.lo, origin.hi),
                &iv::mul(
                    &Iv::of2(period.lo, period.hi),
                    &iv::ratio(&a, &rug::Integer::from(1)),
                ),
            );
            if member.hi > x.0 && member.lo < x.1 {
                return false;
            }
            a += 1;
        }
    }
    true
}

/// Bridge a missing derivative span without omitting any interval. Ordinary
/// elementary operations are continuous on each connected component of
/// their domains; steps and modulo need direct continuity evidence. A
/// certified domain's guaranteed interior and excluded families keep the
/// structural fallback from crossing a hole. A derivative bound where it
/// exists then proves the direction, including a continuous domain end.
fn monotone_bridge(fx: &Fx, all: &[RowCert], x: B, want: Option<bool>) -> bool {
    if claims::unmodelled(&fx.f) {
        return false;
    }
    iv::set_prec(160);
    let closed = fx.series(&fx.f, x.0, x.1, 0)[0].clone();
    let mut continuous = !closed.empty && closed.def && closed.cont;
    if !continuous {
        let mut steps = false;
        fx.f.visit(&mut |e| {
            steps |= matches!(
                e,
                graphing::ast::Expr::Call(
                    graphing::ast::Func::Floor
                        | graphing::ast::Func::Ceil
                        | graphing::ast::Func::Round
                        | graphing::ast::Func::Sign
                        | graphing::ast::Func::Mod,
                    _
                )
            )
        });
        let inside = domain_pieces(all).is_some_and(|ds| {
            ds.iter().any(|p| {
                let lo = match p.lo {
                    Bound::At { x, .. } => x.hi,
                    _ => extent(p).0,
                };
                let hi = match p.hi {
                    Bound::At { x, .. } => x.lo,
                    _ => extent(p).1,
                };
                lo <= x.0 && x.1 <= hi
            })
        });
        continuous = !steps && inside && families_clear_span(all, x);
    }
    if !continuous {
        return false;
    }
    if want.is_none() && super::algebra::zero_derivative(&fx.f, &fx.lits, &fx.vars, fx.unit) {
        return true;
    }
    let origin = if x.0.is_finite() {
        x.0
    } else if x.1.is_finite() {
        x.1
    } else {
        0.0
    };
    let ctx = fx.ctx();
    let e = super::eval::recentered(&fx.f, origin, &ctx);
    let dx = iv::sub(&Iv::of2(x.0, x.1), &Iv::of(origin));
    let xb = dx.strict(x.0 == origin && x.1 > origin, x.1 == origin && x.0 < origin);
    let s = super::series::where_defined(|| {
        super::eval::eval(&e, &super::series::var(xb.clone(), 1), 1, &ctx)
    });
    let d = &s[1];
    if !d.empty
        && match want {
            Some(up) => d.beyond(0.0, up),
            None => d.is_exactly(0.0),
        }
    {
        return true;
    }
    // f′'s own exact form, its sign factor by factor, where the series'
    // sum of terms is ∞ − ∞ (beside an open end of the domain).
    want.is_some_and(|up| {
        super::algebra::derivative_tree(&e, &fx.lits, &fx.vars, fx.unit)
            .is_some_and(|d| claims::factor_sign_over(fx, &d, &xb, 6) == Some(up))
    })
}

fn monotonicity(fx: &Fx, rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    let w = walk(fx, &rc.claims, 1);
    out.problems.extend(w.problems.iter().cloned());
    coverage(rc, all, &w.boxes, out);
    gap_ends(fx, rc, all, &w, 1, out);
    holes(rc, 1, complete(rc), out);
    // Where a piece may end: at a turn or kink of f (a zero of the walk),
    // at an end of a stretch where f′ ≡ 0, at an end of the domain (its
    // row's enclosure), at a pole a claim places or a zero of an excluded
    // family's g, or at one of those a whole number of periods on (a row
    // over one period): the listed end holds that enclosure whole, the
    // place being somewhere in it. Or at a double inside a stretch of the
    // piece's own sign (a window's edge). And a piece lies in the domain.
    // (A piece of √(3x − 1) was listed from the double below the domain's
    // end 1/3, or from −1; the two of 1/(3x − 1) as meeting at the double
    // beside its pole. Review 18's follow-up.)
    let mut anchors: Vec<Enc> = w.zeros.iter().map(|z| z.at).collect();
    for f in &w.flat {
        anchors.push(Enc { lo: f.0, hi: f.0 });
        anchors.push(Enc { lo: f.1, hi: f.1 });
    }
    let domain = domain_pieces(all);
    for d in domain.iter().flatten() {
        for b in [d.lo, d.hi] {
            if let Bound::At { x, .. } = b {
                anchors.push(x);
            }
        }
    }
    for r in all {
        for c in &r.claims {
            if let Claim::Unbounded { at, .. } = c {
                anchors.push(Enc { lo: at.0, hi: at.1 });
            }
        }
    }
    let period = match rc.region {
        Some(Region::Period(..)) => row_period(rc),
        _ => None,
    };
    // The excluded families' zeros (the certificate's Family claims): a
    // member is in an enclosure its g changes sign across, monotone.
    let families: Vec<&Subject> = all
        .iter()
        .flat_map(|r| &r.claims)
        .filter_map(|c| match c {
            Claim::Family { of, .. } => Some(of),
            _ => None,
        })
        .fold(Vec::new(), |mut v, s| {
            if !v.contains(&s) {
                v.push(s);
            }
            v
        });
    let anchored = |x: &Enc| {
        anchors.iter().any(|a| x.contains(a))
            || period.is_some_and(|p| {
                anchors
                    .iter()
                    .any(|a| [-2, -1, 1, 2].iter().any(|k| holds_shifted(x, a, &p, *k)))
            })
            || families
                .iter()
                .any(|g| claims::one_zero_in(fx, g, (x.lo, x.hi)))
    };
    // The listed pieces, repeated by the proven period where appropriate,
    // must cover every part of a complete row's domain.
    if let Some(rg @ (Region::Line | Region::Window(..) | Region::Period(..))) = &rc.region {
        let extents: Vec<B> = list(&rc.value)?
            .iter()
            .filter_map(|i| piece(i.get("on")?).ok())
            .flat_map(|p| repeated_boxes(extent(&p), period))
            .collect();
        for t in targets(rg, all) {
            if let Some((a, b)) = uncovered(t, &extents) {
                out.problems.push(format!(
                    "the pieces leave [{a:e}, {b:e}] of the domain out, yet the row is complete"
                ));
                break;
            }
        }
    }
    for item in list(&rc.value)? {
        let p = piece(item.get("on").ok_or("on")?)?;
        let dir = item.get("dir").and_then(Value::as_str).unwrap_or("");
        let (a, b) = extent(&p);
        if let Some(ds) = &domain
            && !ds.iter().any(|d| {
                let (da, db) = extent(d);
                da <= a && b <= db
            })
        {
            out.problems.push(format!(
                "the piece on [{a:e}, {b:e}] reaches outside the domain's pieces"
            ));
        }
        // Inside the piece (its ends' enclosures aside), f′ keeps the
        // listed sign, but at isolated zeros; ≡ 0 for a constant piece.
        let inner_lo = match p.lo {
            Bound::At { x, .. } => x.hi,
            _ => a,
        };
        let inner_hi = match p.hi {
            Bound::At { x, .. } => x.lo,
            _ => b,
        };
        let want = match dir {
            "Increasing" => Some(true),
            "Decreasing" => Some(false),
            "Constant" => None,
            _ => return Err(format!("bad direction {dir:?}")),
        };
        // A partial row too proves each piece throughout its interior:
        // signed stretches of its direction, or flat ones for Constant.
        let mut decided: Vec<B> = match want {
            Some(up) => w
                .signs
                .iter()
                .filter(|(_, _, s)| *s == up)
                .map(|(a, b, _)| (*a, *b))
                .collect(),
            None => w.flat.clone(),
        };
        if let Some(up) = want {
            for g in &w.gaps {
                let clear = rc.claims.iter().any(|c| {
                    matches!(c,
                    Claim::GapClear { x, of: Subject::F(1) } if x.0 <= g.0 && g.1 <= x.1)
                });
                let signed_beside = w
                    .signs
                    .iter()
                    .any(|(sa, sb, s)| *s == up && (*sb == g.0 || *sa == g.1));
                // A hole strictly inside the piece must not hide a jump
                // or split its domain, even when f′ has no zero there.
                let interior = inner_lo < g.0 && g.1 < inner_hi;
                let continuous = !interior
                    || claims::check(fx, &Claim::Continuous(*g), &rc.claims).class == Class::Strong;
                if clear && signed_beside && continuous {
                    decided.push(*g);
                }
            }
        }
        let proof: Vec<B> = decided
            .iter()
            .flat_map(|b| repeated_boxes(*b, period))
            .collect();
        let mut covered = proof;
        while let Some((lo, hi)) = uncovered((inner_lo, inner_hi), &covered) {
            // Endpoints are kept: there are reals between adjacent doubles.
            // Re-evaluate the whole missing span; only a continuous f with
            // this derivative sign (where it exists) can bridge it.
            if !monotone_bridge(fx, all, (lo, hi), want) {
                out.problems.push(format!(
                    "the claims leave [{lo:e}, {hi:e}] of a listed {dir} piece undecided"
                ));
                break;
            }
            covered.push((lo, hi));
        }
        // (Or the end is a double inside a stretch where f′ has the
        // piece's sign: f is monotone from there on, the way it is listed.
        // A piece cut at a window's edge ends so.)
        let within = |d: f64| match want {
            Some(up) => w
                .signs
                .iter()
                .any(|(sa, sb, s)| *s == up && *sa <= d && d <= *sb),
            None => w.flat.iter().any(|f| f.0 <= d && d <= f.1),
        };
        // A closed end belongs to the piece: f is defined there and
        // continuous to it from the piece (on the end's enclosure, or from
        // the double beside a point end), so the direction shown inside
        // holds up to and at the end. (Only an end shown undefined, or one
        // that is an open end of the domain's pieces, was refused: tan's
        // piece closed at its poles, which are an excluded family, passed.
        // Round 19's mutation tester.)
        for (end, low) in [(p.lo, true), (p.hi, false)] {
            let Bound::At { x, closed: true } = end else {
                continue;
            };
            let beside = if x.lo < x.hi {
                (x.lo, x.hi)
            } else if low {
                (x.lo, x.lo.next_up())
            } else {
                (x.lo.next_down(), x.lo)
            };
            if claims::check(fx, &Claim::Continuous(beside), &[]).class != Class::Strong {
                out.problems.push(format!(
                    "a monotone piece is closed at [{:e}, {:e}], where f is not shown defined \
                     and continuous from the piece",
                    x.lo, x.hi
                ));
            }
        }
        for end in [p.lo, p.hi] {
            if let Bound::At { x, closed: true } = end
                && x.lo == x.hi
                && no_value(fx, x.lo)
            {
                out.problems.push(format!(
                    "a monotone piece includes its undefined endpoint {:e}",
                    x.lo
                ));
            }
            if let Bound::At { x, closed: true } = end
                && domain.iter().flatten().flat_map(|d| [d.lo, d.hi]).any(|b|
                    matches!(b, Bound::At { x: excluded, closed: false } if x.contains(&excluded)))
            {
                out.problems.push(format!(
                    "a closed monotone endpoint [{:e}, {:e}] contains an open domain end",
                    x.lo, x.hi));
            }
            if let Bound::At { x, .. } = end
                && !(anchored(&x) || (x.lo == x.hi && within(x.lo)))
            {
                out.problems.push(format!(
                    "a piece ends in [{:e}, {:e}], which holds no turn of f, end of its domain \
                     or pole the claims place",
                    x.lo, x.hi
                ));
            }
        }
        for (sa, sb, s) in &w.signs {
            let overlaps = repeated_boxes((*sa, *sb), period)
                .iter()
                .any(|(a, b)| *a < inner_hi && *b > inner_lo);
            if overlaps && want.is_some_and(|d| d != *s) {
                out.problems.push(format!(
                    "f′ is {} on [{sa:e}, {sb:e}], inside a piece listed {dir}",
                    if *s { "positive" } else { "negative" }
                ));
            }
            if overlaps && want.is_none() {
                out.problems.push(format!(
                    "f′ ≠ 0 on [{sa:e}, {sb:e}], inside a piece listed constant"
                ));
            }
        }
        // A change of f′'s sign is where one piece ends and the next
        // begins: its enclosure lies in the ends' enclosures, none of it
        // inside a piece. (Only a zero wholly inside was looked at: pieces
        // listed as meeting at a double beside the turn, 1/3 at
        // 0.333…3148, held the turn's enclosure across their ends. Review
        // 18's follow-up.)
        for z in &w.zeros {
            let inside = repeated_boxes((z.at.lo, z.at.hi), period)
                .iter()
                .any(|(a, b)| *b > inner_lo && *a < inner_hi);
            if inside && z.left.is_some() && z.right.is_some() && z.left != z.right {
                out.problems.push(format!(
                    "f′ changes sign at {:e}, inside a piece listed {dir}",
                    z.at.mid()
                ));
            }
        }
    }
    Ok(())
}

fn y_intercept(fx: &Fx, rc: &RowCert, out: &mut RowResult) -> Result<(), String> {
    // Directly: f at 0.
    let s = Subj::new(fx, &Subject::F(0), Tree::Orig).expect("f");
    iv::set_prec(160);
    let (v, valid) = s.at(0.0);
    if rc.value.is_null() {
        // (Shown undefined, on f itself: an f(0) the replay can't place
        // was "no y-intercept" too.)
        if !v.empty && !valid {
            out.problems.push(format!(
                "no y-intercept listed, yet f(0) is not shown undefined: {}",
                claims::show(&v)
            ));
        }
        if !v.empty && valid {
            out.problems
                .push(format!(
                    "no y-intercept listed, yet f(0) is defined: {}",
                    claims::show(&v)
                ));
        }
        return Ok(());
    }
    let e = enc(&rc.value)?;
    if v.empty {
        out.problems
            .push("a y-intercept listed, yet f(0) is undefined".into());
    } else if valid && (v.hi < e.lo || v.lo > e.hi) {
        out.problems.push(format!(
            "f(0) is not in the listed [{:e}, {:e}]",
            e.lo, e.hi
        ));
    }
    let claimed = rc.claims.iter().any(|c| {
        matches!(c, Claim::Value { x: (0.0, 0.0), of: Subject::F(0), lo, hi } if e.lo <= *lo && *hi <= e.hi)
    });
    if !claimed {
        out.problems.push("no claim gives f(0)".into());
    }
    Ok(())
}

/// The domain (its row's value) is symmetric about 0: each piece's mirror
/// is a piece, each excluded family its own mirror. None: not known.
fn domain_symmetric(all: &[RowCert]) -> Option<bool> {
    let d = all
        .iter()
        .find(|r| r.name == "domain" && r.status == "Certified")?;
    let pieces: Vec<Piece> = d
        .value
        .get("pieces")?
        .as_array()?
        .iter()
        .map(|p| piece(p).ok())
        .collect::<Option<_>>()?;
    let mirror = |b: Bound| match b {
        Bound::NegInf => Bound::PosInf,
        Bound::PosInf => Bound::NegInf,
        Bound::At { x, closed } => Bound::At {
            x: Enc {
                lo: -x.hi,
                hi: -x.lo,
            },
            closed,
        },
    };
    let pieces_ok = pieces.iter().all(|p| {
        let m = Piece {
            lo: mirror(p.hi),
            hi: mirror(p.lo),
        };
        pieces.contains(&m)
    });
    let fams: Vec<(Enc, Enc)> = d
        .value
        .get("excluded")?
        .as_array()?
        .iter()
        .map(|f| Some((enc(f.get("x0")?).ok()?, enc(f.get("period")?).ok()?)))
        .collect::<Option<_>>()?;
    let fams_ok = fams.iter().all(|(x0, per)| {
        // −x0 = x0 + k·P for an integer k (within the enclosures).
        meets_member(
            x0,
            per,
            &Enc {
                lo: -x0.hi,
                hi: -x0.lo,
            },
        )
    });
    Some(pieces_ok && fams_ok)
}

fn parity(
    fx: &Fx,
    rc: &RowCert,
    all: &[RowCert],
    results: &[ClaimResult],
    out: &mut RowResult,
) -> Result<(), String> {
    let p = rc.value.as_str().ok_or("parity")?;
    match p {
        "Even" | "Odd" => {
            match domain_symmetric(all) {
                Some(true) => {}
                Some(false) => out
                    .problems
                    .push(format!("{p}, but the domain is not symmetric about 0")),
                // No domain row to read it from: f's own structure may
                // still say it (built of even and odd parts, f is defined
                // at −x wherever it is at x). The fact f(−x) = ±f(x) alone
                // doesn't: the algebra shows it where both sides are
                // defined, with what cancels cancelled (ln x − ln x + x²
                // was "even", on x > 0, its other rows withheld: the third
                // pass's audit).
                None if claims::tree_parity(&fx.f, &fx.lits) == Some(p == "Even") => out
                    .notes
                    .push("the domain's symmetry: by f's structure, with no domain row".into()),
                // Else it is not derived. (As far as sample points went, f
                // defined at both or neither of ±x at each, it was noted
                // and passed: a domain is not symmetric by what a few
                // points show.)
                None => out.problems.push(format!(
                    "{p}, but the domain is not known to be symmetric about 0 (no domain row, \
                     and f's structure doesn't say)"
                )),
            }
            let word = if p == "Even" { "even" } else { "odd" };
            let fact = rc.claims.iter().find(
                |c| matches!(c, Claim::Simplifier(f) if f.starts_with(&format!("f is {word}"))),
            );
            match fact {
                None => out
                    .problems
                    .push(format!("{p} with no fact saying f is {word}")),
                Some(c) => {
                    if let Some(r) = results.iter().find(|r| r.claim == *c)
                        && r.outcome.class == Class::Weak
                    {
                        out.notes
                            .push(format!("f is {word}: tested at points only"));
                    }
                }
            }
        }
        "Neither" => {
            // Witnesses: f at x and −x from the claims, apart from f(x)
            // (not even) and from −f(x) (not odd), or defined at only one.
            let at = |x: f64| -> Option<Option<(f64, f64)>> {
                rc.claims.iter().find_map(|c| match c {
                    Claim::Value {
                        x: bx,
                        of: Subject::F(0),
                        lo,
                        hi,
                    } if bx.0 == x && bx.1 == x => Some(Some((*lo, *hi))),
                    Claim::Undefined(bx) if bx.0 == x && bx.1 == x => Some(None),
                    _ => None,
                })
            };
            let (mut not_even, mut not_odd) = (false, false);
            for c in &rc.claims {
                let (Claim::Value { x: bx, .. } | Claim::Undefined(bx)) = c else {
                    continue;
                };
                if bx.0 != bx.1 || bx.0 <= 0.0 {
                    continue;
                }
                let (Some(a), Some(b)) = (at(bx.0), at(-bx.0)) else {
                    continue;
                };
                match (a, b) {
                    (Some((al, ah)), Some((bl, bh))) => {
                        not_even |= ah < bl || bh < al;
                        not_odd |= ah < -bh || -bl < al;
                    }
                    (None, Some(_)) | (Some(_), None) => {
                        not_even = true;
                        not_odd = true;
                    }
                    (None, None) => {}
                }
            }
            if !(not_even && not_odd) {
                out.problems
                    .push("Neither, but the claims don't show f is neither even nor odd".into());
            }
        }
        other => out.problems.push(format!("unknown parity {other}")),
    }
    Ok(())
}

// ------------------------------------------------------------ domain

/// f over the reals strictly between the double p and the double beside
/// it (on its right, or its left), p itself left out: x − p keeps its
/// strict sign there (x − p written in f is that difference exactly: 1/(x
/// − 1) beside 1 is 1/t, t > 0). An enclosure of f's values where it is
/// defined; `def` where it is shown defined throughout.
fn over_open_gap(fx: &Fx, p: f64, right: bool) -> Iv {
    iv::set_prec(160);
    let q = if right { p.next_up() } else { p.next_down() };
    let (lo, hi) = if right { (p, q) } else { (q, p) };
    let ctx = fx.ctx();
    let (e, x) = if p == 0.0 {
        (fx.f.clone(), Iv::of2(lo, hi).strict(right, !right))
    } else {
        (
            super::eval::recentered(&fx.f, p, &ctx),
            iv::sub(&Iv::of2(lo, hi), &Iv::of(p)).strict(right, !right),
        )
    };
    super::series::where_defined(|| {
        super::eval::eval(&e, &super::series::var(x, 0), 0, &ctx)[0].clone()
    })
}

/// Whether f is shown defined at every real strictly between the double p,
/// where it is not, and the double beside it: nothing else is excluded
/// there. By f's own enclosure over those reals ([`over_open_gap`]); or by
/// the nodes that exclude p: each is 0 at p exactly where f has no value
/// at its zeros (a divisor), or at the edge of its function's domain there
/// (ln's 0), and is continuous and strictly monotone from p to that
/// double, so it is nonzero, or inside its domain, at every real between;
/// and with those nodes stood in for, by any value they may take there,
/// the rest of f is defined over the whole of it.
///
/// (The pieces were shown defined from the double beside an open end on.
/// 1/((x − 1)·(x − 1.00000000000000001)) has a second pole strictly
/// between 1 and that double: its domain passed as x ≠ 1, and every other
/// row with it. Round 19's follow-up.)
fn open_gap_defined(fx: &Fx, p: f64, right: bool) -> bool {
    use graphing::ast::Expr;
    if claims::unmodelled(&fx.f) {
        return false;
    }
    let v = over_open_gap(fx, p, right);
    if !v.empty && v.def {
        return true;
    }
    iv::set_prec(160);
    let q = if right { p.next_up() } else { p.next_down() };
    let (lo, hi) = if right { (p, q) } else { (q, p) };
    let at = |g: &Expr, x: f64| fx.series(g, x, x, 0)[0].clone();
    let monotone = |g: &Expr| {
        let s = fx.series(g, lo, hi, 1);
        s[0].cont && !s[1].empty && s[1].def && s[1].ne0()
    };
    // (path, stand-in): shortest paths first, none inside another.
    let mut stood: Vec<(Vec<u8>, &'static str)> = Vec::new();
    let mut zeros = Vec::new();
    paths_where(
        &fx.f,
        &|g, path| {
            let excluded = divisor(&fx.f, path) || {
                let zeroed = replace_with(&fx.f, path, Expr::exact(0.0));
                fx.series(&zeroed, f64::NEG_INFINITY, f64::INFINITY, 0)[0].empty
            };
            excluded && at(g, p).is_exactly(0.0) && monotone(g)
        },
        &mut Vec::new(),
        &mut zeros,
    );
    for path in zeros {
        let Some(g) = super::eval::at_path(&fx.f, &path) else {
            continue;
        };
        let far = at(g, q);
        if far.gt(0.0) {
            stood.push((path, "__pos"));
        } else if far.lt(0.0) {
            stood.push((path, "__neg"));
        }
    }
    let mut es = Vec::new();
    edges(fx, &fx.f, &mut Vec::new(), &mut es);
    for ed in es {
        let Some(g) = super::eval::at_path(&fx.f, &ed.path) else {
            continue;
        };
        let far = at(g, q);
        let allowed = if ed.above {
            far.gt(ed.c)
        } else {
            far.lt(ed.c)
        };
        let inside = ed.stand_in != "__unit" || (far.gt(-1.0) && far.lt(1.0));
        if at(g, p).is_exactly(ed.c) && monotone(g) && allowed && inside {
            stood.push((ed.path, ed.stand_in));
        }
    }
    if stood.is_empty() {
        return false;
    }
    stood.sort_by_key(|(path, _)| path.len());
    let mut done: Vec<Vec<u8>> = Vec::new();
    let mut t = fx.f.clone();
    for (path, stand_in) in stood {
        if done.iter().any(|d| path.starts_with(d)) {
            continue;
        }
        t = replace(&t, &path, stand_in);
        done.push(path);
    }
    let v = &fx.series(&t, lo, hi, 0)[0];
    !v.empty && v.def
}

/// The domain checked directly: f defined on each piece (but at its
/// families' members), undefined between pieces, its ends as listed.
fn domain(fx: &Fx, rc: &RowCert, out: &mut RowResult) -> Result<(), String> {
    let pieces: Vec<Piece> = list(rc.value.get("pieces").ok_or("pieces")?)?
        .iter()
        .map(piece)
        .collect::<Result<_, _>>()?;
    let families: Vec<(Enc, Enc)> = list(rc.value.get("excluded").ok_or("excluded")?)?
        .iter()
        .map(|f| {
            Ok((
                enc(f.get("x0").ok_or("x0")?)?,
                enc(f.get("period").ok_or("period")?)?,
            ))
        })
        .collect::<Result<_, String>>()?;
    if claims::unmodelled(&fx.f) {
        // (Noted and passed: a domain for any such f, its claims withheld.)
        out.problems
            .push("not derived: the tree uses a function the replay doesn't model".into());
        return Ok(());
    }
    // Each family: a Family claim with these boxes, about a node whose
    // condition is g ≠ 0 (a divisor, or the cosine or sine under tan, sec,
    // cot, csc). Every node of f with that condition stands in as "any
    // value" (any nonzero value, of one sign, for g itself) when the rest
    // of f is checked defined: the claim names one, and f may hold more
    // (tan x + sec x, sec³ x written as a product: with one stood in, the
    // others kept the piece from being shown defined).
    let mut stand_ins: Vec<(Paths, Paths)> = Vec::new();
    for (x0, per) in &families {
        let claim = rc.claims.iter().find_map(|c| match c {
            Claim::Family {
                of: Subject::G(path, via),
                x0: a,
                period: p,
            } if a.0 == x0.lo && a.1 == x0.hi && p.0 == per.lo && p.1 == per.hi => {
                Some((path.clone(), *via))
            }
            _ => None,
        });
        let Some((path, via)) = claim else {
            out.problems.push(format!(
                "an excluded family at {:e} without its Family claim",
                x0.mid()
            ));
            continue;
        };
        let Some(g) = claims::side_tree(&fx.f, &path, via) else {
            out.problems
                .push(format!("the family at {:e}: no such node of f", x0.mid()));
            return Ok(());
        };
        // g itself, where f has no value at its zeros: a divisor, the base
        // of a negative power (or of a positive one that is such a node),
        // or any node f is undefined everywhere with 0 in the place of
        // (tan(x)⁰, a root of a negative degree, the divisor |tan x|).
        let zero_excluded = |at: &[u8]| {
            divisor(&fx.f, at) || {
                iv::set_prec(160);
                let zeroed = replace_with(&fx.f, at, graphing::ast::Expr::exact(0.0));
                fx.series(&zeroed, f64::NEG_INFINITY, f64::INFINITY, 0)[0].empty
            }
        };
        if via == super::Via::Itself && !zero_excluded(&path) {
            // (Not read is not checked: the pieces listed beside such a
            // family were let through with a note.)
            out.problems.push(format!(
                "the family at {:e}: its node's condition is not read (f has a value with 0 \
                 in its place), so the domain is not derived",
                x0.mid()
            ));
            return Ok(());
        }
        let (mut signed, mut any) = (Vec::new(), Vec::new());
        paths_where(
            &fx.f,
            &|e, at| *e == g && zero_excluded(at),
            &mut Vec::new(),
            &mut signed,
        );
        paths_where(
            &fx.f,
            &|e, _| {
                use graphing::ast::{Expr, Func};
                let of = |f: Func, a: &Expr| Expr::Call(f, vec![a.clone()]);
                match e {
                    Expr::Call(Func::Tan | Func::Sec, a) => of(Func::Cos, &a[0]) == g,
                    Expr::Call(Func::Cot | Func::Csc, a) => of(Func::Sin, &a[0]) == g,
                    _ => false,
                }
            },
            &mut Vec::new(),
            &mut any,
        );
        stand_ins.push((signed, any));
    }
    // The tree(s) to check definedness on.
    let mut trees: Vec<graphing::ast::Expr> = vec![fx.f.clone()];
    for (signed, any) in &stand_ins {
        let all = |t: &graphing::ast::Expr, paths: &[Vec<u8>], name: &str| {
            paths.iter().fold(t.clone(), |t, at| replace(&t, at, name))
        };
        let mut next = Vec::new();
        for t in &trees {
            let t = all(t, any, "__any");
            if signed.is_empty() {
                next.push(t);
            } else {
                next.push(all(&t, signed, "__pos"));
                next.push(all(&t, signed, "__neg"));
            }
        }
        trees = next;
    }
    let defined_on = |t: &graphing::ast::Expr, x: B| -> claims::V {
        claims::ladder(|| {
            let mut budget = 4000;
            claims::prove(x.0, x.1, &mut budget, &mut |lo, hi| {
                let v = &fx.series(t, lo, hi, 0)[0];
                if !v.empty && v.def {
                    claims::V::Yes
                } else if v.empty {
                    claims::V::No(format!("undefined on [{lo:e}, {hi:e}]"))
                } else {
                    claims::V::Unknown(format!("not shown defined on [{lo:e}, {hi:e}]"))
                }
            })
        })
    };
    let undefined_on = |x: B| -> claims::V {
        claims::ladder(|| {
            let mut budget = 4000;
            claims::prove(x.0, x.1, &mut budget, &mut |lo, hi| {
                let v = &fx.series(&fx.f, lo, hi, 0)[0];
                if v.empty {
                    claims::V::Yes
                } else if v.def {
                    claims::V::No(format!("defined on [{lo:e}, {hi:e}]"))
                } else {
                    claims::V::Unknown(format!("not shown undefined on [{lo:e}, {hi:e}]"))
                }
            })
        })
    };
    // The inside of each piece (doubles strictly inside its ends'
    // enclosures; a closed point end included).
    let mut sorted = pieces.clone();
    sorted.sort_by(|a, b| extent(a).0.total_cmp(&extent(b).0));
    for p in &sorted {
        let lo = match p.lo {
            Bound::NegInf => f64::NEG_INFINITY,
            Bound::PosInf => f64::INFINITY,
            Bound::At { x, closed } => {
                if x.lo == x.hi && closed {
                    x.lo
                } else {
                    x.hi.next_up()
                }
            }
        };
        let hi = match p.hi {
            Bound::NegInf => f64::NEG_INFINITY,
            Bound::PosInf => f64::INFINITY,
            Bound::At { x, closed } => {
                if x.lo == x.hi && closed {
                    x.lo
                } else {
                    x.lo.next_down()
                }
            }
        };
        if lo > hi {
            continue;
        }
        if families.is_empty() {
            match defined_on(&fx.f, (lo, hi)) {
                claims::V::Yes => {}
                claims::V::No(w) => out
                    .problems
                    .push(format!("f is not defined on its piece: {w}")),
                // Not shown on f's tree as written: as a `Defined` claim
                // is (its rearranged form where the tree is ∞ − ∞ on a
                // tail: 1/(x² + x + 1)). A piece shown no way is not the
                // row's to list: it was noted and let through, and any
                // domain passed for an f the replay places nowhere
                // (√(10^5000 + 1 − 10^5000 − 1 + x) on all of ℝ: the third
                // pass's audit).
                claims::V::Unknown(_) if claims::orig_defined(fx, (lo, hi)) => {}
                claims::V::Unknown(w) => out
                    .problems
                    .push(format!("f is not shown defined on its piece: {w}")),
            }
        } else {
            for t in &trees {
                match defined_on(t, (lo, hi)) {
                    claims::V::Yes => {}
                    // (Not shown is not let through: a piece reaching past
                    // √x's end beside tan's poles was noted and passed.)
                    claims::V::No(w) | claims::V::Unknown(w) => {
                        out.problems.push(format!(
                            "off its families, f is not shown defined on its piece: {w}"
                        ));
                        break;
                    }
                }
            }
        }
        // Open ends at doubles: undefined there.
        for b in [p.lo, p.hi] {
            if let Bound::At { x, closed: false } = b
                && x.lo == x.hi
                && let claims::V::No(w) = undefined_on((x.lo, x.lo))
            {
                out.problems
                    .push(format!("an open end at {:e}, yet {w}", x.lo));
            }
        }
        // Between an open end at a double and the double beside it, inside
        // the piece, f is defined too: the boxes above start at that next
        // double.
        for (b, low) in [(p.lo, true), (p.hi, false)] {
            if let Bound::At { x, closed: false } = b
                && x.lo == x.hi
                && lo <= hi
                && !open_gap_defined(fx, x.lo, low)
            {
                out.problems.push(format!(
                    "f is not shown defined between its open end {:e} and the double beside it",
                    x.lo
                ));
            }
        }
        // Ends known only to an enclosure: the end inside it, and whether
        // f is defined there, from the edge of a node's domain.
        for (b, low) in [(p.lo, true), (p.hi, false)] {
            let Bound::At { x, closed } = b else { continue };
            if x.lo == x.hi {
                continue;
            }
            let shut = |c: bool| if c { "closed" } else { "open" };
            match enclosed_end(fx, (x.lo, x.hi), low) {
                Some(c) if c != closed => out.problems.push(format!(
                    "the end in [{:e}, {:e}] is listed {}, yet it is {}",
                    x.lo,
                    x.hi,
                    shut(closed),
                    shut(c)
                )),
                Some(_) => {}
                // (Noted and passed: the end open or closed as listed,
                // whichever it was.)
                None => out.problems.push(format!(
                    "the end in [{:e}, {:e}]: not shown {}",
                    x.lo,
                    x.hi,
                    shut(closed)
                )),
            }
        }
        // The end is in its listed enclosure, not beside it. Above, f is
        // checked at the doubles outside the enclosure and inside the
        // piece, which an end strictly between two doubles slips past: an
        // edge of one of f's nodes crossed strictly between the enclosure
        // and the double next to it (√(3x − 1)'s end 1/3, listed closed at
        // the double above it or in an interval from there) puts the end
        // there, outside the listing. And an enclosure f is defined
        // throughout holds no end at all. (Review 18's follow-up.)
        for (b, low) in [(p.lo, true), (p.hi, false)] {
            let Bound::At { x, .. } = b else { continue };
            for beside in [(x.lo.next_down(), x.lo), (x.hi, x.hi.next_up())] {
                if enclosed_end(fx, beside, low).is_some() {
                    out.problems.push(format!(
                        "the end listed in [{:e}, {:e}] is strictly between {:e} and {:e}",
                        x.lo, x.hi, beside.0, beside.1
                    ));
                }
            }
            if x.lo < x.hi && matches!(defined_on(&fx.f, (x.lo, x.hi)), claims::V::Yes) {
                out.problems.push(format!(
                    "no end of the domain in [{:e}, {:e}]: f is defined throughout it",
                    x.lo, x.hi
                ));
            }
        }
    }
    // Two pieces meeting in one enclosure, an end of them closed: the point
    // they meet at is in the domain, so f is defined throughout the
    // enclosure (and nothing separates the pieces), or the listing lets in
    // a point f is undefined at (the pole of 1/(3x − 1), its left piece
    // listed closed: review 18's follow-up).
    for pair in sorted.windows(2) {
        if let (Bound::At { x, closed: c0 }, Bound::At { x: y, closed: c1 }) =
            (pair[0].hi, pair[1].lo)
            && x == y
            && x.lo < x.hi
            && (c0 || c1)
            && !matches!(defined_on(&fx.f, (x.lo, x.hi)), claims::V::Yes)
        {
            out.problems.push(format!(
                "two pieces meet in [{:e}, {:e}] with an end closed, yet f is not shown \
                 defined throughout it",
                x.lo, x.hi
            ));
        }
    }
    // Between pieces, and beyond the outer ones: undefined. (Doubles only
    // outside an end's enclosure; an open end at a double is outside.)
    let before = |b: Bound| -> Option<f64> {
        match b {
            Bound::At { x, closed } if x.lo == x.hi && !closed => Some(x.lo),
            Bound::At { x, .. } => Some(x.lo.next_down()),
            _ => None,
        }
    };
    let after = |b: Bound| -> Option<f64> {
        match b {
            Bound::At { x, closed } if x.lo == x.hi && !closed => Some(x.lo),
            Bound::At { x, .. } => Some(x.hi.next_up()),
            _ => None,
        }
    };
    let mut outside: Vec<B> = Vec::new();
    let mut start = Some(f64::NEG_INFINITY);
    for p in &sorted {
        if let (Some(s), Some(e)) = (start, before(p.lo)) {
            outside.push((s, e));
        }
        start = if p.hi == Bound::PosInf {
            None
        } else {
            after(p.hi)
        };
    }
    if let Some(s) = start {
        outside.push((s, f64::INFINITY));
    }
    if sorted.is_empty() {
        outside = vec![(f64::NEG_INFINITY, f64::INFINITY)];
    }
    for x in outside.into_iter().filter(|x| x.0 <= x.1) {
        if let claims::V::No(w) | claims::V::Unknown(w) = undefined_on(x) {
            out.problems
                .push(format!("outside the domain, f is not shown undefined: {w}"));
        }
    }
    if pieces.is_empty() {
        out.notes.push("an empty domain".into());
    }
    Ok(())
}

/// Where a node's argument g meets an edge `c` of its function's domain:
/// allowed strictly `above` c (or below), undefined strictly beyond it,
/// with a stand-in for g's allowed values beside the edge.
struct Edge {
    path: Vec<u8>,
    c: f64,
    above: bool,
    stand_in: &'static str,
}

/// The edges of f's nodes: √, even roots and powers with an even
/// denominator, ln and log (g ≥ 0, g > 0); asin, acos, atanh (|g| ≤ 1,
/// < 1); acosh (g ≥ 1); a power with a varying exponent (a positive base,
/// or 0 to a positive power); 0^b (b > 0). Which side of the edge holds,
/// at the edge itself, is f's evaluation with g = c.
fn edges(fx: &Fx, e: &graphing::ast::Expr, path: &mut Vec<u8>, out: &mut Vec<Edge>) {
    use graphing::ast::{BinOp, Expr, Func};
    let mut push = |i: u8, c: f64, above: bool| {
        let mut p = path.clone();
        p.push(i);
        let stand_in = match (c == 0.0, above) {
            (true, true) => "__pos",
            (true, false) => "__neg",
            (false, true) if c == 1.0 => "__ge1",
            _ => "__unit",
        };
        out.push(Edge {
            path: p,
            c,
            above,
            stand_in,
        });
    };
    let even = |k: &Expr| {
        super::eval::written_rational(k, &fx.lits).is_some_and(|(p, q)| q == 1 && p.is_even())
    };
    match e {
        Expr::Call(f, args) => match f {
            Func::Sqrt | Func::Ln | Func::Log => push(0, 0.0, true),
            Func::Root if even(&args[1]) => push(0, 0.0, true),
            Func::Asin | Func::Acos | Func::Atanh => {
                push(0, 1.0, false);
                push(0, -1.0, true);
            }
            Func::Acosh => push(0, 1.0, true),
            _ => {}
        },
        Expr::Bin(BinOp::Pow, a, b) => match super::eval::written_rational(b, &fx.lits) {
            Some((_, q)) if q.is_even() => push(0, 0.0, true),
            Some(_) => {}
            // A constant exactly an integer: an integer power.
            None if !super::eval::contains_x(b)
                && super::exact::eval(b, None, &fx.lits, &fx.vars, fx.unit)
                    .is_some_and(|k| k.is_integer()) => {}
            None => {
                push(0, 0.0, true);
                let zero = !super::eval::contains_x(a) && {
                    let v = &fx.series(a, 0.0, 0.0, 0)[0];
                    v.def && v.is_point() && v.lo == 0
                };
                if zero {
                    push(1, 0.0, true);
                }
            }
        },
        _ => {}
    }
    let kids: Vec<&Expr> = match e {
        Expr::Neg(a) | Expr::Degrees(a) => vec![&**a],
        Expr::Bin(_, a, b) => vec![&**a, &**b],
        Expr::Call(_, args) => args.iter().collect(),
        _ => Vec::new(),
    };
    for (i, k) in kids.into_iter().enumerate() {
        // A path names a child by a byte: the edges under a child past the
        // 256th (of a min or max) have no path, and are left out, not
        // found under another child's (the 257th's index wrapped to the
        // first's: review 18's audit). Fewer edges show less, never more.
        let Ok(i) = u8::try_from(i) else { break };
        path.push(i);
        edges(fx, k, path, out);
        path.pop();
    }
}

/// The end of a piece known only to the enclosure `x` (a low end: the
/// piece to its right): whether f is defined there (closed), shown from an
/// edge of one of f's nodes. Its g is continuous and strictly monotone
/// over `x`, strictly allowed at the end toward the piece and strictly
/// beyond c at the other, so g = c at exactly one e inside, with f
/// undefined on the far side of e; f with g's stand-in is defined over
/// `x`, so f is defined on the near side; f with g = c is defined over `x`
/// (closed) or nowhere on it (open).
fn enclosed_end(fx: &Fx, x: B, low: bool) -> Option<bool> {
    use graphing::ast::Expr;
    if claims::unmodelled(&fx.f) {
        return None;
    }
    iv::set_prec(160);
    let (near, far) = if low { (x.1, x.0) } else { (x.0, x.1) };
    let mut es = Vec::new();
    edges(fx, &fx.f, &mut Vec::new(), &mut es);
    for ed in es {
        let Some(g) = super::eval::at_path(&fx.f, &ed.path) else {
            continue;
        };
        let s = fx.series(g, x.0, x.1, 1);
        if !(s[0].cont && !s[1].empty && s[1].def && s[1].ne0()) {
            continue;
        }
        let at = |p: f64| fx.series(g, p, p, 0)[0].clone();
        let (gn, gf) = (at(near), at(far));
        let allowed = |v: &Iv| if ed.above { v.gt(ed.c) } else { v.lt(ed.c) };
        let beyond = |v: &Iv| if ed.above { v.lt(ed.c) } else { v.gt(ed.c) };
        let inside = ed.stand_in != "__unit" || (gn.gt(-1.0) && gn.lt(1.0));
        if !(allowed(&gn) && inside && beyond(&gf)) {
            continue;
        }
        let v = &fx.series(&replace(&fx.f, &ed.path, ed.stand_in), x.0, x.1, 0)[0];
        if v.empty || !v.def {
            continue;
        }
        let edge = replace_with(&fx.f, &ed.path, Expr::exact(ed.c));
        let v = &fx.series(&edge, x.0, x.1, 0)[0];
        if v.empty {
            return Some(false);
        }
        if v.def {
            return Some(true);
        }
    }
    None
}

/// Whether the node at `path` is one f has no value at the zeros of: a
/// divisor of `f`, the base of a negative written power (0 to it is 1/0:
/// cos(x)^(−9007199254740993/1000001) at cos's zeros), or the base of a
/// positive power (a chain of them) that is one of these.
fn divisor(f: &graphing::ast::Expr, path: &[u8]) -> bool {
    use graphing::ast::{BinOp, Expr};
    let Some((last, up)) = path.split_last() else {
        return false;
    };
    match super::eval::at_path(f, up) {
        Some(Expr::Bin(BinOp::Div, ..)) => *last == 1,
        Some(Expr::Bin(BinOp::Pow, _, k)) if *last == 0 => {
            match super::eval::written_rational(k, &super::eval::Lits::default()) {
                Some((p, _)) if p < 0 => true,
                Some((p, _)) if p > 0 => divisor(f, up),
                _ => false,
            }
        }
        _ => false,
    }
}

/// Paths to nodes of f's tree.
type Paths = Vec<Vec<u8>>;

/// The paths of the nodes of `e` that `is` holds of, none inside another
/// (a path names a child by a byte: those past the 256th are left out).
fn paths_where(
    e: &graphing::ast::Expr,
    is: &dyn Fn(&graphing::ast::Expr, &[u8]) -> bool,
    path: &mut Vec<u8>,
    out: &mut Paths,
) {
    use graphing::ast::Expr;
    if is(e, path) {
        out.push(path.clone());
        return;
    }
    let kids: Vec<&Expr> = match e {
        Expr::Neg(a) | Expr::Degrees(a) => vec![&**a],
        Expr::Bin(_, a, b) => vec![&**a, &**b],
        Expr::Call(_, args) => args.iter().collect(),
        _ => Vec::new(),
    };
    for (i, k) in kids.into_iter().enumerate() {
        let Ok(i) = u8::try_from(i) else { break };
        path.push(i);
        paths_where(k, is, path, out);
        path.pop();
    }
}

/// `t` with the node at `path` replaced by a stand-in variable.
fn replace(t: &graphing::ast::Expr, path: &[u8], name: &str) -> graphing::ast::Expr {
    replace_with(t, path, graphing::ast::Expr::Var(name.into()))
}

/// `t` with the node at `path` replaced by `new`.
fn replace_with(
    t: &graphing::ast::Expr,
    path: &[u8],
    new: graphing::ast::Expr,
) -> graphing::ast::Expr {
    use graphing::ast::Expr;
    let Some((&i, rest)) = path.split_first() else {
        return new;
    };
    match t {
        Expr::Neg(a) => Expr::Neg(Box::new(replace_with(a, rest, new))),
        Expr::Degrees(a) => Expr::Degrees(Box::new(replace_with(a, rest, new))),
        Expr::Bin(op, a, b) => {
            if i == 0 {
                Expr::Bin(*op, Box::new(replace_with(a, rest, new)), b.clone())
            } else {
                Expr::Bin(*op, a.clone(), Box::new(replace_with(b, rest, new)))
            }
        }
        Expr::Call(f, args) => {
            let mut args = args.clone();
            if let Some(a) = args.get_mut(i as usize) {
                *a = replace_with(a, rest, new);
            }
            Expr::Call(*f, args)
        }
        other => other.clone(),
    }
}

// ------------------------------------------------------------ period

fn period(fx: &Fx, rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    if rc.value.is_null() {
        // Not periodic: a domain no shift maps to itself, a monotone piece
        // reaching ±∞, or a tail with a limit and two values apart, or one
        // going to ±∞.
        let dom = all.iter().find(|r| r.name == "domain");
        let dom_not_line = dom.is_some_and(|d| {
            d.status == "Certified"
                && d.value
                    .get("excluded")
                    .and_then(Value::as_array)
                    .is_some_and(|e| e.is_empty())
                && d.value
                    .get("pieces")
                    .and_then(Value::as_array)
                    .is_some_and(|p| {
                        !(p.len() == 1
                            && p[0].get("lo") == Some(&Value::from("NegInf"))
                            && p[0].get("hi") == Some(&Value::from("PosInf")))
                    })
        });
        if dom_not_line {
            return Ok(());
        }
        let mono_tail = all
            .iter()
            .find(|r| r.name == "monotonicity")
            .is_some_and(|m| {
                m.status == "Certified"
                    && m.value.as_array().is_some_and(|ps| {
                        ps.iter().any(|p| {
                            p.get("on").is_some_and(|on| {
                                on.get("lo") == Some(&Value::from("NegInf"))
                                    || on.get("hi") == Some(&Value::from("PosInf"))
                            }) && p.get("dir").and_then(Value::as_str) != Some("Constant")
                        })
                    })
            });
        if mono_tail {
            return Ok(());
        }
        // f′ ≡ 0 on a stretch reaching −∞ and on one reaching +∞ (the
        // line itself, or two tails either side of a kink's box: sign(|x| +
        // 1)): f is constant on each tail. A periodic f takes at each x the
        // value it has at x + kP far out on either side, so it is constant
        // throughout, on the whole line (its domain holds every x − kP too):
        // every shift a period, none least (the corpus's convention:
        // "none"). Otherwise it is not periodic. Either way no least period.
        let flat = |c: &Claim, left: bool| {
            matches!(c, Claim::Value { x, of: Subject::F(1), lo, hi }
                if *lo == 0.0 && *hi == 0.0
                    && if left { x.0 == f64::NEG_INFINITY } else { x.1 == f64::INFINITY })
        };
        if rc.claims.iter().any(|c| flat(c, true)) && rc.claims.iter().any(|c| flat(c, false)) {
            out.notes
                .push("f′ ≡ 0 on both tails: no least period".into());
            return Ok(());
        }
        // A tail: to ±∞ (f′ beyond a nonzero c), or a limit with two
        // values apart. (A tail, not a pole: tan is unbounded and periodic.
        // And a limit, not a bound: sin is within [−1, 1] far out, with
        // two values apart, and periodic.)
        let to_inf = [Side::Left, Side::Right]
            .iter()
            .any(|&s| tail_infinite(&rc.claims, s, true) || tail_infinite(&rc.claims, s, false));
        let vals: Vec<(f64, f64, f64)> = rc
            .claims
            .iter()
            .filter_map(|c| match c {
                Claim::Value {
                    x: bx,
                    of: Subject::F(0),
                    lo,
                    hi,
                } if bx.0 == bx.1 => Some((bx.0, *lo, *hi)),
                _ => None,
            })
            .collect();
        let apart = vals.iter().any(|a| vals.iter().any(|b| a.2 < b.1));
        // A limit at a tail: by the tree's structure, a rational f's exact
        // one, f bounded and monotone there, or the simplifier's with f's
        // bands closing in on it.
        let limit = [(Side::Left, At::NegInf, "−∞"), (Side::Right, At::PosInf, "+∞")]
            .iter()
            .any(|&(s, toward, at)| {
                let bands = tail_bands(&rc.claims, s);
                let fact = rc.claims.iter().find_map(|c| match c {
                    Claim::Simplifier(f)
                        if f.starts_with("f → ") && f.ends_with(&format!("as x → {at}")) =>
                    {
                        fact_limit(f)
                    }
                    _ => None,
                });
                matches!(limit_at(&rc.claims, toward, false), Some(Toward::In(..)))
                    || rc.claims.iter().any(|c| {
                        matches!(c, Claim::Simplifier(f) if f.starts_with(claims::RATIONAL)
                            && f.ends_with(&format!("as x → {at}")) && fact_limit(f).is_some())
                    })
                    || (!bands.is_empty() && tail_mono(&rc.claims, s).is_some())
                    || fact.is_some_and(|y| borne_out(&bands, &y, "the limit").is_ok())
            });
        if !(to_inf || (limit && apart)) {
            out.problems
                .push("not periodic, but the claims don't show it".into());
        }
        return Ok(());
    }
    let p = enc(&rc.value)?;
    // The least period is a positive number: a negative shift f repeats
    // under is no least period, and below it the bound n on a smaller one
    // was negative, with no prime to try (review 18's audit).
    if p.lo.is_nan() || p.lo <= 0.0 {
        out.problems.push(format!(
            "the period [{:e}, {:e}] is not shown positive",
            p.lo, p.hi
        ));
        return Ok(());
    }
    // A period: the simplifier's fact (weak), then the least: every
    // P/q for a prime q up to the bound n shown no period.
    let fact = rc.claims.iter().find_map(|c| match c {
        Claim::Simplifier(f) if f.starts_with("f(x + ") => Some(f.clone()),
        _ => None,
    });
    let Some(fact) = fact else {
        out.problems
            .push("a period with no fact that it is one".into());
        return Ok(());
    };
    // A period maps each excluded family onto itself: P is a multiple of
    // its period.
    let fams: Vec<Enc> = all
        .iter()
        .find(|r| r.name == "domain")
        .and_then(|d| d.value.get("excluded"))
        .and_then(Value::as_array)
        .map(|e| {
            e.iter()
                .filter_map(|f| f.get("period").and_then(|p| enc(p).ok()))
                .collect()
        })
        .unwrap_or_default();
    for q in &fams {
        let n = (p.mid() / q.mid()).round();
        let fits = n >= 1.0 && n * q.lo <= p.hi * (1.0 + 1e-12) && p.lo * (1.0 - 1e-12) <= n * q.hi;
        if !fits {
            out.problems.push(format!(
                "the period [{:e}, {:e}] is no multiple of an excluded family's [{:e}, {:e}]",
                p.lo, p.hi, q.lo, q.hi
            ));
        }
    }
    // The listed period is the fact's.
    iv::set_prec(160);
    let stated = fact
        .strip_prefix("f(x + ")
        .and_then(|r| r.split_once(") = f(x)"))
        .and_then(|(q, _)| claims::pi_q(q));
    match stated {
        Some(v) if v.ge(p.lo) && v.le(p.hi) => {}
        Some(v) => out.problems.push(format!(
            "the period is listed in [{:e}, {:e}], the fact says {}",
            p.lo,
            p.hi,
            claims::show(&v)
        )),
        None => out.problems.push(format!("unread period fact: {fact}")),
    }
    out.notes
        .push("P is a period: the simplifier's fact, tested at points".into());
    // The bound n: from f′'s values over one period (L) and two values of
    // f apart (V): n ≤ P·L/V; or, with one excluded family of period q, P/q.
    let dprime: Vec<(B, f64, f64)> = rc
        .claims
        .iter()
        .filter_map(|c| match c {
            Claim::Value {
                x,
                of: Subject::F(1),
                lo,
                hi,
            } => Some((*x, *lo, *hi)),
            _ => None,
        })
        .collect();
    let n = if !dprime.is_empty() {
        // They must cover one period.
        let boxes: Vec<B> = dprime.iter().map(|(x, _, _)| *x).collect();
        let start = boxes.iter().map(|b| b.0).fold(f64::INFINITY, f64::min);
        if let Some(h) = uncovered((start, start + p.hi), &boxes) {
            out.problems.push(format!(
                "f′'s bounds leave [{:e}, {:e}] of a period out",
                h.0, h.1
            ));
        }
        let l = dprime
            .iter()
            .map(|(_, lo, hi)| lo.abs().max(hi.abs()))
            .fold(0.0, f64::max);
        let pts: Vec<(f64, f64)> = rc
            .claims
            .iter()
            .filter_map(|c| match c {
                Claim::Value {
                    x,
                    of: Subject::F(0),
                    lo,
                    hi,
                } if x.0 == x.1 => Some((*lo, *hi)),
                _ => None,
            })
            .collect();
        let top = pts.iter().map(|v| v.0).fold(f64::NEG_INFINITY, f64::max);
        let bottom = pts.iter().map(|v| v.1).fold(f64::INFINITY, f64::min);
        let v = top - bottom;
        if !(v > 0.0 && l > 0.0) {
            out.problems
                .push("no bound on a smaller period from the claims".into());
            return Ok(());
        }
        (p.hi * l / v * (1.0 + 1e-12)).floor()
    } else {
        let fams: Vec<Enc> = all
            .iter()
            .find(|r| r.name == "domain")
            .and_then(|d| d.value.get("excluded"))
            .and_then(Value::as_array)
            .map(|e| {
                e.iter()
                    .filter_map(|f| f.get("period").and_then(|p| enc(p).ok()))
                    .collect()
            })
            .unwrap_or_default();
        // (A family period not shown positive bounds nothing: P over a
        // negative low end was a negative bound, with no prime to try.)
        if fams.len() != 1 || fams[0].lo.is_nan() || fams[0].lo <= 0.0 {
            out.problems
                .push("no bound on a smaller period from the claims".into());
            return Ok(());
        }
        (p.hi / fams[0].lo * (1.0 + 1e-12)).floor()
    };
    // A bound the certifier itself never passes (10⁶, and no NaN): one read
    // off the claims past it is no bound a sieve reaches. (Review 18's
    // audit: cast to u64, a family period of 10⁻³⁰⁰ under P made it 2⁶⁴ − 1
    // primes to try, and the replay never returned.)
    if n.is_nan() || n > 1e6 {
        out.problems.push(format!(
            "a smaller period is ruled out only by the primes to {n:e}: past any sieve"
        ));
        return Ok(());
    }
    // Each prime q ≤ n: two values, at x and x + P/q, apart. (n ≤ 10⁶, a
    // whole number or below 2: exact as a u64.)
    let primes: Vec<u64> = (2..=(n.max(1.0) as u64))
        .filter(|&q| (2..).take_while(|d| d * d <= q).all(|d| q % d != 0))
        .collect();
    let vals: Vec<(B, f64, f64)> = rc
        .claims
        .iter()
        .filter_map(|c| match c {
            Claim::Value {
                x,
                of: Subject::F(0),
                lo,
                hi,
            } => Some((*x, *lo, *hi)),
            _ => None,
        })
        .collect();
    for q in primes {
        iv::set_prec(160);
        let shift = iv::div(&Iv::of2(p.lo, p.hi), &Iv::of(q as f64));
        let found = vals.iter().any(|(a, al, ah)| {
            a.0 == a.1
                && vals.iter().any(|(b, bl, bh)| {
                    // b's box holds x + P/q.
                    let s = iv::add(&Iv::of(a.0), &shift);
                    let (sl, sh) = claims::outward(&s);
                    b.0 <= sl && sh <= b.1 && (ah < bl || bh < al)
                })
        });
        if !found {
            out.problems
                .push(format!("P/{q} is not shown to be no period"));
        }
    }
    let _ = fx;
    Ok(())
}

// ------------------------------------------------------------ limits

/// f′'s strict sign on the tail (`Some(true)`: rising), and from where.
fn tail_mono(claims: &[Claim], side: Side) -> Option<(bool, f64)> {
    tail_mono_of(claims, side, 1)
}

/// f⁽ᵏ⁾'s strict sign on the tail (`Some(true)`: positive), and from
/// where: f⁽ᵏ⁻¹⁾ is strictly monotone there.
fn tail_mono_of(claims: &[Claim], side: Side, k: usize) -> Option<(bool, f64)> {
    // The widest such tail (from −∞ on the right: the whole line).
    let right = side == Side::Right;
    claims
        .iter()
        .filter_map(|c| match c {
            Claim::TailBeyond {
                side: s,
                from,
                of: Subject::F(o),
                c,
                above,
            } if *s == side && *c == 0.0 && *o == k => Some((*above, *from)),
            _ => None,
        })
        .reduce(|a, b| {
            let wider = if right { b.1 < a.1 } else { b.1 > a.1 };
            if wider { b } else { a }
        })
}

/// x lies on the tail that starts at `from` (on the right: x ≥ from).
fn on_tail(x: f64, from: f64, right: bool) -> bool {
    if right { x >= from } else { x <= from }
}

/// The bands f's own enclosures put it in on a tail, as (|M|, lo, hi):
/// a `TailValue` on [M, ∞), or (f monotone there) its value at M and a
/// bound beyond which it stays on [M, ∞) — moving away from f(M) one way
/// out along the tail, held by the bound the other way. Out along the
/// tail.
fn tail_bands(claims: &[Claim], side: Side) -> Vec<(f64, f64, f64)> {
    let right = side == Side::Right;
    let mut out: Vec<(f64, f64, f64)> = claims
        .iter()
        .filter_map(|c| match c {
            Claim::TailValue {
                side: s,
                from,
                lo,
                hi,
            } if *s == side => Some((from.abs(), *lo, *hi)),
            _ => None,
        })
        .collect();
    if let Some((rising, start)) = tail_mono(claims, side) {
        let away_up = rising == right;
        for c in claims {
            let Claim::Value {
                x,
                of: Subject::F(0),
                lo,
                hi,
            } = c
            else {
                continue;
            };
            let m = x.0;
            if x.0 != x.1 || (m > 0.0) != right || !on_tail(m, start, right) {
                continue;
            }
            let bound = claims.iter().find_map(|d| match d {
                Claim::TailBeyond {
                    side: s,
                    from,
                    of: Subject::F(0),
                    c,
                    above,
                } if *s == side && *from == m && *above != away_up => Some(*c),
                _ => None,
            });
            if let Some(b) = bound {
                out.push(if away_up {
                    (m.abs(), *lo, b)
                } else {
                    (m.abs(), b, *hi)
                });
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// Whether the claims show f → +∞ (`up`) or −∞ on the tail: f′ beyond a
/// nonzero bound of the matching sign, f′'s sign chain (f′ moving away
/// from 0), or the limit by the tree's structure. (Bounds on f over tails
/// [M, ∞) that "grew" with M counted too, and f's values at points of a
/// monotone tail: a finite list of either shows no limit. −1/x on [1, ∞)
/// is above −1, then −1/10, then −1/100. One genuine certificate of the
/// corpus and pool rested on such bounds, beside a pole: the vertical row
/// derives that pole itself now.)
fn tail_infinite(claims: &[Claim], side: Side, up: bool) -> bool {
    let right = side == Side::Right;
    let proof = claims.iter().any(|c| match c {
        Claim::TailBeyond {
            side: s,
            of: Subject::F(1),
            c,
            above,
            ..
        } if *s == side && *c != 0.0 => {
            (*above && *c > 0.0 || !*above && *c < 0.0) && (*above == right) == up
        }
        // (f′ moving away from c on its side of it: away from 0 only for
        // a c on that side of 0, or 0 itself. 1/x's f′ rises from above
        // −5 on the right tail, toward 0.)
        Claim::TailChain {
            side: s,
            of: Subject::F(1),
            c,
            above,
            ..
        } if *s == side && (if *above { *c >= 0.0 } else { *c <= 0.0 }) => {
            (*above == right) == up
        }
        _ => false,
    });
    let at = if right { At::PosInf } else { At::NegInf };
    let structure =
        limit_at(claims, at, false) == Some(if up { Toward::PosInf } else { Toward::NegInf });
    structure || proof
}

/// What a `Limit` claim says f (or f/x, `over_x`) tends to as x → `at`.
fn limit_at(claims: &[Claim], at: At, over_x: bool) -> Option<Toward> {
    claims.iter().find_map(|c| match c {
        Claim::Limit {
            at: a,
            over_x: o,
            to,
        } if *a == at && *o == over_x => Some(*to),
        _ => None,
    })
}

/// Whether the claims show f → ±∞ beside the very double p: its one-sided
/// limit there by the tree's structure. (Bounds on boxes from the double
/// next to p that "grew" as the boxes shrank counted too: (x + 1)/(x² − 1)
/// is above −10¹⁵, then −10⁹, then −2252 on boxes closing in on its hole
/// at −1, where it tends to −1/2. Round 19's follow-up.)
fn side_grows(claims: &[Claim], p: f64, right: bool) -> bool {
    let at = if right { At::Right(p) } else { At::Left(p) };
    matches!(
        limit_at(claims, at, false),
        Some(Toward::PosInf | Toward::NegInf)
    )
}

/// f's bands on boxes beside p (from the right, or the left, p left
/// out), as (width, lo, hi), the boxes shrinking: a `Beyond` claim above a
/// bound and one below a bound on the same box.
fn side_bands(claims: &[Claim], p: f64, right: bool) -> Vec<(f64, f64, f64)> {
    let near = |x: &B| {
        if right {
            x.0 == p.next_up()
        } else {
            x.1 == p.next_down()
        }
    };
    let bound = |x: &B, up: bool| {
        claims.iter().find_map(|c| match c {
            Claim::Beyond {
                x: y,
                of: Subject::F(0),
                c,
                above,
            } if y == x && *above == up => Some(*c),
            _ => None,
        })
    };
    let mut out: Vec<(f64, f64, f64)> = Vec::new();
    for c in claims {
        if let Claim::Beyond {
            x,
            of: Subject::F(0),
            above: true,
            c: lo,
        } = c
            && near(x)
            && let Some(hi) = bound(x, false)
            && !out.iter().any(|o| o.0 == x.1 - x.0)
        {
            out.push((x.1 - x.0, *lo, hi));
        }
    }
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    out
}

/// The limit bands bear out `y`: two or more, each holding it, and
/// settling — the last narrow (10⁻⁹ of it), or three or more, the last
/// three none wider than the one before, the last a hundredth as wide as
/// the widest; their intersection, or why not.
fn borne_out(bands: &[(f64, f64, f64)], y: &Enc, what: &str) -> Result<Enc, String> {
    if bands.len() < 2 {
        return Err(format!("{what} with no bands of f's values to bear it out"));
    }
    for (_, lo, hi) in bands {
        if !(y.lo <= *hi && *lo <= y.hi) {
            return Err(format!(
                "{what} is listed in [{:e}, {:e}], outside f's band [{lo:e}, {hi:e}]",
                y.lo, y.hi
            ));
        }
    }
    let (_, llo, lhi) = bands[bands.len() - 1];
    let w: Vec<f64> = bands.iter().map(|b| b.2 - b.1).collect();
    let n = w.len();
    let shrinking = n >= 3
        && w[n - 3..].windows(2).all(|p| p[1] <= p[0])
        && w[n - 1] <= 1e-2 * w.iter().copied().fold(0.0, f64::max);
    if lhi - llo > 1e-9 * y.lo.abs().max(y.hi.abs()).max(1.0) && !shrinking {
        return Err(format!(
            "{what}: f's bands don't narrow to it ([{llo:e}, {lhi:e}] last)"
        ));
    }
    let lo = bands.iter().map(|b| b.1).fold(f64::NEG_INFINITY, f64::max);
    let hi = bands.iter().map(|b| b.2).fold(f64::INFINITY, f64::min);
    Ok(Enc { lo, hi })
}

/// f′ on the tail shown within bands about 0 narrowing to 10⁻¹² (f′ → 0,
/// so f/x → 0): from bounds on f′ over tails, or, f′ monotone there (f″ of
/// one sign), from its values at points of the tail, beyond which it
/// moves. (Evidence for the simplifier's fact, which is the claim.)
fn slope_to_zero(claims: &[Claim], side: Side) -> bool {
    let right = side == Side::Right;
    let bs: Vec<(f64, f64, bool)> = claims
        .iter()
        .filter_map(|c| match c {
            Claim::TailBeyond {
                side: s,
                from,
                of: Subject::F(1),
                c,
                above,
            } if *s == side && *c != 0.0 => Some((from.abs(), *c, *above)),
            _ => None,
        })
        .collect();
    // f′'s values at points of the tail where f′ is monotone, as (|x|,
    // lo, hi), and whether it moves up out along the tail.
    let (points, away_up) = match tail_mono_of(claims, side, 2) {
        Some((rising, start)) => (
            claims
                .iter()
                .filter_map(|c| match c {
                    Claim::Value {
                        x,
                        of: Subject::F(1),
                        lo,
                        hi,
                    } if x.0 == x.1 && (x.0 > 0.0) == right && on_tail(x.0, start, right) => {
                        Some((x.0.abs(), *lo, *hi))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            rising == right,
        ),
        None => (Vec::new(), false),
    };
    // The farthest tail with a bound each way: two bounds there, or
    // one and f′'s value at its start, moving towards it.
    let mut bands: Vec<(f64, f64, f64)> = Vec::new();
    for &(m, _, _) in &bs {
        let above = bs
            .iter()
            .filter(|b| b.0 == m && b.2)
            .map(|b| b.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let below = bs
            .iter()
            .filter(|b| b.0 == m && !b.2)
            .map(|b| b.1)
            .fold(f64::INFINITY, f64::min);
        let at = points.iter().find(|p| p.0 == m);
        let lo = match at {
            Some(p) if away_up => above.max(p.1),
            _ => above,
        };
        let hi = match at {
            Some(p) if !away_up => below.min(p.2),
            _ => below,
        };
        bands.push((m, lo, hi));
    }
    bands
        .iter()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .is_some_and(|b| b.1 >= -1e-12 && b.2 <= 1e-12)
}

/// f″ beyond a nonzero bound on the tail (a `TailBeyond` or a sign chain
/// on f″ past c ≠ 0 of c's sign): f′ moves past f′(M) + c·(x − M), so
/// |f/x| → ∞ and no line is approached.
fn bends_away(claims: &[Claim], side: Side) -> bool {
    claims.iter().any(|c| match c {
        Claim::TailBeyond {
            side: s,
            of: Subject::F(2),
            c,
            above,
            ..
        }
        | Claim::TailChain {
            side: s,
            of: Subject::F(2),
            c,
            above,
            ..
        } if *s == side => (*above && *c > 0.0) || (!*above && *c < 0.0),
        _ => false,
    })
}

// ------------------------------------------------------------ asymptotes

fn vertical(fx: &Fx, rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    let listed: Vec<(Enc, Option<Enc>)> = list(&rc.value)?
        .iter()
        .map(spot)
        .collect::<Result<_, _>>()?;
    let periods: Vec<Enc> = listed.iter().filter_map(|(_, p)| *p).collect();
    families_follow("a vertical asymptote", rc, &periods, out);
    repeats_listed("a vertical asymptote", rc, &listed, out);
    for (x, _) in &listed {
        // The listed enclosure is the box of a claim that f is unbounded
        // at a point in it (which the claim's check places there), or the
        // very double beside which f is shown to grow. (A double inside
        // such a claim's wider box is not where the pole is: π/2 listed at
        // the double nearest it passed for tan's. Review 18's follow-up.)
        // (Or the replay's own derivation at a double, between the
        // doubles beside it: f = a/b with b(p) = 0 and a away from 0, ln of
        // something vanishing there, …, by `claims`' rules for a pole.)
        let own = || {
            x.lo == x.hi && {
                let about = Claim::Unbounded {
                    near: (x.lo.next_down(), x.lo.next_up()),
                    at: (x.lo, x.lo),
                };
                claims::check(fx, &about, &[]).class == Class::Strong
            }
        };
        let unbounded = rc
            .claims
            .iter()
            .any(|c| matches!(c, Claim::Unbounded { at, .. } if at.0 == x.lo && at.1 == x.hi))
            || (x.lo == x.hi
                && (side_grows(&rc.claims, x.lo, true) || side_grows(&rc.claims, x.lo, false)))
            || own();
        if !unbounded {
            out.problems.push(format!(
                "a vertical asymptote at {:e} with no claim it is one",
                x.mid()
            ));
        }
    }
    // Every excluded point (an undefined double between pieces of the
    // walk, or a gap holding one) classified: unbounded (listed), bounded
    // (a value near it, a hole filled) or by one-sided limits.
    let mut excluded: Vec<Enc> = Vec::new();
    for c in &rc.claims {
        if let Claim::Unbounded { at, .. } | Claim::Removable { at, .. } = c {
            excluded.push(Enc { lo: at.0, hi: at.1 });
        }
        // f bounded near a point: no asymptote may be listed there.
        if let Claim::Bounded { at, .. } = c
            && listed.iter().any(|(x, _)| x.lo <= at.1 && at.0 <= x.hi)
        {
            out.problems.push(format!(
                "an asymptote is listed at {:e}, where f is bounded",
                at.0
            ));
        }
    }
    let is_listed = |e: &Enc| {
        // (Listed itself, or a repeat of a listed family's. A row over one
        // period was let off this altogether: tan's poles could all be
        // left out.)
        listed.iter().any(|(x, _)| x.contains(e))
            || listed
                .iter()
                .any(|(x, p)| p.is_some_and(|p| meets_member(x, &p, e)))
    };
    for e in &excluded {
        let unb = rc
            .claims
            .iter()
            .any(|c| matches!(c, Claim::Unbounded { at, .. } if at.0 == e.lo && at.1 == e.hi));
        if unb && !is_listed(e) {
            out.problems.push(format!(
                "f is unbounded at {:e}, but no asymptote is listed there",
                e.mid()
            ));
        }
    }
    if complete(rc) {
        no_other_asymptote(fx, rc, all, &is_listed, out);
    }
    Ok(())
}

/// A complete vertical row lists every asymptote: nowhere else in its
/// region does |f| grow without bound beside a point. The row's claims
/// place every x of the region (within the domain) in a box where f is
/// valid, at a point where f is undefined, or in a gap beside one; then
///
/// * on a box where f is valid it is bounded near each point: a value
///   claim bounds it there, or f is continuous on the box (built without
///   steps, so continuous wherever defined; else shown so on the box);
/// * each point where f is undefined, from each side the domain reaches
///   it, is at a listed asymptote, or f is shown bounded beside it: a
///   `Bounded` or `Removable` claim, a finite one-sided limit by the
///   tree's structure, a band (bounds above and below) on the box from the
///   double beside it, or the replay's own continuous extension of f at
///   the point; a gap that holds an excluded point between two doubles
///   has a claim that places it, unbounded (and listed) or bounded.
///
/// The reals strictly between an undefined double and the double beside it
/// (a `Gap`) are the one place nothing is derived: no row shows that f has
/// no second excluded point there.
///
/// (The row was checked only where it listed something: "none" passed for
/// any f with no claims at all, x³ − 2x + 1/(x − 1) among them. Round 19's
/// mutation tester.)
fn no_other_asymptote(
    fx: &Fx,
    rc: &RowCert,
    all: &[RowCert],
    is_listed: &dyn Fn(&Enc) -> bool,
    out: &mut RowResult,
) {
    if !all
        .iter()
        .any(|r| r.name == "domain" && r.status == "Certified")
    {
        out.problems
            .push("the domain is not decided, yet the row is complete".into());
        return;
    }
    if claims::unmodelled(&fx.f) {
        return;
    }
    // Where the claims say f is valid, with a bound if they give one.
    let f0 = Subject::F(0);
    let mut w = Walk::default();
    let mut valid: Vec<(B, bool)> = Vec::new();
    for c in &rc.claims {
        match c {
            Claim::Beyond { x, of, .. }
            | Claim::NoCross { x, of, .. }
            | Claim::Factors { x, of, .. }
            | Claim::OneCross { x, of, .. }
            | Claim::ExactAt { x, of, .. }
            | Claim::Touch { x, of, .. }
                if *of == f0 =>
            {
                valid.push((*x, false));
            }
            Claim::Value { x, of, lo, hi } if *of == f0 => {
                valid.push((*x, lo.is_finite() && hi.is_finite()));
            }
            Claim::TailBeyond { side, from, of, .. } | Claim::TailChain { side, from, of, .. }
                if *of == f0 =>
            {
                valid.push((tail(*side, *from), false));
            }
            Claim::TailValue { side, from, lo, hi } => {
                valid.push((tail(*side, *from), lo.is_finite() && hi.is_finite()));
            }
            Claim::Undefined(x) => {
                w.holes.push(*x);
                w.solid.push(*x);
            }
            Claim::Gap(x) => w.gaps.push(*x),
            _ => {}
        }
    }
    w.solid.extend(valid.iter().map(|(b, _)| *b));
    w.boxes = w.solid.iter().chain(&w.gaps).copied().collect();
    coverage(rc, all, &w.boxes, out);
    gap_ends(fx, rc, all, &w, 0, out);
    // Bounded near each point of a box where f is valid.
    let mut steps = false;
    fx.f.visit(&mut |e| {
        use graphing::ast::{Expr, Func};
        steps |= matches!(
            e,
            Expr::Call(
                Func::Floor | Func::Ceil | Func::Round | Func::Sign | Func::Mod,
                _
            )
        );
    });
    if steps {
        for (b, bounded) in &valid {
            let within = valid
                .iter()
                .any(|(v, vb)| *vb && v.0 <= b.0 && b.1 <= v.1);
            if *bounded || within {
                continue;
            }
            if claims::check(fx, &Claim::Continuous(*b), &[]).class != Class::Strong {
                out.problems.push(format!(
                    "f is not shown continuous or bounded on [{:e}, {:e}]: an asymptote there \
                     is not ruled out",
                    b.0, b.1
                ));
                break;
            }
        }
    }
    // Each excluded point of the region, from each side the domain reaches
    // it: at a listed asymptote, or f shown bounded beside it.
    let Some(rg) = &rc.region else { return };
    let ts = targets(rg, all);
    // (p, from the right): the ends of the boxes where f is undefined.
    let mut sides: Vec<(f64, bool)> = Vec::new();
    for u in &w.holes {
        if u.1.is_finite() && ts.iter().any(|t| t.0 <= u.1 && u.1 < t.1) {
            sides.push((u.1, true));
        }
        if u.0.is_finite() && ts.iter().any(|t| t.0 < u.0 && u.0 <= t.1) {
            sides.push((u.0, false));
        }
    }
    // f bounded between p and the double beside it, where it is defined
    // (the domain row's): a removable point (its claim's check: f's limit
    // there), or the replay's own enclosure of f over those reals. (A
    // bound on the box from that next double on says nothing of them: 1/(x
    // − 1) is below 5·10¹⁵ on [1 + 2⁻⁵², 2]. Such bands, and a `Bounded`
    // claim, which leaves the same reals out, passed for "no asymptote at
    // 1" once the claim that it is one was left out.)
    let removable = |p: f64| {
        rc.claims.iter().any(
            |c| matches!(c, Claim::Removable { at, .. } if at.0 <= p && p <= at.1),
        )
    };
    let own_bound = |p: f64, right: bool| {
        let v = over_open_gap(fx, p, right);
        !v.empty && v.bounded()
    };
    for (p, right) in sides {
        let at = Enc { lo: p, hi: p };
        if is_listed(&at) {
            continue;
        }
        let toward = if right { At::Right(p) } else { At::Left(p) };
        let finite = matches!(limit_at(&rc.claims, toward, false), Some(Toward::In(..)));
        // (The replay's own: f's continuous extension at p, by series.)
        let own = || {
            iv::set_prec(160);
            claims::limit_at(fx, p).is_some()
        };
        if finite || removable(p) || own() || own_bound(p, right) {
            continue;
        }
        let unbounded = rc
            .claims
            .iter()
            .any(|c| matches!(c, Claim::Unbounded { at, .. } if at.0 <= p && p <= at.1))
            || matches!(
                limit_at(&rc.claims, toward, false),
                Some(Toward::PosInf | Toward::NegInf)
            );
        out.problems.push(if unbounded {
            format!("f is unbounded at {p:e}, but no asymptote is listed there")
        } else {
            format!(
                "f is not shown bounded {} {p:e}, where it is undefined: an asymptote there \
                 is not ruled out",
                if right { "right of" } else { "left of" }
            )
        });
        return;
    }
    // A gap beside no undefined double holds an excluded point between two
    // doubles: one a claim places there, f unbounded at it (and listed) or
    // bounded about it.
    for g in &w.gaps {
        let asked = ts.iter().any(|t| t.0 <= g.1 && g.0 <= t.1);
        let beside = w.holes.iter().any(|u| u.1 == g.0 || u.0 == g.1);
        if !asked || beside {
            continue;
        }
        let placed = rc.claims.iter().find_map(|c| match c {
            Claim::Unbounded { at, .. } if g.0 <= at.0 && at.1 <= g.1 => {
                Some((true, Enc { lo: at.0, hi: at.1 }))
            }
            Claim::Bounded { near, at, .. } | Claim::Removable { near, at, .. }
                if g.0 <= at.0 && at.1 <= g.1 && near.0 <= g.0 && g.1 <= near.1 =>
            {
                Some((false, Enc { lo: at.0, hi: at.1 }))
            }
            _ => None,
        });
        match placed {
            Some((false, _)) => {}
            Some((true, at)) if is_listed(&at) => {}
            Some((true, at)) => {
                out.problems.push(format!(
                    "f is unbounded at {:e}, but no asymptote is listed there",
                    at.mid()
                ));
                return;
            }
            None => {
                out.problems.push(format!(
                    "f in the gap [{:e}, {:e}] is not shown bounded: an asymptote there is not \
                     ruled out",
                    g.0, g.1
                ));
                return;
            }
        }
    }
}

/// A fact that f is a line on its domain (a constant, m·x + b, holes
/// aside): its graph is that line, so it has no asymptote.
fn line_fact(claims: &[Claim]) -> bool {
    claims.iter().any(|c| match c {
        Claim::Simplifier(f) => {
            f.strip_prefix(claims::RATIONAL)
                .is_some_and(|r| r.starts_with("the line "))
                || (f.starts_with("f = ") && f.contains(" on its domain (the simplifier's form)"))
        }
        _ => false,
    })
}

/// f's values at points (`Value` claims on f), as (x, lo, hi), by x.
fn point_values(claims: &[Claim]) -> Vec<(f64, f64, f64)> {
    let mut v: Vec<(f64, f64, f64)> = claims
        .iter()
        .filter_map(|c| match c {
            Claim::Value {
                x,
                of: Subject::F(0),
                lo,
                hi,
            } if x.0 == x.1 => Some((x.0, *lo, *hi)),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

/// f − m·x periodic (a fact) and not constant (two of f's values with
/// f − m·x apart): no line at all.
fn periodic_rest(claims: &[Claim]) -> bool {
    let Some(m) = claims.iter().find_map(|c| match c {
        Claim::Simplifier(f)
            if f.contains(" has period ") && f.ends_with(": no oblique asymptote") =>
        {
            f.strip_prefix("f − ")?
                .split_once("·x = ")
                .map(|(m, _)| m.to_string())
        }
        _ => None,
    }) else {
        return false;
    };
    iv::set_prec(160);
    let Some(mv) = claims::pi_q(&m) else {
        return false;
    };
    let rest: Vec<Iv> = point_values(claims)
        .iter()
        .map(|(x, lo, hi)| iv::sub(&Iv::of2(*lo, *hi), &iv::mul(&mv, &Iv::of(*x))))
        .collect();
    rest.iter().any(|a| rest.iter().any(|b| a.hi < b.lo))
}

/// Two of f's values apart: f is no constant.
fn not_constant(claims: &[Claim]) -> bool {
    let v = point_values(claims);
    v.iter().any(|a| v.iter().any(|b| a.2 < b.1))
}

/// Three of f's values off one line (the slopes between them apart): f is
/// no m·x + b.
fn not_affine(claims: &[Claim]) -> bool {
    let v = point_values(claims);
    let slope = |a: &(f64, f64, f64), b: &(f64, f64, f64)| {
        iv::div(
            &iv::sub(&Iv::of2(b.1, b.2), &Iv::of2(a.1, a.2)),
            &iv::sub(&Iv::of(b.0), &Iv::of(a.0)),
        )
    };
    iv::set_prec(160);
    v.windows(3).any(|w| {
        let (s, t) = (slope(&w[0], &w[1]), slope(&w[1], &w[2]));
        w[0].0 < w[1].0 && w[1].0 < w[2].0 && (s.hi < t.lo || t.hi < s.lo)
    })
}

/// Whether the domain's pieces reach the tail (`None`: no domain row to
/// say; then it may, for all that is known).
fn reaches_tail(all: &[RowCert], side: Side) -> bool {
    domain_pieces(all).is_none_or(|ps| {
        ps.iter().any(|p| match side {
            Side::Right => p.hi == Bound::PosInf,
            Side::Left => p.lo == Bound::NegInf,
        })
    })
}

/// A fact, in any row of the certificate, that f has a period (each is
/// checked as a claim of its row).
fn periodic(all: &[RowCert]) -> bool {
    all.iter()
        .flat_map(|r| &r.claims)
        .any(|c| matches!(c, Claim::Simplifier(f) if f.starts_with("f(x + ")))
}

fn horizontal(fx: &Fx, rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    // A tail with no asymptote listed has none: the domain doesn't reach
    // it, f is a line (its own graph), f → ±∞ there, or f is periodic and
    // no constant (it has no limit at all). A row that listed nothing was
    // let through whatever f does far out: atan's two asymptotes removed,
    // "none" for any f with no claims (round 19's mutation tester).
    for (side, name, at) in [(Side::Left, "Left", "−∞"), (Side::Right, "Right", "+∞")] {
        let n = list(&rc.value)?
            .iter()
            .filter(|h| h.get("side").and_then(Value::as_str) == Some(name))
            .count();
        if n > 1 {
            out.problems
                .push(format!("{n} horizontal asymptotes listed at {at}"));
        }
        let ruled_out = !reaches_tail(all, side)
            || line_fact(&rc.claims)
            || tail_infinite(&rc.claims, side, true)
            || tail_infinite(&rc.claims, side, false)
            || (periodic(all) && not_constant(&rc.claims));
        if n == 0 && !ruled_out {
            out.problems.push(format!(
                "no horizontal asymptote at {at}, but nothing rules one out"
            ));
        }
    }
    // A line's graph is the line itself: no asymptote (a constant, m·x +
    // b, holes aside). One listed needs f shown to be no constant.
    if !list(&rc.value)?.is_empty() {
        if line_fact(&rc.claims) {
            out.problems
                .push("a horizontal asymptote listed for a line, its own graph".into());
        }
        // Two values apart, f unbounded on a tail, or two limits apart.
        let ys: Vec<Enc> = list(&rc.value)?
            .iter()
            .filter_map(|h| enc(h.get("y")?).ok())
            .collect();
        let limits_apart = ys.iter().any(|a| ys.iter().any(|b| a.hi < b.lo));
        let unbounded = [Side::Left, Side::Right]
            .iter()
            .any(|&s| tail_infinite(&rc.claims, s, true) || tail_infinite(&rc.claims, s, false));
        if !(not_constant(&rc.claims) || unbounded || limits_apart) {
            out.problems
                .push("a horizontal asymptote listed, but nothing shows f is no constant".into());
        }
    }
    for item in list(&rc.value)? {
        let side = match item.get("side").and_then(Value::as_str) {
            Some("Left") => Side::Left,
            Some("Right") => Side::Right,
            other => return Err(format!("bad side {other:?}")),
        };
        let y = enc(item.get("y").ok_or("y")?)?;
        // f's own enclosures far out (bands on [M, ∞) narrowing to the
        // limit) bear it out; it exists by f's monotonicity there, or by
        // the simplifier's fact. The fact alone never stands.
        let mono = tail_mono(&rc.claims, side).is_some();
        let at = if side == Side::Right {
            "+∞"
        } else {
            "−∞"
        };
        let fact_text = rc.claims.iter().find_map(|c| match c {
            Claim::Simplifier(f)
                if f.starts_with("f → ") && f.ends_with(&format!("as x → {at}")) =>
            {
                Some(f.as_str())
            }
            _ => None,
        });
        let fact = fact_text.is_some();
        let rational = rc.claims.iter().any(|c| {
            matches!(c, Claim::Simplifier(f) if f.starts_with(claims::RATIONAL) && f.ends_with(&format!("as x → {at}")))
        });
        // What the item says besides its enclosure: the limit written
        // exactly, or a closed form it looks like (not proven: a number
        // inside the enclosure, no more).
        let exact = item.get("exact").and_then(Value::as_str);
        if let Some(l) = item.get("looks_like").filter(|l| !l.is_null()) {
            let l = r(l)?;
            if !(y.lo <= l && l <= y.hi) {
                out.problems.push(format!(
                    "the limit at {at} is said to look like {l:e}, outside its listed [{:e}, {:e}]",
                    y.lo, y.hi
                ));
            }
        }
        if rational {
            // N/D's limit, exactly (its fact checked so): the listed
            // enclosure holds it. The limit is the replay's own, from N/D's
            // degrees and leading coefficients, compared as a rational:
            // read back from the fact's text as doubles and only met with
            // the listing, a limit that is no double was listed as the
            // double beside it (2⁵³ for 2⁵³ + 1, 0.333…3148 for 1/3: review
            // 18's audit).
            let limit = super::algebra::Rat::of(&fx.f, &fx.lits, &fx.vars, fx.unit)
                .and_then(|r| r.limit(side == Side::Right).ok());
            match &limit {
                Some(l) if *l >= y.lo && *l <= y.hi => {}
                _ => out.problems.push(format!(
                    "the limit at {at} is listed in [{:e}, {:e}], not N/D's",
                    y.lo, y.hi
                )),
            }
            if let Some(text) = exact
                && (limit.is_none() || claims::rational(text.trim()) != limit)
            {
                out.problems
                    .push(format!("the limit at {at} is written {text}, not N/D's"));
            }
            continue;
        }
        // By the tree's structure (its claim checked so): the limit lies
        // in the claim's enclosure, and the listing holds all of it; a
        // narrower listing only beside the simplifier's fact, and then it
        // holds the value the fact states (exactly, for a rational: met
        // only, 1/3 was listed at the double below it beside the true fact
        // "f → 1/3". Review 18's follow-up).
        let toward = if side == Side::Right {
            At::PosInf
        } else {
            At::NegInf
        };
        match limit_at(&rc.claims, toward, false) {
            Some(Toward::In(lo, hi)) => {
                let shown = Enc { lo, hi };
                // The replay's own limit by the tree's structure, at 160
                // bits: apart from the listing, the listing is wrong
                // whatever stands beside it.
                let own = own_limit(fx, toward);
                if !(y.lo <= hi && lo <= y.hi) {
                    out.problems.push(format!(
                        "the limit at {at} is listed in [{:e}, {:e}], outside [{lo:e}, {hi:e}]",
                        y.lo, y.hi
                    ));
                } else if let Err(why) = listing_follows(&y, &shown, fact_text) {
                    out.problems.push(format!("the limit at {at}: {why}"));
                } else if own.as_ref().is_some_and(|v| v.hi < y.lo || v.lo > y.hi) {
                    out.problems.push(format!(
                        "the limit at {at} is listed in [{:e}, {:e}], apart from the limit by \
                         f's structure",
                        y.lo, y.hi
                    ));
                }
                if let Some(text) = exact
                    && let Err(why) = exact_form(text, Some(&shown), fact_text, own.as_ref())
                {
                    out.problems
                        .push(format!("the limit at {at} is written {text}: {why}"));
                }
                continue;
            }
            Some(_) => {
                out.problems
                    .push(format!("a limit listed at {at}, where f tends to ±∞"));
                continue;
            }
            None => {}
        }
        match borne_out(
            &tail_bands(&rc.claims, side),
            &y,
            &format!("the limit at {at}"),
        ) {
            Err(why) => out.problems.push(why),
            Ok(b) => {
                if let Err(why) = listing_follows(&y, &b, fact_text) {
                    out.problems.push(format!("the limit at {at}: {why}"));
                }
                if !mono && !fact {
                    out.problems
                        .push(format!("a limit at {at} with nothing showing it exists"));
                }
                if fact {
                    out.notes.push(format!(
                        "the limit at {at}: the simplifier's, borne out by f's bands far out"
                    ));
                }
                if let Some(text) = exact
                    && let Err(why) = exact_form(text, Some(&b), fact_text, None)
                {
                    out.problems
                        .push(format!("the limit at {at} is written {text}: {why}"));
                }
            }
        }
    }
    Ok(())
}

/// The replay's own enclosure, at 160 bits, of f's limit at `at` by the
/// tree's structure ([`super::growth`]); None where that decides no finite
/// one.
fn own_limit(fx: &Fx, at: At) -> Option<Iv> {
    iv::set_prec(160);
    match super::growth::limit(fx, at, false)? {
        super::growth::Goes::To(v) if !v.empty && v.bounded() => Some(v),
        _ => None,
    }
}

/// Why the exact form `text` written for a limit doesn't stand (Ok: it
/// does). The form is what the panel shows as the limit itself, so it is
/// the limit, not a number near it: the same form a fact of the row
/// states (the fact its claim, checked as one); or the point the claims'
/// enclosure `shown` is; or, at 160 bits, a number the replay's own limit
/// by the tree's structure (`own`) can be (about 48 digits: no proof of
/// equality, but no double beside the limit passes for it).
fn exact_form(
    text: &str,
    shown: Option<&Enc>,
    fact: Option<&str>,
    own: Option<&Iv>,
) -> Result<(), String> {
    let stated_by_fact = fact
        .map(|f| f.strip_prefix(claims::RATIONAL).unwrap_or(f))
        .and_then(|f| f.strip_prefix("f → "))
        .and_then(|f| f.split_once(" as x → "))
        .is_some_and(|(l, _)| l.trim() == text.trim());
    if stated_by_fact {
        return Ok(());
    }
    iv::set_prec(160);
    let Some(v) = claims::pi_q(text.trim()) else {
        return Err("no number the replay reads".into());
    };
    if let Some(s) = shown
        && s.lo == s.hi
    {
        return if v.is_exactly(s.lo) {
            Ok(())
        } else {
            Err(format!("the claims show {:e}", s.lo))
        };
    }
    match own {
        Some(o) if !(v.hi < o.lo || o.hi < v.lo) => Ok(()),
        Some(_) => Err("apart from the limit by f's structure".into()),
        None => Err("no fact states it, and no claim shows it exactly".into()),
    }
}

// ------------------------------------------------------------ range

/// f's value at a place, or its limit toward one: an enclosure in the
/// extended reals (±∞ at both ends for an infinite limit), and whether it
/// is a value f takes.
#[derive(Clone, Copy, Debug)]
struct Seen {
    lo: f64,
    hi: f64,
    taken: bool,
}

/// An end of a stretch of the domain on which f is continuous.
#[derive(Clone, Copy, Debug)]
enum Brink {
    /// f is defined there (a closed end, an edge of the period's window).
    Taken(B),
    /// An excluded point or an open end, approached from its right (true)
    /// or its left.
    Toward(B, bool),
    Tail(Side),
}

impl Brink {
    fn at(&self) -> B {
        match self {
            Brink::Taken(b) | Brink::Toward(b, _) => *b,
            Brink::Tail(Side::Left) => (f64::NEG_INFINITY, f64::NEG_INFINITY),
            Brink::Tail(Side::Right) => (f64::INFINITY, f64::INFINITY),
        }
    }
}

/// The values f takes on one stretch where it is continuous: every value
/// strictly between `lo` and `hi` for certain (and `lo` or `hi` itself
/// where `closed`), none outside `hull`; with what was seen at its ends
/// and turns.
struct Image {
    lo: f64,
    hi: f64,
    closed: (bool, bool),
    hull: B,
    seen: Vec<Seen>,
}

/// The range derived, and the listed pieces held to it.
///
/// The domain row's pieces, cut at the excluded points inside them, are
/// the stretches on which f is continuous; the row's walk of f′ (which
/// covers them: [`coverage`], [`gap_ends`], [`holes`]) cuts each into
/// finitely many monotone parts, at f′'s zeros and f's kinks. So on a
/// stretch f's least and greatest values are among: f at those zeros and
/// kinks and at a closed end (taken), and f's limits at an open end, an
/// excluded point or a tail (not taken). Each is enclosed here (`Seen`),
/// from the replay's own evaluation where f is defined and from the
/// row's claims where it is a limit; f is continuous on the stretch, so it
/// takes every value strictly between the least and the greatest. Then:
///
/// * no value falls outside the list: each stretch's hull lies in one
///   listed piece;
/// * the list holds no value f doesn't take: each listed piece, between
///   its ends' enclosures, is covered by the stretches' certain images;
/// * a finite end is closed just when the value there is taken.
///
/// (The row was checked at its ends alone, each against the source it
/// names, and at sample points: √x's range passed as (−∞, ∞), a third of
/// x³ − 2x + 1/(x − 1)'s as everything above it. Round 19's mutation
/// tester.)
fn images(
    fx: &Fx,
    rc: &RowCert,
    all: &[RowCert],
    w: &Walk,
    pieces: &[Piece],
    out: &mut RowResult,
) {
    let Some((ra, rb)) = rc.region.as_ref().and_then(region_box) else {
        return;
    };
    let Some(domain) = domain_pieces(all) else {
        out.problems
            .push("the range is not derived: no domain row places the stretches of f".into());
        return;
    };
    if claims::unmodelled(&fx.f) {
        out.problems
            .push("the range is not derived: f uses a function the replay doesn't model".into());
        return;
    }
    // The excluded places: the boxes where f is undefined, and the gaps
    // that hold an excluded point between two doubles (beside no undefined
    // box), those that overlap as one.
    let mut excluded: Vec<B> = w.holes.clone();
    for g in &w.gaps {
        let beside = w
            .holes
            .iter()
            .any(|u| u.1 == g.0 || u.0 == g.1 || (u.0 <= g.0 && g.1 <= u.1));
        if !beside {
            excluded.push(*g);
        }
    }
    excluded.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<B> = Vec::new();
    for e in excluded {
        match merged.last_mut() {
            Some(l) if e.0 <= l.1 => l.1 = l.1.max(e.1),
            _ => merged.push(e),
        }
    }
    // The stretches.
    let mut stretches: Vec<(Brink, Brink)> = Vec::new();
    for d in &domain {
        let (dlo, dhi) = extent(d);
        if dlo.max(ra) > dhi.min(rb) {
            continue;
        }
        let low = if dlo < ra {
            Brink::Taken((ra, ra))
        } else {
            match d.lo {
                Bound::NegInf => Brink::Tail(Side::Left),
                Bound::PosInf => continue,
                Bound::At { x, closed: true } => Brink::Taken((x.lo, x.hi)),
                Bound::At { x, closed: false } => Brink::Toward((x.lo, x.hi), true),
            }
        };
        let high = if dhi > rb {
            Brink::Taken((rb, rb))
        } else {
            match d.hi {
                Bound::PosInf => Brink::Tail(Side::Right),
                Bound::NegInf => continue,
                Bound::At { x, closed: true } => Brink::Taken((x.lo, x.hi)),
                Bound::At { x, closed: false } => Brink::Toward((x.lo, x.hi), false),
            }
        };
        let mut from = low;
        for e in &merged {
            if e.0 > from.at().1 && e.1 < high.at().0 {
                stretches.push((from, Brink::Toward(*e, false)));
                from = Brink::Toward(*e, true);
            }
        }
        stretches.push((from, high));
    }
    iv::set_prec(160);
    // f over a box where it is defined: the replay's own enclosure, within
    // any value claim on that very box.
    let taken = |b: B| -> Option<Seen> {
        let v = super::series::where_defined(|| fx.series(&fx.f, b.0, b.1, 0)[0].clone());
        let mut seen = (!v.empty && v.bounded()).then(|| {
            let (lo, hi) = claims::outward(&v);
            Seen {
                lo,
                hi,
                taken: true,
            }
        });
        for c in &rc.claims {
            if let Claim::Value {
                x,
                of: Subject::F(0),
                lo,
                hi,
            } = c
                && *x == b
                && lo.is_finite()
                && hi.is_finite()
            {
                seen = Some(match seen {
                    Some(s) => Seen {
                        lo: s.lo.max(*lo),
                        hi: s.hi.min(*hi),
                        taken: true,
                    },
                    None => Seen {
                        lo: *lo,
                        hi: *hi,
                        taken: true,
                    },
                });
            }
        }
        seen.filter(|s| s.lo <= s.hi)
    };
    // f constant on [lo, hi] (f′ ≡ 0 there, no excluded point inside):
    // its one value, within every value claim on a box of it; with none,
    // the replay's own at a point.
    let constant = |lo: f64, hi: f64| -> Option<Seen> {
        let mut seen: Option<Seen> = None;
        for c in &rc.claims {
            if let Claim::Value {
                x,
                of: Subject::F(0),
                lo: l,
                hi: h,
            } = c
                && lo <= x.0
                && x.1 <= hi
                && l.is_finite()
                && h.is_finite()
            {
                seen = Some(match seen {
                    Some(s) => Seen {
                        lo: s.lo.max(*l),
                        hi: s.hi.min(*h),
                        taken: true,
                    },
                    None => Seen {
                        lo: *l,
                        hi: *h,
                        taken: true,
                    },
                });
            }
        }
        seen.filter(|s| s.lo <= s.hi).or_else(|| {
            [0.0, lo, hi, lo + 1.0, hi - 1.0]
                .into_iter()
                .filter(|x| x.is_finite() && lo <= *x && *x <= hi)
                .find_map(|x| taken((x, x)))
        })
    };
    // f constant on a stretch that reaches the edge: its value there.
    let flat_to = |e: &Brink| -> Option<Seen> {
        let b = w.flat.iter().find(|b| match e {
            Brink::Tail(Side::Left) => b.0 == f64::NEG_INFINITY,
            Brink::Tail(Side::Right) => b.1 == f64::INFINITY,
            Brink::Toward(x, true) => b.0 <= x.1.next_up() && x.1 < b.1,
            Brink::Toward(x, false) => b.1 >= x.0.next_down() && x.0 > b.0,
            Brink::Taken(_) => false,
        })?;
        match e {
            Brink::Toward(x, true) => constant(x.1.next_up().max(b.0), b.1),
            Brink::Toward(x, false) => constant(b.0, x.0.next_down().min(b.1)),
            _ => constant(b.0, b.1),
        }
    };
    let infinite = |up: bool| {
        let v = if up {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        };
        Seen {
            lo: v,
            hi: v,
            taken: false,
        }
    };
    let limit = |v: Enc| Seen {
        lo: v.lo,
        hi: v.hi,
        taken: false,
    };
    // f′'s sign on the stretch beside x (f's direction there).
    let rising_beside = |x: f64, right: bool| -> Option<bool> {
        let q = if right { x.next_up() } else { x.next_down() };
        w.signs
            .iter()
            .find(|s| (s.0 <= q && q <= s.1) || (s.0 <= x && x <= s.1))
            .map(|s| s.2)
    };
    let toward = |b: B, right: bool| -> Option<Seen> {
        let point = (b.0 == b.1).then_some(b.0);
        let at = point.map(|p| if right { At::Right(p) } else { At::Left(p) });
        // An infinite limit by the tree's structure.
        match at.and_then(|at| limit_at(&rc.claims, at, false)) {
            Some(Toward::PosInf) => return Some(infinite(true)),
            Some(Toward::NegInf) => return Some(infinite(false)),
            _ => {}
        }
        // A finite one: every enclosure of it that is proven, together.
        let mut shown: Vec<Enc> = Vec::new();
        if let Some(p) = point {
            let side = serde_json::json!({"Side": {"at": p, "right": right}});
            if let Ok(v) = source_value(fx, rc, &side) {
                shown.push(v.enc());
            }
            iv::set_prec(160);
            if let Some(v) = claims::limit_at(fx, p) {
                let (lo, hi) = claims::outward(&v);
                shown.push(Enc { lo, hi });
            }
            // (f's own enclosure between p and the double beside it holds
            // the limit; a band on the box from that double on does not.)
            let v = over_open_gap(fx, p, right);
            if !v.empty && v.bounded() {
                let (lo, hi) = claims::outward(&v);
                shown.push(Enc { lo, hi });
            }
        }
        for c in &rc.claims {
            if let Claim::Removable { at, lo, hi, .. } = c
                && ((at.0 <= b.0 && b.1 <= at.1) || (b.0 <= at.0 && at.1 <= b.1))
            {
                shown.push(Enc { lo: *lo, hi: *hi });
            }
        }
        if !shown.is_empty() {
            let v = Enc {
                lo: shown.iter().map(|e| e.lo).fold(f64::NEG_INFINITY, f64::max),
                hi: shown.iter().map(|e| e.hi).fold(f64::INFINITY, f64::min),
            };
            return (v.lo <= v.hi).then(|| limit(v));
        }
        // A pole: |f| → ∞ there (a claim's proof, or the replay's own at a
        // double), and f monotone beside it, so to +∞ or to −∞: up, coming
        // from the right, where f falls away from it.
        let pole = rc.claims.iter().any(|c| {
            matches!(c, Claim::Unbounded { at, .. }
                if (at.0 <= b.0 && b.1 <= at.1) || (b.0 <= at.0 && at.1 <= b.1))
        }) || point.is_some_and(|p| {
            let about = Claim::Unbounded {
                near: (p.next_down(), p.next_up()),
                at: (p, p),
            };
            claims::check(fx, &about, &[]).class == Class::Strong
        });
        let rising = rising_beside(if right { b.1 } else { b.0 }, right)?;
        pole.then(|| infinite(rising != right))
    };
    let tail = |side: Side| -> Option<Seen> {
        let right = side == Side::Right;
        let at = if right { At::PosInf } else { At::NegInf };
        match limit_at(&rc.claims, at, false) {
            Some(Toward::PosInf) => return Some(infinite(true)),
            Some(Toward::NegInf) => return Some(infinite(false)),
            _ => {}
        }
        for up in [true, false] {
            if tail_infinite(&rc.claims, side, up) {
                return Some(infinite(up));
            }
        }
        // A rational f's own limit, exactly.
        if let Some(r) = super::algebra::Rat::of(&fx.f, &fx.lits, &fx.vars, fx.unit) {
            match r.limit(right) {
                Err(up) => return Some(infinite(up)),
                Ok(q) => return Some(limit(Stated::Exact(q).enc())),
            }
        }
        let name = if right { "Right" } else { "Left" };
        source_value(fx, rc, &serde_json::json!({"Tail": name}))
            .ok()
            .map(|v| limit(v.enc()))
    };
    let see = |e: &Brink| -> Option<Seen> {
        match e {
            Brink::Taken(b) => taken(*b),
            Brink::Toward(b, right) => flat_to(e).or_else(|| toward(*b, *right)),
            Brink::Tail(side) => flat_to(e).or_else(|| tail(*side)),
        }
    };
    let name = |e: &Brink| match e {
        Brink::Taken(b) => format!("at [{:e}, {:e}]", b.0, b.1),
        Brink::Toward(b, right) => format!(
            "{} [{:e}, {:e}]",
            if *right { "right of" } else { "left of" },
            b.0,
            b.1
        ),
        Brink::Tail(Side::Left) => "toward −∞".to_string(),
        Brink::Tail(Side::Right) => "toward +∞".to_string(),
    };
    let mut found: Vec<(B, Vec<Seen>)> = Vec::new();
    for (lo, hi) in &stretches {
        let (a, b) = (lo.at().0, hi.at().1);
        let mut seen = Vec::new();
        for e in [lo, hi] {
            match see(e) {
                Some(s) => seen.push(s),
                None => {
                    out.problems.push(format!(
                        "the range is not derived: f {} is not placed",
                        name(e)
                    ));
                    return;
                }
            }
        }
        // Where f is constant on the stretch.
        for f in &w.flat {
            // (Off the stretch's own ends, where f may be undefined.)
            let lo = f.0.max(if lo.at().0 == a { a.next_up() } else { a });
            let hi = f.1.min(if hi.at().1 == b { b.next_down() } else { b });
            if lo > hi {
                continue;
            }
            match constant(lo, hi) {
                Some(s) => seen.push(s),
                None => {
                    out.problems.push(format!(
                        "the range is not derived: f, constant on [{lo:e}, {hi:e}], is not enclosed"
                    ));
                    return;
                }
            }
        }
        // f′'s zeros and f's kinks on the stretch.
        let inside: Vec<B> = w
            .zeros
            .iter()
            .map(|z| (z.at.lo, z.at.hi))
            .filter(|z| a <= z.0 && z.1 <= b)
            .collect();
        for z in inside {
            match taken(z) {
                Some(s) => seen.push(s),
                None => {
                    out.problems.push(format!(
                        "the range is not derived: f on [{:e}, {:e}] is not enclosed",
                        z.0, z.1
                    ));
                    return;
                }
            }
        }
        found.push(((a, b), seen));
    }
    // Over one period: f at the window's low edge is f a period on, a
    // point of the stretch that holds it (and so for the high edge).
    if let Some(Region::Period(wa, wb)) = rc.region
        && let Some(per) = row_period(rc)
    {
        let q = rug::Rational::from_f64;
        for (edge, sign) in [(wa, 1), (wb, -1)] {
            let (Some(e), Some(plo), Some(phi)) = (q(edge), q(per.lo), q(per.hi)) else {
                continue;
            };
            let (lo, hi) = if sign > 0 {
                (rug::Rational::from(&e + &plo), rug::Rational::from(&e + &phi))
            } else {
                (rug::Rational::from(&e - &phi), rug::Rational::from(&e - &plo))
            };
            let Some(v) = taken((edge, edge)) else {
                continue;
            };
            if let Some((_, seen)) = found.iter_mut().find(|((a, b), _)| *a < lo && hi < *b) {
                seen.push(v);
            }
        }
    }
    let imgs: Vec<Image> = found
        .into_iter()
        .map(|(_, seen)| {
            let least = |f: fn(&Seen) -> f64| seen.iter().map(f).fold(f64::INFINITY, f64::min);
            let most = |f: fn(&Seen) -> f64| seen.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
            let (lo, hi) = (least(|s| s.hi), most(|s| s.lo));
            Image {
                lo,
                hi,
                closed: (
                    seen.iter().any(|s| s.taken && s.hi == lo),
                    seen.iter().any(|s| s.taken && s.lo == hi),
                ),
                hull: (least(|s| s.lo), most(|s| s.hi)),
                seen,
            }
        })
        .collect();
    // No value outside the list.
    let within = |hull: B, p: &Piece| {
        let (a, b) = extent(p);
        a <= hull.0 && hull.1 <= b
    };
    for im in &imgs {
        if !pieces.iter().any(|p| within(im.hull, p)) {
            out.problems.push(format!(
                "f takes values in [{:e}, {:e}] on one stretch, not all inside a listed piece",
                im.hull.0, im.hull.1
            ));
            return;
        }
    }
    for p in pieces {
        let mine: Vec<&Image> = imgs.iter().filter(|im| within(im.hull, p)).collect();
        // No listed value untaken: from the low end's enclosure up to the
        // high end's, covered by the certain images (an exact point taken
        // is one too).
        let (from, to) = (
            match p.lo {
                Bound::At { x, .. } => x.hi,
                _ => f64::NEG_INFINITY,
            },
            match p.hi {
                Bound::At { x, .. } => x.lo,
                _ => f64::INFINITY,
            },
        );
        let (mut reach, mut reached) = (from, true);
        while reach < to {
            // The image that carries on from `reach`, furthest.
            let next = mine
                .iter()
                .filter(|im| {
                    im.lo < im.hi
                        && (im.lo < reach || (im.lo == reach && (reached || im.closed.0)))
                        && im.hi > reach
                })
                .max_by(|x, y| x.hi.total_cmp(&y.hi).then(x.closed.1.cmp(&y.closed.1)));
            let Some(next) = next else {
                out.problems.push(format!(
                    "the listed range holds values about {reach:e} that f is not shown to take"
                ));
                return;
            };
            (reach, reached) = (next.hi, next.closed.1);
        }
        // A finite end closed just when it is taken: the least value of
        // the stretches in the piece is among what was seen no higher than
        // the lowest of their upper bounds.
        let seen: Vec<&Seen> = mine.iter().flat_map(|im| &im.seen).collect();
        for (b, low) in [(p.lo, true), (p.hi, false)] {
            let Bound::At { x, closed } = b else { continue };
            let may: Vec<&&Seen> = if low {
                let top = seen.iter().map(|s| s.hi).fold(f64::INFINITY, f64::min);
                seen.iter().filter(|s| s.lo <= top).collect()
            } else {
                let bottom = seen.iter().map(|s| s.lo).fold(f64::NEG_INFINITY, f64::max);
                seen.iter().filter(|s| s.hi >= bottom).collect()
            };
            if may.is_empty() {
                continue;
            }
            let (all_taken, none_taken) = (may.iter().all(|s| s.taken), may.iter().all(|s| !s.taken));
            if (closed && none_taken) || (!closed && all_taken) {
                out.problems.push(format!(
                    "the range's end in [{:e}, {:e}] is listed {}, yet f {} it",
                    x.lo,
                    x.hi,
                    if closed { "closed" } else { "open" },
                    if closed {
                        "only tends to"
                    } else {
                        "takes"
                    }
                ));
            }
        }
    }
}

/// The range: the listed pieces are the images of f's continuous stretches
/// ([`images`]), and every finite end holds the value its named source
/// gives.
fn range(fx: &Fx, rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    // The monotone pieces' images need a decided derivative walk, not
    // just continuity in the boxes left around unplaced kinks.
    let w = walk(fx, &rc.claims, 1);
    out.problems.extend(w.problems.iter().cloned());
    coverage(rc, all, &w.boxes, out);
    gap_ends(fx, rc, all, &w, 1, out);
    holes(rc, 1, complete(rc), out);
    let pieces: Vec<Piece> = list(&rc.value)?
        .iter()
        .map(piece)
        .collect::<Result<_, _>>()?;
    if out.problems.is_empty() {
        images(fx, rc, all, &w, &pieces, out);
    }
    // Each finite end names where it comes from (`range_ends`: f's value at
    // a point, a tail's limit, a hole's, a one-sided limit; several where
    // it is the hull of ends not told apart). The listed enclosure holds
    // the value each source's claims give: all of a proven enclosure, the
    // number itself where it is known exactly. (Met only, or held by the
    // source's enclosure, an end was listed at the double beside the value:
    // the range of (x + 1)/(3x) leaving out 0.333…3148, not 1/3. Review
    // 18's follow-up.) An end is closed just when its sources are values
    // taken; without its sources neither follows. (An infinite end is
    // [`images`]': a stretch whose image reaches it.)
    let Some(ends) = rc.extra.as_array() else {
        out.problems
            .push("the range's ends name no sources (no range_ends)".into());
        return Ok(());
    };
    if ends.len() != pieces.len() {
        out.problems.push(format!(
            "{} range pieces but {} pairs of end sources",
            pieces.len(),
            ends.len()
        ));
    }
    for (i, p) in pieces.iter().enumerate() {
        for (j, b) in [p.lo, p.hi].into_iter().enumerate() {
            let (x, closed) = match b {
                Bound::NegInf | Bound::PosInf => continue,
                Bound::At { x, closed } => (x, closed),
            };
            let srcs: Vec<&Value> = ends
                .get(i)
                .and_then(|pair| pair.get(j))
                .and_then(Value::as_array)
                .map(|s| s.iter().collect())
                .unwrap_or_default();
            if srcs.is_empty() || srcs.iter().any(|s| s.as_str() == Some("Other")) {
                out.problems.push(format!(
                    "a range end [{:e}, {:e}] with no source named: neither its value nor \
                     whether it is taken follows",
                    x.lo, x.hi
                ));
                continue;
            }
            let taken = srcs.iter().filter(|s| s.get("At").is_some()).count();
            if (closed && taken != srcs.len()) || (!closed && taken != 0) {
                out.problems.push(format!(
                    "a range end at {:e} {} but its sources say otherwise",
                    x.mid(),
                    if closed { "closed" } else { "open" }
                ));
            }
            for src in srcs {
                match source_value(fx, rc, src) {
                    Err(why) => out.problems.push(why),
                    Ok(v) if v.held_by(&x) => {}
                    Ok(v) => {
                        let e = v.enc();
                        out.problems.push(format!(
                            "a range end [{:e}, {:e}] doesn't hold the value its source gives, \
                             in [{:e}, {:e}]",
                            x.lo, x.hi, e.lo, e.hi
                        ));
                    }
                }
            }
        }
    }
    // f at sample points of its domain lies in the listed range.
    let inside = |v: &Iv| -> bool {
        pieces.iter().any(|p| {
            let (a, b) = extent(p);
            v.hi >= a && v.lo <= b
        })
    };
    let s = Subj::new(fx, &Subject::F(0), Tree::Orig).expect("f");
    iv::set_prec(160);
    let mut xs = vec![0.0];
    let mut t = 1e-3;
    while t < 1e7 {
        xs.push(t);
        xs.push(-t);
        t *= 1.7;
    }
    for x in xs {
        let (v, valid) = s.at(x);
        if valid && !v.empty && v.bounded() && !inside(&v) {
            out.problems
                .push(format!(
                    "f({x:e}) ∈ {} is outside the listed range",
                    claims::show(&v)
                ));
            break;
        }
    }
    let _ = split;
    Ok(())
}

/// What a listing must hold: a number known exactly (a rational a fact
/// writes out, the replay's own limit of N/D), or every point of an
/// enclosure a claim proves (the number is somewhere in it).
///
/// A listing is an enclosure too, and the number it lists is in it: so it
/// holds the exact number, or all of the proven enclosure. Met only (an
/// overlap), or held by the proven enclosure, a listing could be a point
/// at a double the number is not: 1/3 listed as [0.333…3148, 0.333…3148]
/// beside the true fact that it is 1/3 (review 18's follow-up).
#[derive(Clone, Debug)]
enum Stated {
    Exact(rug::Rational),
    Within(Enc),
}

impl Stated {
    fn held_by(&self, y: &Enc) -> bool {
        match self {
            Stated::Exact(l) => *l >= y.lo && *l <= y.hi,
            Stated::Within(v) => y.contains(v),
        }
    }

    /// The doubles about it (for a message, or to meet bands with).
    fn enc(&self) -> Enc {
        match self {
            Stated::Exact(l) => {
                iv::set_prec(160);
                let (lo, hi) = claims::outward(&iv::ratio(l.numer(), l.denom()));
                Enc { lo, hi }
            }
            Stated::Within(v) => *v,
        }
    }

    /// The part of it inside the proven enclosure `shown` (both hold the
    /// same number); None if they are apart.
    fn within(&self, shown: &Enc) -> Option<Stated> {
        match self {
            Stated::Exact(l) => (*l >= shown.lo && *l <= shown.hi).then(|| self.clone()),
            Stated::Within(v) => {
                let (lo, hi) = (shown.lo.max(v.lo), shown.hi.min(v.hi));
                (lo <= hi).then_some(Stated::Within(Enc { lo, hi }))
            }
        }
    }
}

/// The finite limit a "f → L as x → …" fact states: exactly, when L is
/// written as a rational; else enclosed (q·πᵏ at 160 bits, rounded out to
/// the doubles about it; or the bracket "in [lo, hi]").
fn fact_stated(f: &str) -> Option<Stated> {
    let f = f.strip_prefix(claims::RATIONAL).unwrap_or(f);
    let (l, _) = f.strip_prefix("f → ")?.split_once(" as x → ")?;
    match claims::rational(l.trim()) {
        Some(q) => Some(Stated::Exact(q)),
        None => fact_limit(f).map(Stated::Within),
    }
}

/// Why the listing `y` doesn't follow from the enclosure `shown` its claims
/// prove (Ok: it does). It holds all of `shown`; or, beside a fact that
/// states the number (`fact`), the part of `shown` the stated value is in,
/// the stated value itself when that is exact.
fn listing_follows(y: &Enc, shown: &Enc, fact: Option<&str>) -> Result<(), String> {
    if y.contains(shown) {
        return Ok(());
    }
    let listed = format!("[{:e}, {:e}]", y.lo, y.hi);
    let Some(stated) = fact.and_then(fact_stated) else {
        return Err(format!("{listed} is narrower than the claims show"));
    };
    match stated.within(shown) {
        None => Err(format!(
            "the fact's value is outside the claims' [{:e}, {:e}]",
            shown.lo, shown.hi
        )),
        Some(s) if s.held_by(y) => Ok(()),
        Some(s) => {
            let e = s.enc();
            Err(format!(
                "{listed} doesn't hold the value the fact states, in [{:e}, {:e}]",
                e.lo, e.hi
            ))
        }
    }
}

/// The finite limit a "f → L as x → …" fact states, enclosed.
fn fact_limit(f: &str) -> Option<Enc> {
    let f = f.strip_prefix(claims::RATIONAL).unwrap_or(f);
    let rest = f.strip_prefix("f → ")?;
    let (l, _) = rest.split_once(" as x → ")?;
    iv::set_prec(160);
    let v = if let Some(b) = l.strip_prefix("in [") {
        let (a, b) = b.strip_suffix(']')?.split_once(", ")?;
        return Some(Enc {
            lo: a.trim().parse().ok()?,
            hi: b.trim().parse().ok()?,
        });
    } else {
        claims::pi_q(l)?
    };
    let (lo, hi) = claims::outward(&v);
    Some(Enc { lo, hi })
}

/// The value a range end's source gives, from the row's claims: what the
/// listed end must hold.
fn source_value(fx: &Fx, rc: &RowCert, src: &Value) -> Result<Stated, String> {
    // The fact stating a limit as x → `at` (a rational f's marked so).
    let fact_at = |at: &str| -> Option<&str> {
        rc.claims.iter().find_map(|c| match c {
            Claim::Simplifier(f)
                if f.strip_prefix(claims::RATIONAL)
                    .unwrap_or(f)
                    .starts_with("f → ")
                    && f.ends_with(&format!("as x → {at}")) =>
            {
                Some(f.as_str())
            }
            _ => None,
        })
    };
    let rational_at = |at: &str| {
        rc.claims.iter().any(|c| {
            matches!(c, Claim::Simplifier(f) if f.starts_with(claims::RATIONAL) && f.ends_with(&format!("as x → {at}")))
        })
    };
    // A limit by the tree's structure (its claim checked so), narrowed by
    // the simplifier's exact value where they meet.
    let structural = |toward: At, at: &str| -> Option<Stated> {
        let Some(Toward::In(lo, hi)) = limit_at(&rc.claims, toward, false) else {
            return None;
        };
        let shown = Enc { lo, hi };
        Some(
            fact_at(at)
                .and_then(fact_stated)
                .and_then(|s| s.within(&shown))
                .unwrap_or(Stated::Within(shown)),
        )
    };
    if let Some(x) = src.get("At") {
        let x = enc(x)?;
        return rc
            .claims
            .iter()
            .find_map(|c| match c {
                Claim::Value {
                    x: bx,
                    of: Subject::F(0),
                    lo,
                    hi,
                } if bx.0 == x.lo && bx.1 == x.hi => Some(Stated::Within(Enc { lo: *lo, hi: *hi })),
                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "a range end taken at {:e} with no value claim there",
                    x.mid()
                )
            });
    }
    if let Some(side) = src.get("Tail").and_then(Value::as_str) {
        let s = if side == "Right" {
            Side::Right
        } else {
            Side::Left
        };
        // f's bands far out, with the simplifier's exact limit if any:
        // never the fact alone, but for a rational f's exact one.
        let at = if s == Side::Right { "+∞" } else { "−∞" };
        let toward = if s == Side::Right {
            At::PosInf
        } else {
            At::NegInf
        };
        if let Some(v) = structural(toward, at) {
            return Ok(v);
        }
        if rational_at(at) {
            // N/D's limit there, the replay's own, exactly (its fact
            // checked so): not the fact's text read back as doubles.
            return super::algebra::Rat::of(&fx.f, &fx.lits, &fx.vars, fx.unit)
                .and_then(|r| r.limit(s == Side::Right).ok())
                .map(Stated::Exact)
                .ok_or_else(|| {
                    format!("a range end from the {side} tail, where N/D has no finite limit")
                });
        }
        let bands = tail_bands(&rc.claims, s);
        if bands.len() < 2 {
            return Err(format!(
                "a range end from the {side} tail with no bands of f's values there"
            ));
        }
        let shown = Enc {
            lo: bands.iter().map(|b| b.1).fold(f64::NEG_INFINITY, f64::max),
            hi: bands.iter().map(|b| b.2).fold(f64::INFINITY, f64::min),
        };
        return match fact_at(at).and_then(fact_stated) {
            Some(f) => f.within(&shown).ok_or_else(|| {
                format!("a range end from the {side} tail: the fact's limit is outside f's bands")
            }),
            None => Ok(Stated::Within(shown)),
        };
    }
    if let Some(x) = src.get("Hole") {
        let x = enc(x)?;
        return rc
            .claims
            .iter()
            .find_map(|c| match c {
                Claim::Removable { at, lo, hi, .. } if at.0 == x.lo && at.1 == x.hi => {
                    Some(Stated::Within(Enc { lo: *lo, hi: *hi }))
                }
                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "a range end from a hole at {:e} with no claim there",
                    x.mid()
                )
            });
    }
    if let Some(sd) = src.get("Side") {
        let at = sd.get("at").and_then(|v| r(v).ok()).ok_or("side at")?;
        let right = sd
            .get("right")
            .and_then(Value::as_bool)
            .ok_or("side right")?;
        let toward = if right { At::Right(at) } else { At::Left(at) };
        let name = format!("{at}{}", if right { "⁺" } else { "⁻" });
        if let Some(v) = structural(toward, &name) {
            return Ok(v);
        }
        let stated = fact_at(&name)
            .and_then(fact_stated)
            .ok_or_else(|| format!("a range end from a one-sided limit at {at:e} with no fact"))?;
        let fact = stated.enc();
        // Borne out by f's bands on boxes beside the point (p left out):
        // settling, and closing in on the limit.
        let bands = side_bands(&rc.claims, at, right);
        let scale = fact.lo.abs().max(fact.hi.abs()).max(1.0);
        let apart = |b: &(f64, f64, f64)| (fact.lo - b.2).max(b.1 - fact.hi).max(0.0);
        let d: Vec<f64> = bands.iter().map(apart).collect();
        let n = d.len();
        let tol = 1e-9 * scale;
        let closing = n >= 3
            && (d[n - 3..].windows(2).all(|p| p[1] <= p[0])
                || d[n - 3..].iter().all(|v| *v <= tol))
            && d[n - 1] <= tol;
        let w: Vec<f64> = bands.iter().map(|b| b.2 - b.1).collect();
        let settled = n >= 3
            && (w[n - 1] <= 1e-9 * scale
                || (w[n - 3..].windows(2).all(|p| p[1] <= p[0])
                    && w[n - 1] <= 1e-2 * w.iter().copied().fold(0.0, f64::max)));
        if !(closing && settled) {
            return Err(format!(
                "a one-sided limit at {at:e}: f's bands beside it don't close in on it"
            ));
        }
        return Ok(stated);
    }
    Err(format!("an unread range end source {src}"))
}

/// Oblique asymptotes: each listed line from the simplifier's facts
/// (weak), each tail without one ruled out by a fact, a horizontal
/// asymptote there, or the domain not reaching it.
fn oblique(fx: &Fx, rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    let facts: Vec<&String> = rc
        .claims
        .iter()
        .filter_map(|c| match c {
            Claim::Simplifier(f) => Some(f),
            _ => None,
        })
        .collect();
    let rational_line = facts
        .iter()
        .any(|f| f.starts_with("f = N/D exactly and N/D − ("));
    let rational_none = facts
        .iter()
        .any(|f| f.starts_with("f = N/D exactly with deg N ≠ deg D + 1"));
    let periodic = facts.iter().any(|f| f.starts_with("f is periodic"));
    // N/D − (m·x + b) → 0: the replay's own m and b, exactly.
    let exact_line = rational_line
        .then(|| super::algebra::Rat::of(&fx.f, &fx.lits, &fx.vars, fx.unit))
        .flatten()
        .and_then(|r| r.line());
    let holds = |y: &Enc, v: &rug::Rational| *v >= y.lo && *v <= y.hi;
    let mut sides = Vec::new();
    // A line's graph is the line itself: no asymptote. One listed needs f
    // shown to be no m·x + b.
    if !list(&rc.value)?.is_empty() {
        if line_fact(&rc.claims) {
            out.problems
                .push("an oblique asymptote listed for a line, its own graph".into());
        }
        if !not_affine(&rc.claims) {
            out.problems
                .push("an oblique asymptote listed, but nothing shows f is no line".into());
        }
    }
    for item in list(&rc.value)? {
        let side = item.get("side").and_then(Value::as_str).ok_or("side")?;
        let at = if side == "Right" { "+∞" } else { "−∞" };
        // (A line from the simplifier's limits alone isn't borne out.) Or
        // f's expansion on a tail there (its claims checked so), the
        // listed slope and intercept holding the proven ones.
        let s = if side == "Right" {
            Side::Right
        } else {
            Side::Left
        };
        let (m, b) = (
            enc(item.get("m").ok_or("m")?)?,
            enc(item.get("b").ok_or("b")?)?,
        );
        let lines: Vec<(B, B)> = rc
            .claims
            .iter()
            .filter_map(|c| match c {
                Claim::TailLine { side: t, m, b, .. } if *t == s => Some((*m, *b)),
                _ => None,
            })
            .collect();
        let expanded = !lines.is_empty()
            && lines
                .iter()
                .all(|(lm, lb)| m.lo <= lm.0 && lm.1 <= m.hi && b.lo <= lb.0 && lb.1 <= b.hi);
        if !lines.is_empty() && !expanded {
            out.problems.push(format!(
                "the oblique asymptote at {at} is listed outside its expansion's slope or intercept"
            ));
        }
        // A rational f's line (its fact checked so): the listed slope and
        // intercept hold N/D's own, the replay's, exactly. (The fact's
        // presence alone backed any listing beside it: the slope 1/3 of
        // (x² + x + 1)/(3x) at the double below it, or as 5. Review 18's
        // follow-up.)
        let on_line = exact_line
            .as_ref()
            .is_some_and(|(gm, gb)| holds(&m, gm) && holds(&b, gb));
        if rational_line && lines.is_empty() && !on_line {
            out.problems.push(format!(
                "the oblique asymptote at {at} is listed off N/D's line"
            ));
        }
        let backed = on_line || expanded;
        if !backed {
            out.problems
                .push(format!("an oblique asymptote at {at} with no fact for it"));
        }
        sides.push(side.to_string());
    }
    let horizontal: Vec<String> = all
        .iter()
        .find(|r| r.name == "horizontal" && r.status == "Certified")
        .and_then(|h| h.value.as_array())
        .map(|hs| {
            hs.iter()
                .filter_map(|h| h.get("side").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for (side, at) in [("Left", "−∞"), ("Right", "+∞")] {
        let n = sides.iter().filter(|s| *s == side).count();
        if n > 1 {
            out.problems
                .push(format!("{n} oblique asymptotes listed at {at}"));
        }
        if n > 0 {
            continue;
        }
        let s = if side == "Right" {
            Side::Right
        } else {
            Side::Left
        };
        // (With no domain row the tail may be reached, for all that is
        // known: read as "not reached", a row alone ruled every line out.)
        let reaches = reaches_tail(all, s);
        let fact = |head: &str| {
            facts
                .iter()
                .any(|f| f.starts_with(head) && f.contains(&format!("as x → {at}")))
        };
        // (A rational f with a line has it at both tails: its fact rules
        // none out. It did, and the line on one side could be left out.)
        let ruled_out = !reaches
            || rational_none
            || periodic
            || horizontal.iter().any(|h| h == side)
            || line_fact(&rc.claims)
            || periodic_rest(&rc.claims)
            || bends_away(&rc.claims, s)
            || matches!(
                limit_at(
                    &rc.claims,
                    if side == "Right" {
                        At::PosInf
                    } else {
                        At::NegInf
                    },
                    true
                ),
                Some(Toward::PosInf | Toward::NegInf)
            )
            || limit_at(
                &rc.claims,
                if side == "Right" {
                    At::PosInf
                } else {
                    At::NegInf
                },
                true,
            ) == Some(Toward::In(0.0, 0.0))
            || (fact("f/x → 0") && slope_to_zero(&rc.claims, s));
        if !ruled_out {
            out.problems.push(format!(
                "no oblique asymptote at {at}, but nothing rules one out"
            ));
        }
    }
    if !facts.is_empty() {
        out.notes.push("rests on the simplifier's limits".into());
    }
    Ok(())
}
