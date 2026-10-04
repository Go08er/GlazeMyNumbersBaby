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
use super::iv::{self, Iv};
use super::{B, Claim, ClaimResult, Class, Fx, Region, RowCert, RowResult, Side, Subject, r};
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

/// f⁽ᵏ⁾'s sign at a double, from the replay's own evaluation.
fn sign_at(fx: &Fx, k: usize, x: f64) -> Option<bool> {
    for tree in [Tree::Orig, Tree::Eval] {
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
                });
            }
            Claim::Undefined(x) | Claim::Gap(x) => {
                w.holes.push(*x);
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

/// Changes of sign: (where, from positive).
fn changes(w: &Walk) -> Vec<(Enc, bool)> {
    w.zeros
        .iter()
        .filter_map(|z| match (z.left, z.right) {
            (Some(l), Some(r)) if l != r => Some((z.at, l)),
            _ => None,
        })
        .collect()
}

/// The note on a row's gaps: f⁽ᵏ⁾ must have no zero in them, which the
/// certificate doesn't state (research/replay-issues.md, issue 3); the
/// replay shows it where it can.
pub const GAPS_NOT_IN_CERT: &str = "clearness is not in the certificate";

fn holes_note(fx: &Fx, rc: &RowCert, k: usize) -> Option<String> {
    let gaps: Vec<B> = rc
        .claims
        .iter()
        .filter_map(|c| match c {
            Claim::Gap(x) => Some(*x),
            _ => None,
        })
        .collect();
    if gaps.is_empty() {
        return None;
    }
    let (mut strong, mut weak, mut open) = (0, 0, Vec::new());
    for g in &gaps {
        match gap_clear(fx, k, *g) {
            Some(true) => strong += 1,
            Some(false) => weak += 1,
            None => open.push(*g),
        }
    }
    let mut note = format!(
        "{} gaps, f{} free of zeros in them ({GAPS_NOT_IN_CERT}): {strong} shown by the replay",
        gaps.len(),
        ["", "′", "″"][k]
    );
    if weak > 0 {
        note += &format!(", {weak} on the certifier's tree");
    }
    if !open.is_empty() {
        note += &format!(
            ", {} not shown (first [{:e}, {:e}])",
            open.len(),
            open[0].0,
            open[0].1
        );
    }
    Some(note)
}

/// Whether f⁽ᵏ⁾ has no zero in the gap box `g`, wherever it exists there:
/// `Some(true)` shown on the canonical tree, `Some(false)` only on the
/// certifier's derivative tree, `None` not shown.
fn gap_clear(fx: &Fx, k: usize, g: B) -> Option<bool> {
    use super::series as se;
    if claims::unmodelled(&fx.f) {
        return None;
    }
    // The reals strictly inside the gap: its ends are doubles other claims
    // decide (or the excluded point). Beside an excluded 0, x keeps its
    // strict sign.
    let x = |p: u32| {
        iv::set_prec(p);
        Iv::of2(g.0, g.1).strict(g.0 == 0.0, g.1 == 0.0)
    };
    let away = |e: &graphing::ast::Expr, k: usize| -> bool {
        [160, 640].into_iter().any(|p| {
            let xb = x(p);
            let s = se::where_defined(|| fx.series_iv(e, xb, k));
            let c = &s[k];
            let r = s[0].empty || c.empty || c.ne0();
            iv::set_prec(160);
            r
        })
    };
    if away(&fx.f, k) {
        return Some(true);
    }
    // Factor by factor (f's zeros are among its factors').
    let by_factors = |e: &graphing::ast::Expr| -> bool {
        let mut safe = Vec::new();
        claims::nonzero_where_defined(&fx.f, &mut safe);
        claims::nonzero_where_defined(e, &mut safe);
        claims::zero_factors(e)
            .iter()
            .all(|h| safe.contains(h) || away(h, 0))
    };
    if k == 0 && by_factors(&fx.f) {
        return Some(true);
    }
    if k > 0
        && let Some(d) = &fx.d[k - 1]
        && (away(d, 0) || by_factors(d))
    {
        return Some(false);
    }
    // The simplifier's form of f (equal to f where f is defined).
    if let Some(e) = &fx.f_eval
        && (away(e, k) || (k == 0 && by_factors(e)))
    {
        return Some(false);
    }
    None
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

/// Is `x` (enclosed) a repeat of one of `xs` by a whole number of periods?
fn repeat_of(xs: &[Enc], x: &Enc, period: &Enc) -> bool {
    xs.iter().any(|e| {
        let k = ((x.mid() - e.mid()) / period.mid()).round();
        let (a, b) = (e.lo + k * period.lo, e.hi + k * period.hi);
        let (a, b) = (a.min(b), a.max(b));
        let slack = 1e-9 * x.mid().abs().max(1.0);
        a - slack <= x.hi && x.lo <= b + slack
    })
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

fn zeros_row(fx: &Fx, rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    let w = walk(fx, &rc.claims, 0);
    out.problems.extend(w.problems.iter().cloned());
    coverage(rc, all, &w.boxes, out);
    if let Some(n) = holes_note(fx, rc, 0) {
        out.notes.push(n);
    }
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
    if let Some(n) = holes_note(fx, rc, k) {
        out.notes.push(n);
    }
    let mut ch = changes(&w);
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
/// (f′ must also have no zero in the gap beside the end, which the
/// certificate doesn't record.) Returns (where, is a minimum).
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
    if let Some(n) = holes_note(fx, rc, 1) {
        out.notes.push(n);
    }
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
        let k = ((-x0.mid() - x0.mid()) / per.mid()).round();
        let (a, b) = (x0.lo + k * per.lo, x0.hi + k * per.hi);
        let slack = 1e-9 * x0.mid().abs().max(1.0) + (k.abs() + 1.0) * (per.hi - per.lo);
        (a.min(b) - slack) <= -x0.lo && -x0.hi <= (a.max(b) + slack)
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
            super::eval::written_rational(k).is_some_and(|(p, _)| p > 0) && divisor(f, up)
        }
        _ => false,
    }
}

/// `t` with the node at `path` replaced by a stand-in variable.
fn replace(t: &graphing::ast::Expr, path: &[u8], name: &str) -> graphing::ast::Expr {
    use graphing::ast::Expr;
    let Some((&i, rest)) = path.split_first() else {
        return Expr::Var(name.into());
    };
    match t {
        Expr::Neg(a) => Expr::Neg(Box::new(replace(a, rest, name))),
        Expr::Degrees(a) => Expr::Degrees(Box::new(replace(a, rest, name))),
        Expr::Bin(op, a, b) => {
            if i == 0 {
                Expr::Bin(*op, Box::new(replace(a, rest, name)), b.clone())
            } else {
                Expr::Bin(*op, a.clone(), Box::new(replace(b, rest, name)))
            }
        }
        Expr::Call(f, args) => {
            let mut args = args.clone();
            if let Some(a) = args.get_mut(i as usize) {
                *a = replace(a, rest, name);
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
        let to_inf = rc.claims.iter().any(|c| {
            matches!(c, Claim::TailBeyond { of: Subject::F(1), c, .. } if *c != 0.0)
                || matches!(c, Claim::TailChain { of: Subject::F(1), .. })
                || matches!(c, Claim::Simplifier(f) if f.contains("→ +∞ as x → ") || f.contains("→ −∞ as x → "))
        });
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
        let limit = rc
            .claims
            .iter()
            .any(|c| matches!(c, Claim::TailValue { .. } | Claim::Simplifier(_)));
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

// ------------------------------------------------------------ asymptotes

fn vertical(rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    let listed: Vec<(Enc, Option<Enc>)> = list(&rc.value)?
        .iter()
        .map(spot)
        .collect::<Result<_, _>>()?;
    for (x, _) in &listed {
        let unbounded = rc.claims.iter().any(|c| {
            matches!(c, Claim::Unbounded { at, .. } if at.0 == x.lo && at.1 == x.hi)
                || matches!(c, Claim::Simplifier(f) if f.contains("∞ as x → ") && x.lo == x.hi && f.contains(&format!("{}", x.lo)))
        });
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

fn horizontal(rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
    let _ = all;
    for item in list(&rc.value)? {
        let side = match item.get("side").and_then(Value::as_str) {
            Some("Left") => Side::Left,
            Some("Right") => Side::Right,
            other => return Err(format!("bad side {other:?}")),
        };
        let y = enc(item.get("y").ok_or("y")?)?;
        // f strictly monotone on the tail and bounded there: a limit, in
        // every enclosure of f far out; or the simplifier's limit.
        let mono = rc.claims.iter().any(|c| {
            matches!(c, Claim::TailBeyond { side: s, of: Subject::F(1), c, .. } if *s == side && *c == 0.0)
        });
        let vals: Vec<(f64, f64)> = rc
            .claims
            .iter()
            .filter_map(|c| match c {
                Claim::TailValue {
                    side: s, lo, hi, ..
                } if *s == side => Some((*lo, *hi)),
                _ => None,
            })
            .collect();
        let at = if side == Side::Right {
            "+∞"
        } else {
            "−∞"
        };
        let fact = rc.claims.iter().any(|c| {
            matches!(c, Claim::Simplifier(f) if f.starts_with("f → ") && f.ends_with(&format!("as x → {at}")))
        });
        if !vals.is_empty() && mono {
            let lo = vals.iter().map(|v| v.0).fold(f64::NEG_INFINITY, f64::max);
            let hi = vals.iter().map(|v| v.1).fold(f64::INFINITY, f64::min);
            if !(y.lo <= hi && lo <= y.hi) {
                out.problems.push(format!(
                    "the limit at {at} is listed in [{:e}, {:e}], outside f's values far out",
                    y.lo, y.hi
                ));
            }
            if !(lo >= y.lo && hi <= y.hi) && !fact {
                out.problems.push(format!(
                    "the limit at {at}: [{:e}, {:e}] is narrower than the claims show",
                    y.lo, y.hi
                ));
            }
        } else if fact {
            out.notes
                .push(format!("the limit at {at} rests on the simplifier's fact"));
        } else {
            out.problems
                .push(format!("a limit at {at} with no claims for it"));
        }
    }
    Ok(())
}

// ------------------------------------------------------------ range

/// The range: every listed end is a value a claim proves (taken, a
/// limit, or ±∞ by a claim), and no value of f at sample points falls
/// outside the listed pieces.
fn range(fx: &Fx, rc: &RowCert, all: &[RowCert], out: &mut RowResult) -> Result<(), String> {
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
    let unbounded = rc.claims.iter().any(|c| {
        matches!(c, Claim::Unbounded { .. })
            || matches!(c, Claim::TailBeyond { of: Subject::F(1), c, .. } if *c != 0.0)
            || matches!(c, Claim::TailChain { of: Subject::F(1), .. })
            || matches!(c, Claim::Simplifier(f) if f.starts_with("f → +∞") || f.starts_with("f → −∞"))
    });
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
                if f.starts_with("f → ") && f.ends_with(&format!("as x → {at}")) =>
            {
                fact_limit(f)
            }
            _ => None,
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
        let tv = rc
            .claims
            .iter()
            .filter_map(|c| match c {
                Claim::TailValue {
                    side: t, lo, hi, ..
                } if *t == s => Some((*lo, *hi)),
                _ => None,
            })
            .reduce(|a, b| (a.0.max(b.0), a.1.min(b.1)))
            .map(|(lo, hi)| Enc { lo, hi });
        let fact = fact_at(if s == Side::Right { "+∞" } else { "−∞" });
        return match (tv, fact) {
            (Some(t), Some(f)) => Ok(Enc {
                lo: t.lo.max(f.lo),
                hi: t.hi.min(f.hi),
            }),
            (Some(t), None) => Ok(t),
            (None, Some(f)) => Ok(f),
            (None, None) => Err(format!(
                "a range end from the {side} tail with no claim there"
            )),
        };
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
        return fact_at(&format!("{at}{}", if right { "⁺" } else { "⁻" }))
            .ok_or_else(|| format!("a range end from a one-sided limit at {at:e} with no fact"));
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
    for item in list(&rc.value)? {
        let side = item.get("side").and_then(Value::as_str).ok_or("side")?;
        let at = if side == "Right" { "+∞" } else { "−∞" };
        let backed = rational_line
            || facts.iter().any(|f| {
                f.starts_with("f/x → ")
                    && f.contains(" and f − m·x → ")
                    && f.ends_with(&format!("as x → {at}"))
            });
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
        let ruled_out = !reaches
            || rational_line
            || rational_none
            || periodic
            || horizontal.iter().any(|h| h == side)
            || facts
                .iter()
                .any(|f| f.starts_with("f/x → ") && f.contains(&format!("as x → {at}")));
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
