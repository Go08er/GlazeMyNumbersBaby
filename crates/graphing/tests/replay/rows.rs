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
            Claim::Kink(x) if k > 0 => {
                w.zeros.push(Zero {
                    at: Enc { lo: x.0, hi: x.1 },
                    left: None,
                    right: None,
                    kink: true,
                });
                w.boxes.push(*x);
            }
            _ => {}
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
            _ => w.problems.push(format!(
                "a crossing in [{:e}, {:e}] without opposite signs at its ends",
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
    // The claims' own outcomes.
    let mut weak = 0;
    let mut open = 0;
    for c in &rc.claims {
        if let Some(res) = results.iter().find(|r| r.claim == *c) {
            match res.outcome.class {
                Class::Weak => weak += 1,
                Class::Unconfirmed | Class::Unsupported => open += 1,
                _ => {}
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
    let refuted = rc
        .claims
        .iter()
        .filter(|c| {
            results
                .iter()
                .any(|r| r.claim == **c && r.outcome.class == Class::Refuted)
        })
        .count();
    if refuted > 0 {
        out.problems
            .push(format!("it rests on {refuted} refuted claims"));
    }
    if let Err(e) = in_domain(rc, all, results, &mut out) {
        out.problems.push(format!("unreadable value: {e}"));
    }
    let r = match rc.name.as_str() {
        "domain" => domain(fx, rc, &mut out),
        "x_intercepts" => zeros_row(fx, rc, all, &mut out),
        "extrema" => turns(fx, rc, all, 1, &mut out),
        "inflections" => turns(fx, rc, all, 2, &mut out),
        "monotonicity" => monotonicity(fx, rc, all, &mut out),
        "y_intercept" => y_intercept(fx, rc, &mut out),
        "parity" => parity(rc, all, results, &mut out),
        "period" => period(fx, rc, all, &mut out),
        "vertical" => vertical(rc, all, &mut out),
        "horizontal" => horizontal(rc, all, &mut out),
        "oblique" => oblique(rc, all, &mut out),
        "range" => range(fx, rc, all, &mut out),
        _ => Ok(()),
    };
    if let Err(e) = r {
        out.problems.push(format!("unreadable value: {e}"));
    }
    out
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
        for (x, min) in end_extrema(rc, all, &w) {
            ch.push((x, !min));
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
/// minimum).
fn end_extrema(rc: &RowCert, all: &[RowCert], w: &Walk) -> Vec<(Enc, bool)> {
    let Some(pieces) = domain_pieces(all) else {
        return Vec::new();
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
    for p in &pieces {
        if p.lo == p.hi {
            continue;
        }
        for (b, low) in [(p.lo, true), (p.hi, false)] {
            let Bound::At { x, closed: true } = b else {
                continue;
            };
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
                continue;
            };
            // f continuous from the end into that stretch.
            let joined = conts.iter().any(|c| {
                if low {
                    c.0 <= x.lo && c.1 >= sa
                } else {
                    c.1 >= x.hi && c.0 <= sb
                }
            });
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
            }
        }
    }
    out
}

fn monotonicity(fx: &Fx, rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    let w = walk(fx, &rc.claims, 1);
    out.problems.extend(w.problems.iter().cloned());
    coverage(rc, all, &w.boxes, out);
    holes(rc, 1, complete(rc), out);
    for item in list(&rc.value)? {
        let p = piece(item.get("on").ok_or("on")?)?;
        let dir = item.get("dir").and_then(Value::as_str).unwrap_or("");
        let (a, b) = extent(&p);
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
            _ => None,
        };
        for (sa, sb, s) in &w.signs {
            let overlaps = *sa < inner_hi && *sb > inner_lo;
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
        for z in &w.zeros {
            let inside = z.at.lo > inner_lo && z.at.hi < inner_hi;
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
        if !v.empty && valid {
            out.problems
                .push(format!("no y-intercept listed, yet f(0) is defined: {v:?}"));
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
                None => out.notes.push("the domain's symmetry is not known".into()),
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
        out.notes
            .push("not checked: the tree uses a function the replay doesn't model".into());
        return Ok(());
    }
    // Each family: a Family claim with these boxes, about a node whose
    // condition is g ≠ 0 (a divisor, or the cosine or sine under tan, sec,
    // cot, csc). Its node stands in as "any value" (any nonzero value for
    // a divisor) when the rest of f is checked defined.
    let mut stand_ins: Vec<(Vec<u8>, bool)> = Vec::new();
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
        match via {
            super::Via::CosOfArg | super::Via::SinOfArg => stand_ins.push((path, false)),
            super::Via::Itself => {
                // g itself must be a divisor, or the base of a positive
                // power that is (g ≠ 0 ⟺ gⁿ ≠ 0): only those are read.
                let parent_div = divisor(&fx.f, &path);
                if parent_div {
                    stand_ins.push((path, true));
                } else {
                    out.notes.push(format!(
                        "the family at {:e}: its node's condition is not read (not a divisor)",
                        x0.mid()
                    ));
                    return Ok(());
                }
            }
        }
    }
    // The tree(s) to check definedness on.
    let mut trees: Vec<graphing::ast::Expr> = vec![fx.f.clone()];
    for (path, nonzero) in &stand_ins {
        let mut next = Vec::new();
        for t in &trees {
            if *nonzero {
                next.push(replace(t, path, "__pos"));
                next.push(replace(t, path, "__neg"));
            } else {
                next.push(replace(t, path, "__any"));
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
                claims::V::Unknown(w) => out.notes.push(format!("a piece not shown defined: {w}")),
            }
        } else {
            for t in &trees {
                match defined_on(t, (lo, hi)) {
                    claims::V::Yes => {}
                    claims::V::No(w) | claims::V::Unknown(w) => {
                        out.notes
                            .push(format!("off its families, a piece not shown defined: {w}"));
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
                None => out.notes.push(format!(
                    "the end in [{:e}, {:e}]: not shown {}",
                    x.lo,
                    x.hi,
                    shut(closed)
                )),
            }
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
        super::eval::written_rational(k, &fx.lits).is_some_and(|(p, q)| q == 1 && p % 2 == 0)
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
            Some((_, q)) if q % 2 == 0 => push(0, 0.0, true),
            Some(_) => {}
            // A constant exactly an integer: an integer power.
            None if !super::eval::contains_x(b)
                && super::exact::eval(b, None, &fx.lits, &fx.vars)
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
        path.push(i as u8);
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
        let edge = replace_with(&fx.f, &ed.path, Expr::Num(ed.c));
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

/// Whether the node at `path` is a divisor of `f`, or the base of a
/// positive power (a chain of them) that is one.
fn divisor(f: &graphing::ast::Expr, path: &[u8]) -> bool {
    use graphing::ast::{BinOp, Expr};
    let Some((last, up)) = path.split_last() else {
        return false;
    };
    match super::eval::at_path(f, up) {
        Some(Expr::Bin(BinOp::Div, ..)) => *last == 1,
        Some(Expr::Bin(BinOp::Pow, _, k)) if *last == 0 => {
            super::eval::written_rational(k, &super::eval::Lits::default())
                .is_some_and(|(p, _)| p > 0)
                && divisor(f, up)
        }
        _ => false,
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
        // f′ ≡ 0 on the line: f is constant, every shift a period, none
        // least (the corpus's convention: "none").
        let constant = rc.claims.iter().any(|c| {
            matches!(c, Claim::Value { x, of: Subject::F(1), lo, hi }
                if x.0 == f64::NEG_INFINITY && x.1 == f64::INFINITY && *lo == 0.0 && *hi == 0.0)
        });
        if constant {
            out.notes.push("f is constant: no least period".into());
            return Ok(());
        }
        // A tail: to ±∞ (f′ beyond a nonzero c), or a limit with two
        // values apart.
        let to_inf = shows_unbounded(&rc.claims);
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
        // A limit at a tail: f's bands there, or a rational f's exact one.
        let limit = [Side::Left, Side::Right]
            .iter()
            .any(|&s| !tail_bands(&rc.claims, s).is_empty())
            || rc.claims.iter().any(|c| {
                matches!(c, Claim::Simplifier(f) if f.starts_with(claims::RATIONAL) && fact_limit(f).is_some())
            })
            || [At::PosInf, At::NegInf]
                .iter()
                .any(|&at| matches!(limit_at(&rc.claims, at, false), Some(Toward::In(..))));
        if !(to_inf || (limit && apart)) {
            out.problems
                .push("not periodic, but the claims don't show it".into());
        }
        return Ok(());
    }
    let p = enc(&rc.value)?;
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
            "the period is listed in [{:e}, {:e}], the fact says {v:?}",
            p.lo, p.hi
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
        if fams.len() != 1 {
            out.problems
                .push("no bound on a smaller period from the claims".into());
            return Ok(());
        }
        (p.hi / fams[0].lo * (1.0 + 1e-12)).floor()
    };
    // Each prime q ≤ n: two values, at x and x + P/q, apart.
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

/// Bounds that grow without settling: three or more records (each beyond
/// every bound before it, in the direction `up`), gaining at least 1.
fn growing(mut bs: Vec<(f64, f64)>, up: bool) -> bool {
    bs.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Several bounds from one place (other rows' claims): the strongest.
    let mut one: Vec<(f64, f64)> = Vec::new();
    for b in bs {
        match one.last_mut() {
            Some(l) if l.0 == b.0 => {
                l.1 = if up { l.1.max(b.1) } else { l.1.min(b.1) };
            }
            _ => one.push(b),
        }
    }
    // The records: each beyond every bound before it (a weaker bound
    // further out, another row's, adds nothing).
    let mut bs: Vec<(f64, f64)> = Vec::new();
    for b in one {
        let beyond = bs
            .last()
            .is_none_or(|l: &(f64, f64)| if up { b.1 > l.1 } else { b.1 < l.1 });
        if beyond {
            bs.push(b);
        }
    }
    if bs.len() < 3 {
        return false;
    }
    let ok = bs
        .windows(2)
        .all(|w| if up { w[1].1 > w[0].1 } else { w[1].1 < w[0].1 });
    let (first, last) = (bs[0].1, bs[bs.len() - 1].1);
    ok && (last - first).abs() >= 1.0
}

/// Whether the claims show f → +∞ (`up`) or −∞ on the tail: f′ beyond a
/// nonzero bound of the matching sign, or f′'s sign chain (f′ moving away
/// from 0); or f beyond bounds on [M, ∞) that grow out along the tail.
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
        Claim::TailChain {
            side: s,
            of: Subject::F(1),
            above,
            ..
        } if *s == side => (*above == right) == up,
        _ => false,
    });
    let bounds: Vec<(f64, f64)> = claims
        .iter()
        .filter_map(|c| match c {
            Claim::TailBeyond {
                side: s,
                from,
                of: Subject::F(0),
                c,
                above,
            } if *s == side && *above == up => Some((from.abs(), *c)),
            _ => None,
        })
        .collect();
    // f monotone on the tail, moving that way, past values that grow.
    let points = || -> Vec<(f64, f64)> {
        let Some((rising, start)) = tail_mono(claims, side) else {
            return Vec::new();
        };
        if (rising == right) != up {
            return Vec::new();
        }
        claims
            .iter()
            .filter_map(|c| match c {
                Claim::Value {
                    x,
                    of: Subject::F(0),
                    lo,
                    hi,
                } if x.0 == x.1 && (x.0 > 0.0) == right && on_tail(x.0, start, right) => {
                    Some((x.0.abs(), if up { *lo } else { *hi }))
                }
                _ => None,
            })
            .collect()
    };
    let at = if right { At::PosInf } else { At::NegInf };
    let structure =
        limit_at(claims, at, false) == Some(if up { Toward::PosInf } else { Toward::NegInf });
    structure || proof || growing(bounds, up) || growing(points(), up)
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

/// Whether the claims show f → ±∞ beside the point p (from the right, or
/// the left): unbounded there by structure, or beyond bounds on boxes
/// beside p that grow as the boxes shrink.
fn side_infinite(claims: &[Claim], p: f64, right: bool) -> bool {
    let at = if right { At::Right(p) } else { At::Left(p) };
    let structural = claims
        .iter()
        .any(|c| matches!(c, Claim::Unbounded { at, .. } if at.0 <= p && p <= at.1))
        || matches!(
            limit_at(claims, at, false),
            Some(Toward::PosInf | Toward::NegInf)
        );
    let near = |x: &B| {
        if right {
            x.0 == p.next_up()
        } else {
            x.1 == p.next_down()
        }
    };
    let bounds = |up: bool| -> Vec<(f64, f64)> {
        claims
            .iter()
            .filter_map(|c| match c {
                Claim::Beyond {
                    x,
                    of: Subject::F(0),
                    c,
                    above,
                } if near(x) && *above == up => Some((-(x.1 - x.0), *c)),
                _ => None,
            })
            .collect()
    };
    structural || growing(bounds(true), true) || growing(bounds(false), false)
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

/// f′ on the tail shown beyond bounds growing in size (`to_zero` false:
/// f′ → ±∞, so f/x → ±∞) or within bands about 0 narrowing to 10⁻¹²
/// (f′ → 0, so f/x → 0): from bounds on f′ over tails, or, f′ monotone
/// there (f″ of one sign), from its values at points of the tail, beyond
/// which it moves.
fn slope_shown(claims: &[Claim], side: Side, to_zero: bool) -> bool {
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
    if to_zero {
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
    } else {
        let up: Vec<(f64, f64)> = bs.iter().filter(|b| b.2).map(|b| (b.0, b.1)).collect();
        let down: Vec<(f64, f64)> = bs.iter().filter(|b| !b.2).map(|b| (b.0, b.1)).collect();
        // f′ monotone, moving away from values that grow past 0.
        let moving: Vec<(f64, f64)> = points
            .iter()
            .map(|p| (p.0, if away_up { p.1 } else { p.2 }))
            .collect();
        growing(up, true) || growing(down, false) || growing(moving, away_up)
    }
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

fn vertical(rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    let listed: Vec<(Enc, Option<Enc>)> = list(&rc.value)?
        .iter()
        .map(spot)
        .collect::<Result<_, _>>()?;
    for (x, _) in &listed {
        let unbounded = rc
            .claims
            .iter()
            .any(|c| matches!(c, Claim::Unbounded { at, .. } if at.0 == x.lo && at.1 == x.hi))
            || (x.lo == x.hi
                && (side_infinite(&rc.claims, x.lo, true)
                    || side_infinite(&rc.claims, x.lo, false)));
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
    for e in &excluded {
        let is_listed = listed.iter().any(|(x, _)| x.contains(e));
        let unb = rc
            .claims
            .iter()
            .any(|c| matches!(c, Claim::Unbounded { at, .. } if at.0 == e.lo && at.1 == e.hi));
        if unb && !is_listed && !matches!(rc.region, Some(Region::Period(..))) {
            out.problems.push(format!(
                "f is unbounded at {:e}, but no asymptote is listed there",
                e.mid()
            ));
        }
    }
    let _ = all;
    Ok(())
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

fn horizontal(rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    let _ = all;
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
        let fact = rc.claims.iter().any(|c| {
            matches!(c, Claim::Simplifier(f) if f.starts_with("f → ") && f.ends_with(&format!("as x → {at}")))
        });
        let rational = rc.claims.iter().any(|c| {
            matches!(c, Claim::Simplifier(f) if f.starts_with(claims::RATIONAL) && f.ends_with(&format!("as x → {at}")))
        });
        if rational {
            // N/D's limit, exactly (its fact checked so): the listed one.
            let fact = rc.claims.iter().find_map(|c| match c {
                Claim::Simplifier(f)
                    if f.starts_with(claims::RATIONAL) && f.ends_with(&format!("as x → {at}")) =>
                {
                    fact_limit(f)
                }
                _ => None,
            });
            match fact {
                Some(v) if y.lo <= v.hi && v.lo <= y.hi => {}
                _ => out.problems.push(format!(
                    "the limit at {at} is listed in [{:e}, {:e}], not N/D's",
                    y.lo, y.hi
                )),
            }
            continue;
        }
        // By the tree's structure (its claim checked so): the limit lies
        // in the claim's enclosure; a narrower listing only with the
        // simplifier's exact value.
        let toward = if side == Side::Right {
            At::PosInf
        } else {
            At::NegInf
        };
        match limit_at(&rc.claims, toward, false) {
            Some(Toward::In(lo, hi)) => {
                if !(y.lo <= hi && lo <= y.hi) {
                    out.problems.push(format!(
                        "the limit at {at} is listed in [{:e}, {:e}], outside [{lo:e}, {hi:e}]",
                        y.lo, y.hi
                    ));
                } else if !(y.lo <= lo && hi <= y.hi) && !fact {
                    out.problems.push(format!(
                        "the limit at {at}: [{:e}, {:e}] is narrower than the claims show",
                        y.lo, y.hi
                    ));
                }
                // An exact form shown for it lies in the enclosure.
                if let Some(text) = item.get("exact").and_then(Value::as_str) {
                    iv::set_prec(160);
                    match claims::pi_q(text) {
                        Some(v) if v.hi >= lo && v.lo <= hi => {}
                        _ => out.problems.push(format!(
                            "the limit at {at} is written {text}, outside [{lo:e}, {hi:e}]"
                        )),
                    }
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
                if !(b.lo >= y.lo && b.hi <= y.hi) && !fact {
                    out.problems.push(format!(
                        "the limit at {at}: [{:e}, {:e}] is narrower than the claims show",
                        y.lo, y.hi
                    ));
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
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------ range

/// The range: every listed end is a value a claim proves (taken, a
/// limit, or ±∞ by a claim), and no value of f at sample points falls
/// outside the listed pieces.
fn range(fx: &Fx, rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    // The monotone pieces' images: f′'s sign decided in the gaps too.
    holes(rc, 1, complete(rc), out);
    let pieces: Vec<Piece> = list(&rc.value)?
        .iter()
        .map(piece)
        .collect::<Result<_, _>>()?;
    // Each finite end: the value its source's claims give (one source, or
    // several whose hull the end is) lies in the end's enclosure; an
    // attained (closed) end is a value taken. An infinite end: f
    // unbounded somewhere, by a claim.
    let ends = rc.extra.as_array();
    if let Some(e) = ends
        && e.len() != pieces.len()
    {
        out.problems.push(format!(
            "{} range pieces but {} pairs of end sources",
            pieces.len(),
            e.len()
        ));
    }
    let unbounded = shows_unbounded(&rc.claims);
    for (i, p) in pieces.iter().enumerate() {
        for (j, b) in [p.lo, p.hi].into_iter().enumerate() {
            let x = match b {
                Bound::NegInf | Bound::PosInf => {
                    if !unbounded {
                        out.problems
                            .push("an infinite range end with no claim f is unbounded".into());
                    }
                    continue;
                }
                Bound::At { x, closed } => {
                    let srcs: Vec<&Value> = ends
                        .and_then(|e| e.get(i))
                        .and_then(|pair| pair.get(j))
                        .and_then(Value::as_array)
                        .map(|s| s.iter().collect())
                        .unwrap_or_default();
                    if srcs.iter().any(|s| s.get("At").is_some()) != closed
                        && !srcs.is_empty()
                        && srcs.iter().all(|s| s.as_str() != Some("Other"))
                    {
                        out.problems.push(format!(
                            "a range end at {:e} {} but its sources say otherwise",
                            x.mid(),
                            if closed { "closed" } else { "open" }
                        ));
                    }
                    if srcs.is_empty() || srcs.iter().any(|s| s.as_str() == Some("Other")) {
                        // No single source named: some claim must give it.
                        if !end_given(rc, &x) {
                            out.problems.push(format!(
                                "a range end [{:e}, {:e}] no claim gives",
                                x.lo, x.hi
                            ));
                        }
                        continue;
                    }
                    x
                }
            };
            let srcs = ends
                .and_then(|e| e.get(i))
                .and_then(|pair| pair.get(j))
                .and_then(Value::as_array);
            for src in srcs.into_iter().flatten() {
                match source_value(rc, src) {
                    Err(why) => out.problems.push(why),
                    Ok(v) => {
                        if !(x.lo <= v.hi && v.lo <= x.hi)
                            || (srcs.is_some_and(|s| s.len() == 1)
                                && !x.contains(&v)
                                && !v.contains(&x))
                        {
                            out.problems.push(format!(
                                "a range end [{:e}, {:e}], but its source gives [{:e}, {:e}]",
                                x.lo, x.hi, v.lo, v.hi
                            ));
                        }
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
                .push(format!("f({x:e}) ∈ {v:?} is outside the listed range"));
            break;
        }
    }
    let _ = (all, split);
    Ok(())
}

/// Whether some claim shows f unbounded: structurally at a point, f′
/// beyond a nonzero bound on a tail, f′'s sign chain, or bounds that grow
/// out along a tail or beside a point. (A simplifier's "→ ±∞" alone never
/// does.)
fn shows_unbounded(claims: &[Claim]) -> bool {
    claims.iter().any(|c| matches!(c, Claim::Unbounded { .. }))
        || claims.iter().any(|c| {
            matches!(c, Claim::Limit { over_x: false, to: Toward::PosInf | Toward::NegInf, .. })
        })
        || claims.iter().any(|c| {
            matches!(c, Claim::Simplifier(f)
                if f.strip_prefix(claims::RATIONAL).is_some_and(|r| r.starts_with("f → +∞") || r.starts_with("f → −∞")))
        })
        || [Side::Left, Side::Right]
            .iter()
            .any(|&s| tail_infinite(claims, s, true) || tail_infinite(claims, s, false))
        || claims.iter().any(|c| match c {
            Claim::Beyond {
                x,
                of: Subject::F(0),
                ..
            } => {
                side_infinite(claims, x.0.next_down(), true)
                    || side_infinite(claims, x.1.next_up(), false)
            }
            _ => false,
        })
}

/// Some claim's value (or a fact's limit) lies in `e`.
fn end_given(rc: &RowCert, e: &Enc) -> bool {
    rc.claims.iter().any(|c| match c {
        Claim::Value {
            lo,
            hi,
            of: Subject::F(0),
            ..
        }
        | Claim::Removable { lo, hi, .. }
        | Claim::TailValue { lo, hi, .. } => e.lo <= *lo && *hi <= e.hi,
        Claim::Simplifier(f) => fact_limit(f).is_some_and(|v| e.contains(&v) || v.contains(e)),
        _ => false,
    })
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

/// The value a range end's source gives, from the row's claims.
fn source_value(rc: &RowCert, src: &Value) -> Result<Enc, String> {
    let fact_at = |at: &str| -> Option<Enc> {
        rc.claims.iter().find_map(|c| match c {
            Claim::Simplifier(f)
                if f.strip_prefix(claims::RATIONAL)
                    .unwrap_or(f)
                    .starts_with("f → ")
                    && f.ends_with(&format!("as x → {at}")) =>
            {
                fact_limit(f)
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
    let structural = |toward: At, at: &str| -> Option<Enc> {
        let Some(Toward::In(lo, hi)) = limit_at(&rc.claims, toward, false) else {
            return None;
        };
        Some(match fact_at(at) {
            Some(f) if f.lo <= hi && lo <= f.hi => Enc {
                lo: lo.max(f.lo),
                hi: hi.min(f.hi),
            },
            _ => Enc { lo, hi },
        })
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
                } if bx.0 == x.lo && bx.1 == x.hi => Some(Enc { lo: *lo, hi: *hi }),
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
        let bands = tail_bands(&rc.claims, s);
        let fact = fact_at(at);
        if rational_at(at)
            && let Some(f) = fact
        {
            return Ok(f);
        }
        if bands.len() < 2 {
            return Err(format!(
                "a range end from the {side} tail with no bands of f's values there"
            ));
        }
        let lo = bands.iter().map(|b| b.1).fold(f64::NEG_INFINITY, f64::max);
        let hi = bands.iter().map(|b| b.2).fold(f64::INFINITY, f64::min);
        return Ok(match fact {
            Some(f) => Enc {
                lo: lo.max(f.lo),
                hi: hi.min(f.hi),
            },
            None => Enc { lo, hi },
        });
    }
    if let Some(x) = src.get("Hole") {
        let x = enc(x)?;
        return rc
            .claims
            .iter()
            .find_map(|c| match c {
                Claim::Removable { at, lo, hi, .. } if at.0 == x.lo && at.1 == x.hi => {
                    Some(Enc { lo: *lo, hi: *hi })
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
        if let Some(v) = structural(toward, &format!("{at}{}", if right { "⁺" } else { "⁻" })) {
            return Ok(v);
        }
        let fact = fact_at(&format!("{at}{}", if right { "⁺" } else { "⁻" }))
            .ok_or_else(|| format!("a range end from a one-sided limit at {at:e} with no fact"))?;
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
        return Ok(fact);
    }
    Err(format!("an unread range end source {src}"))
}

/// Oblique asymptotes: each listed line from the simplifier's facts
/// (weak), each tail without one ruled out by a fact, a horizontal
/// asymptote there, or the domain not reaching it.
fn oblique(rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
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
        let backed = rational_line || expanded;
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
    let pieces = domain_pieces(all).unwrap_or_default();
    for (side, at) in [("Left", "−∞"), ("Right", "+∞")] {
        if sides.iter().any(|s| s == side) {
            continue;
        }
        let reaches = pieces.iter().any(|p| {
            if side == "Right" {
                p.hi == Bound::PosInf
            } else {
                p.lo == Bound::NegInf
            }
        });
        let s = if side == "Right" {
            Side::Right
        } else {
            Side::Left
        };
        let fact = |head: &str| {
            facts
                .iter()
                .any(|f| f.starts_with(head) && f.contains(&format!("as x → {at}")))
        };
        let ruled_out = !reaches
            || rational_line
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
            || (fact("f/x → ±∞") && slope_shown(&rc.claims, s, false))
            || (fact("f/x → 0") && slope_shown(&rc.claims, s, true));
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
