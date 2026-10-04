//! Certified analysis: function-analysis rows that are *proven*, each with
//! a machine-readable certificate a separate checker can replay.
//!
//! Every row is either certified (complete and correct), partial (every
//! item proven, but there may be more beyond the region the proof covers)
//! or unknown. Proofs come from the interval core (`crate::interval`): an
//! enclosure that excludes 0 proves a sign, a strictly monotone box with
//! opposite signs at its ends proves exactly one crossing, and the side
//! conditions of the original tree (divisors, logarithm arguments, roots,
//! the trigonometric poles) prove where f is defined.
//!
//! The panel shows these rows: `crate::analysis::analyze` certifies and
//! `analysis::certified` writes the rows from the proofs.

pub mod cert;
pub mod cover;
pub mod expand;
pub mod fun;
pub mod growth;
pub mod pole;
pub mod rows;
pub mod side;

pub use cert::*;
pub use fun::{Fun, Stop, canonical};

use std::sync::atomic::AtomicBool;

use crate::Equation;
use crate::compile::CompileOptions;
use crate::equation::Axis;
use crate::interval::Literals;
use crate::lexer::ParseOptions;
use crate::simplify::ExactLiterals;
use cover::{Cover, Target};
use serde::{Deserialize, Serialize};

/// Every row of a certified analysis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    /// The text analysed.
    pub source: String,
    /// The canonical tree every claim refers to (prefix notation).
    pub formula: String,
    /// The tree values were enclosed with, when not `formula`: the
    /// simplifier's form of f, equal to f wherever f is defined (claims
    /// about f are checked on it, where f is defined).
    pub evaluated: Option<String>,
    /// "radians", "degrees" or "grads".
    pub unit: String,
    pub domain: Row<DomainValue>,
    pub x_intercepts: Row<Vec<Spot>>,
    pub y_intercept: Row<Option<Enc>>,
    pub parity: Row<Parity>,
    /// The least period; `None`: not periodic.
    pub period: Row<Option<Enc>>,
    pub extrema: Row<Vec<Extremum>>,
    pub inflections: Row<Vec<Inflection>>,
    pub monotonicity: Row<Vec<Monotone>>,
    pub range: Row<Vec<Piece>>,
    /// Where each range piece's ends come from (for writing them exactly):
    /// one source, or several whose hull the end is.
    #[serde(default)]
    pub range_ends: Vec<[Vec<rows::EndSrc>; 2]>,
    pub vertical: Row<Vec<Spot>>,
    pub horizontal: Row<Vec<Horizontal>>,
    pub oblique: Row<Vec<Oblique>>,
    /// What the certificate was made under (certifier, rules, power
    /// convention, sliders); absent from older certificates.
    #[serde(default)]
    pub binding: Option<Binding>,
    /// Interval evaluations spent, in budget units (an evaluation of a
    /// big tree costs several: [`fun::UNIT_WORK`]).
    pub evals: u64,
    /// Set if the budget ran out or the caller cancelled.
    pub stopped: Option<String>,
}

/// The evaluation budget, in units of [`fun::UNIT_WORK`] (an evaluation
/// of a tree up to that size, more for a bigger one), shared out over the
/// phases (domain, f, f′, f″, the rows) unless a budget is given.
pub const DEFAULT_BUDGET: u64 = 200_000;

/// The half-width of the window the covers start from: four times the
/// largest constant in the expression (where its features sit), at least
/// 16, and at least four periods of any excluded family.
fn window(f: &Fun<'_>, d: &side::Domain) -> f64 {
    let c = fun::centres(f)
        .into_iter()
        .map(f64::abs)
        .filter(|v| v.is_finite() && *v <= fun::REACH)
        .fold(0.0, f64::max);
    let p = d
        .families
        .iter()
        .map(|fam| fam.period.hi())
        .fold(0.0, f64::max);
    (4.0 * c).max(16.0).max(4.0 * p)
}

/// The most family members the boxes over a window cut out (each a box, a
/// gap and their claims, through every cover and row).
const CUT_MEMBERS: f64 = 1000.0;

/// The half-width of the window for the boxes when excluded families cut
/// them: `w`, narrowed until the families have at most [`CUT_MEMBERS`]
/// members in it (x + tan(190x) over [−760, 760] would cut 92 000).
fn cut_window(dom: &side::Domain, w: f64) -> f64 {
    let per_unit: f64 = dom
        .families
        .iter()
        .map(|fam| 1.0 / fam.period.lo())
        .filter(|d| d.is_finite())
        .sum();
    if per_unit > 0.0 {
        w.min(CUT_MEMBERS / (2.0 * per_unit))
    } else {
        w
    }
}

/// One period `[s, s + P]` of f to analyse instead of the line, when the
/// simplifier proves f periodic with period P and f's domain is the line
/// (less excluded families): its start s is no family member, and f, f′
/// and f″ are strictly of one sign there (no feature sits on its edge, to
/// be missed at both ends).
fn periodic_window(f: &Fun<'_>, dom: &side::Domain) -> Option<(f64, Enc)> {
    if !dom.row.is_certified() || dom.pieces != [side::line()] {
        return None;
    }
    let s = f.settings()?;
    let per = crate::simplify::prove_period(&f.expr, &s)?;
    let pv = rows::piq_interval(per.value)?;
    if !(pv.lo() > 0.0 && pv.hi() <= 1e6) {
        return None;
    }
    let p = pv.mid();
    for c in [-0.5, -0.37, -0.29, -0.41, -0.23, -0.47, -0.31] {
        let start = c * p;
        let margin = 1e-9 * p;
        if dom.families.iter().any(|fam| {
            fam.members(start - margin, start + margin)
                .is_none_or(|m| !m.is_empty())
        }) {
            continue;
        }
        let d = 1e-6 * p;
        let Ok(sr) = f.ser(crate::interval::Interval::new(start - d, start + d), 2) else {
            return None;
        };
        if fun::usable(&sr, 2) && sr.iter().take(3).all(|v| !v.is_empty() && v.ne0()) {
            return Some((start, Enc::new(pv.lo(), pv.hi())));
        }
    }
    None
}

fn unit_name(u: crate::functions::TrigUnit) -> &'static str {
    match u {
        crate::functions::TrigUnit::Radians => "radians",
        crate::functions::TrigUnit::Degrees => "degrees",
        crate::functions::TrigUnit::Grads => "grads",
    }
}

/// Certifies the analysis of `y = f(x)` given as text (`y=` may be left
/// out).
pub fn certify_text(
    text: &str,
    opts: CompileOptions<'_>,
    budget: u64,
    cancel: Option<&AtomicBool>,
) -> Result<Analysis, String> {
    let text = if text.contains('=') {
        text.to_string()
    } else {
        format!("y={text}")
    };
    let eq = Equation::parse(&text).map_err(|e| format!("{e:?}"))?;
    certify_equation(&eq, opts, budget, cancel)
}

/// Certifies the analysis of a parsed explicit function `y = f(x)`.
pub fn certify_equation(
    eq: &Equation,
    opts: CompileOptions<'_>,
    budget: u64,
    cancel: Option<&AtomicBool>,
) -> Result<Analysis, String> {
    let text = eq.text();
    let Some((Axis::X, expr)) = eq.explicit() else {
        return Err("not a function of x".into());
    };
    let binding = Binding::of(eq, &opts, budget);
    let po: ParseOptions = eq.parse_options();
    let lits = Literals::of(text, po).map_err(|e| format!("{e:?}"))?;
    let exact = ExactLiterals::of(text, po).map_err(|e| format!("{e:?}"))?;
    let mut f = Fun::new(expr, &lits, opts, budget, cancel);
    f.exact = Some(&exact);
    if let Some(g) = rewrite(&f) {
        f.eval = canonical(&g);
    }
    f.eval = fun::recentre(&f.eval, &exact);
    // A symbolic derivative only for f differentiable wherever defined: no
    // floor, round, sign, mod (their derivative 0 hides the jumps), and no
    // abs, min or max of x (their derivative's tree, sign(u)·u′, is defined
    // at the kink, where f′ is not).
    let steps = !crate::simplify::side::jumps(&f.expr).is_empty();
    f.steps = steps;
    f.kinked = kinked(&f.expr) || kinked(&f.eval);
    f.smooth_tree = !steps && !f.kinked;
    if let Ok(conds) = side::sides(&f) {
        f.nonzero = conds
            .iter()
            .filter(|c| c.allowed_nonzero())
            .map(|c| canonical(&c.g))
            .collect();
    }
    f.numerators = fun::rational_numerators(&f.expr, &exact);
    let mut a = certify(&f, text);
    a.binding = Some(binding);
    Ok(a)
}

/// The kinks of f inside `spans`: where the argument of an abs, or the
/// difference of a min's or max's arguments, depending on x, is 0, each
/// enclosed by a box a few doubles wide (one double either side of an
/// exact zero), merged where they meet. `None` when a kink can't be placed
/// (those zeros aren't all decided, or the argument is 0 on a stretch).
fn kinks(f: &Fun<'_>, spans: &[(f64, f64)], args: &[crate::ast::Expr]) -> Option<Vec<(f64, f64)>> {
    let mut boxes: Vec<(f64, f64)> = Vec::new();
    for u in args {
        let c = Cover::run(
            f,
            &Target {
                expr: u,
                k: 0,
                in_domain: true,
            },
            &[0.0],
            spans,
        );
        if !c.complete() {
            return None;
        }
        for l in &c.leaves {
            match *l {
                cover::Leaf::Band { .. } | cover::Leaf::Undefined { .. } => {}
                cover::Leaf::Cross { exact: Some(p), .. }
                | cover::Leaf::At { p, .. }
                | cover::Leaf::Touch { p, .. } => boxes.push((p.next_down(), p.next_up())),
                cover::Leaf::Cross { l, r, .. } => boxes.push((l, r)),
                _ => return None,
            }
        }
    }
    Some(merged(boxes))
}

/// Boxes sorted and merged where they meet.
fn merged(mut boxes: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for b in boxes {
        match out.last_mut() {
            Some(last) if b.0 <= last.1 => last.1 = last.1.max(b.1),
            _ => out.push(b),
        }
    }
    out
}

/// The points where f′ (`k` = 1) or f″ (`k` = 2) is singular though f is
/// continuous (`Fun::singular_args`), each in a box strictly inside a span
/// of the domain (a zero of a base at a domain end is that end's), f
/// continuous across it. Those not placed are left out (f⁽ᵏ⁾ is then
/// undecided there, as before).
fn singular_boxes(f: &Fun<'_>, spans: &[(f64, f64)], k: usize) -> Vec<(f64, f64)> {
    let args: Vec<crate::ast::Expr> = f
        .singular_args()
        .into_iter()
        .filter(|(_, r)| *r < k as f64)
        .map(|(u, _)| u)
        .collect();
    if args.is_empty() {
        return Vec::new();
    }
    let Some(boxes) = kinks(f, spans, &args) else {
        return Vec::new();
    };
    boxes
        .into_iter()
        .filter(|&(l, r)| {
            spans.iter().any(|&(a, b)| a < l && r < b)
                && f.val(crate::interval::Interval::new(l, r))
                    .is_ok_and(|v| !v.is_empty() && v.dec >= crate::interval::Dec::Dac)
        })
        .collect()
}

/// `spans` with the kink boxes cut out: a span through one ends at its
/// low end and resumes at its high end; a point span inside one is
/// dropped.
fn split_at_kinks(spans: &[(f64, f64)], kinks: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &s in spans {
        let mut pieces = vec![s];
        for &(l, r) in kinks {
            let mut next = Vec::new();
            for (a, b) in pieces {
                if a == b {
                    if !(l <= a && a <= r) {
                        next.push((a, b));
                    }
                } else if r <= a || b <= l {
                    next.push((a, b));
                } else {
                    if a < l {
                        next.push((a, l));
                    }
                    if r < b {
                        next.push((r, b));
                    }
                }
            }
            pieces = next;
        }
        out.extend(pieces);
    }
    out
}

/// Whether f has a kink: abs, min or max of something depending on x.
fn kinked(e: &crate::ast::Expr) -> bool {
    use crate::ast::{Expr, Func};
    e.any(&|n| {
        matches!(n, Expr::Call(Func::Abs | Func::Min | Func::Max, args)
            if args.iter().any(|a| a.contains_x()))
    })
}

/// The simplifier's form of f to enclose values with, if it changed f and
/// every constant in it is enclosed soundly by [`Literals`]: an integer the
/// simplifier computed from inexact numbers would be read back as exact.
fn rewrite(f: &Fun<'_>) -> Option<crate::ast::Expr> {
    use crate::simplify::lang::Math;
    let settings = f.settings()?;
    let s = crate::simplify::simplify(&f.expr, &settings).ok()?;
    if !s.changed {
        return None;
    }
    let mut typed = Vec::new();
    f.expr.visit(&mut |n| {
        if let crate::ast::Expr::Num(v) = n {
            typed.push(v.abs());
        }
    });
    let unsound = s.term.as_ref().iter().any(|n| match n {
        Math::Real(r) => {
            let v = r.value().abs();
            v == v.trunc() && v <= 9007199254740992.0 && !typed.contains(&v)
        }
        // An exact rational whose numerator or denominator no double
        // holds: written back as rounded numbers (1 + 10⁻³⁰ would read 1).
        Math::Num(q) => q.numer().unsigned_abs() > 1 << 53 || q.denom() > 1 << 53,
        _ => false,
    });
    (!unsound).then_some(s.expr)
}

fn stop(s: Stop) -> String {
    match s {
        Stop::Budget => "out of budget".into(),
        Stop::Cancelled => "cancelled".into(),
    }
}

fn or_unknown<T>(r: Result<Row<T>, Stop>) -> Row<T> {
    r.unwrap_or_else(|s| Row::unknown(stop(s)))
}

/// The trees values were enclosed with, when not the formula's: the
/// simplified form, and f′, f″ of a rational function.
fn evaluated(f: &Fun<'_>) -> Option<String> {
    let mut parts = Vec::new();
    if f.eval != f.expr {
        parts.push(format!("f = {}", f.eval.formula()));
    }
    if let Some([d1, d2]) = f.derivs_built() {
        parts.push(format!("f′ = {}", d1.formula()));
        parts.push(format!("f″ = {}", d2.formula()));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// The row with `extra` claims added to its certificate.
fn with<T>(mut r: Row<T>, extra: &[Claim]) -> Row<T> {
    if let Row::Certified { cert, .. } | Row::Partial { cert, .. } = &mut r {
        cert.extend(extra.iter().cloned());
    }
    r
}

/// Certifies every row for the function `f`.
pub fn certify(f: &Fun<'_>, source: &str) -> Analysis {
    // Domain, f, f′ (and its fallback), f″ (and its fallback), the rows.
    let phase = f.left() / 8;
    f.allow(phase);
    let dom = side::domain(f);
    let w = window(f, &dom);
    // One period of a periodic f stands for the line; a domain with
    // excluded families otherwise gets a window.
    let periodic = periodic_window(f, &dom);
    // An undecided domain: the rows run where f's own tree is shown
    // defined in [-w, w], and list what they find there alone.
    let decided = dom.row.is_certified();
    let (win, period) = match periodic {
        Some((s, p)) => (Some((s, s + p.hi.0)), Some(p)),
        None if !dom.families.is_empty() || !decided => {
            let c = cut_window(&dom, w);
            (Some((-c, c)), None)
        }
        None => (None, None),
    };
    f.allow(phase);
    let (boxes, gaps, whole) = if decided {
        side::interior(&dom, win)
    } else {
        let (b, defined) = side::defined_boxes(f, w);
        (b, defined, false)
    };
    let defined: Vec<(f64, f64)> = gaps
        .iter()
        .filter_map(|c| match c {
            Claim::Defined { x } => Some((x.a.0, x.b.0)),
            _ => None,
        })
        .collect();
    let scope = rows::Scope {
        whole,
        window: win.unwrap_or((-w, w)),
        period,
        w,
    };
    // A closed end at a double is also decided on its own (a point box,
    // decided before the box it ends), so a zero right at the end is found
    // exactly and the box beside it can touch it.
    let mut spans: Vec<(f64, f64)> = Vec::new();
    for b in &boxes {
        let point_end = |bd: Bound, at: f64| matches!(bd, Bound::At { x, closed: true } if x.is_point() && x.lo.0 == at);
        if b.a < b.b && point_end(b.lo, b.a) && !b.lo_clipped {
            spans.push((b.a, b.a));
        }
        if b.a < b.b && point_end(b.hi, b.b) && !b.hi_clipped {
            spans.push((b.b, b.b));
        }
        spans.push((b.a, b.b));
    }
    f.allow(phase);
    // The kinks of f (abs, min, max), where its derivative trees are not
    // f′ (below). f's own boxes are cut at a kink placed at a double, so
    // f is decided there exactly and the boxes beside it can touch it
    // (|x − 3| is 0 at 3 alone).
    let kinks_found = if f.kinked {
        kinks(f, &spans, &f.kink_args())
    } else {
        Some(Vec::new())
    };
    // And where f′, f″ are singular (x^(1/3) at 0), f continuous.
    let singular = [singular_boxes(f, &spans, 1), singular_boxes(f, &spans, 2)];
    let mut spans0 = spans.clone();
    for &(l, r) in kinks_found.iter().flatten().chain(&singular[1]) {
        let p = l.next_up();
        if p.next_up() != r || !p.is_finite() {
            continue;
        }
        let p = if p == 0.0 { 0.0 } else { p };
        spans0 = spans0
            .into_iter()
            .flat_map(|(a, b)| {
                if a < p && p < b {
                    vec![(a, p), (p, p), (p, b)]
                } else {
                    vec![(a, b)]
                }
            })
            .collect();
    }
    f.allow(phase);
    let c0 = Cover::run(
        f,
        &Target {
            expr: &f.eval,
            k: 0,
            in_domain: true,
        },
        &[0.0],
        &spans0,
    );
    // f is continuous on every box: the derivative's own tree may stand
    // for f′ (it equals f′ wherever f is differentiable, and f is monotone
    // where it keeps a sign).
    let continuous = c0.complete()
        && c0.leaves.iter().all(|l| match *l {
            cover::Leaf::Band { cont, .. } | cover::Leaf::Equal { cont, .. } => cont,
            _ => true,
        });
    // For f′ and f″, a closed end where f isn't differentiable (√x at 0)
    // is left out: its box starts one double in, the reals between are a
    // gap (f⁽ᵏ⁾ must have no zero there), and f must be continuous across
    // it.
    let mut kgaps: [Vec<XBox>; 2] = [Vec::new(), Vec::new()];
    let mut kcont = [true, true];
    let mut kspans: [Vec<(f64, f64)>; 2] = [Vec::new(), Vec::new()];
    for (i, k) in [1usize, 2].into_iter().enumerate() {
        for &(a, b) in &spans {
            // (Point spans are the closed ends.)
            if a == b {
                let smooth = f
                    .ser(crate::interval::Interval::point(a), k)
                    .is_ok_and(|s| fun::usable(&s, k));
                if !smooth {
                    // Drop the point box; trim the box it ends.
                    continue;
                }
            }
            kspans[i].push((a, b));
        }
        // Trim boxes at dropped ends.
        let dropped: Vec<f64> = spans
            .iter()
            .filter(|&&(a, b)| a == b && !kspans[i].contains(&(a, b)))
            .map(|&(a, _)| a)
            .collect();
        for s in kspans[i].iter_mut() {
            if s.0 < s.1 && dropped.contains(&s.0) {
                let p = s.0;
                s.0 = p.next_up();
                kgaps[i].push(XBox::new(p, s.0));
            }
            if s.0 < s.1 && dropped.contains(&s.1) {
                let p = s.1;
                s.1 = p.next_down();
                kgaps[i].push(XBox::new(s.1, p));
            }
        }
        for g in &kgaps[i] {
            let v = f.val(crate::interval::Interval::new(g.a.0, g.b.0));
            if !v.is_ok_and(|v| !v.is_empty() && v.dec >= crate::interval::Dec::Dac) {
                kcont[i] = false;
            }
        }
    }
    // A kink of f (abs, min, max) is cut out of f′'s and f″'s boxes: their
    // trees, sign(u)·u′, equal f′ only off it. f must be continuous across
    // each (a Kink claim); a kink that can't be placed leaves f′ and f″ to
    // f's Taylor coefficients (undecided at the kink).
    let mut tree_ok = true;
    let kink_boxes: Vec<(f64, f64)> = if f.kinked {
        let ok = |&(l, r): &(f64, f64)| {
            f.val(crate::interval::Interval::new(l, r))
                .is_ok_and(|v| !v.is_empty() && v.dec >= crate::interval::Dec::Dac)
        };
        match kinks_found {
            Some(k) if k.iter().all(ok) => k,
            _ => {
                tree_ok = false;
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    // Per order: the kinks, and where that derivative is singular.
    let kbox: [Vec<(f64, f64)>; 2] = [0, 1].map(|i| {
        let mut b = kink_boxes.clone();
        b.extend(singular[i].iter().copied());
        merged(b)
    });
    for (i, sp) in kspans.iter_mut().enumerate() {
        *sp = split_at_kinks(sp, &kbox[i]);
    }
    let kink_claims_of = |i: usize| -> Vec<Claim> {
        kbox[i]
            .iter()
            .map(|&(l, r)| Claim::Kink { x: XBox::new(l, r) })
            .collect()
    };
    let mut kink_claims: [Vec<Claim>; 2] = [kink_claims_of(0), kink_claims_of(1)];
    // f⁽ᵏ⁾ from f's Taylor coefficients; if that leaves boxes undecided and
    // f is continuous, again from the derivative's own tree (for a rational
    // f its numerator is expanded exactly, so what cancels there cancels
    // exactly), keeping whichever cover is complete.
    let cover = |k: usize| {
        let sp = &kspans[k - 1];
        f.allow(phase);
        let c = Cover::run(
            f,
            &Target {
                expr: &f.eval,
                k,
                in_domain: true,
            },
            &[0.0],
            sp,
        );
        if continuous
            && tree_ok
            && !c.complete()
            && let Some(d) = f.derivs_off_kinks()
        {
            f.allow(phase);
            let r = Cover::run(
                f,
                &Target {
                    expr: &d[k - 1],
                    k: 0,
                    in_domain: true,
                },
                &[0.0],
                sp,
            );
            if r.complete() {
                return r;
            }
        }
        c
    };
    let (mut c1, mut c2) = (cover(1), cover(2));
    c1.kinks = kbox[0].clone();
    c2.kinks = kbox[1].clone();
    // A kink at a double, placed exactly with f′'s strict sign on each
    // side: a turn there is at exactly that point.
    for &(l, r) in &kbox[0] {
        let k = f.kink_at(l, r).ok().flatten();
        if let Some(k) = k {
            let claim = Claim::KinkAt {
                x: XBox::new(l, r),
                at: R(k.p),
                left: k.left,
                right: k.right,
            };
            kink_claims[0].push(claim.clone());
            kink_claims[1].push(claim);
        }
        c1.kink_at.push(k);
    }
    if std::env::var_os("CERTIFY_DEBUG").is_some() {
        eprintln!("w = {w}, boxes = {spans:?}");
        for (k, c) in [&c0, &c1, &c2].iter().enumerate() {
            eprintln!(
                "cover {k}: {} leaves, stopped {:?}, flags {:?}",
                c.leaves.len(),
                c.stopped,
                c.leaves
                    .iter()
                    .filter(|l| matches!(l, cover::Leaf::Flag { .. }))
                    .take(6)
                    .collect::<Vec<_>>()
            );
            if c.leaves.len() <= 12 {
                eprintln!("  {:?}", c.leaves);
            }
        }
    }
    f.allow(2 * phase);
    // What the boxes leave out, and the families cut from them: part of
    // every certificate built on the boxes.
    let mut gap_claims = gaps.clone();
    gap_claims.extend(
        dom.claims
            .iter()
            .filter(|c| matches!(c, Claim::Family { .. }))
            .cloned(),
    );
    // Is f⁽ᵏ⁾ free of zeros in the gaps? (Out of budget: not shown.)
    let gap_boxes: Vec<XBox> = gaps
        .iter()
        .filter_map(|c| match c {
            Claim::Gap { x } => Some(*x),
            _ => None,
        })
        .collect();
    // When it is, a GapClear claim for each gap: the rows on f⁽ᵏ⁾ rest on
    // them.
    let clear = |k: usize| -> (bool, Vec<Claim>) {
        let mut g = gap_boxes.clone();
        if k > 0 {
            if !kcont[k - 1] {
                return (false, Vec::new());
            }
            g.extend(kgaps[k - 1].iter().copied());
        }
        if !rows::gaps_clear(f, k, &g, &boxes, continuous).unwrap_or(false) {
            return (false, Vec::new());
        }
        let claims = g
            .iter()
            .map(|x| Claim::GapClear {
                x: *x,
                of: Subject::f(k),
            })
            .collect();
        (true, claims)
    };
    let ((clear0, gc0), (clear1, gc1), (clear2, gc2)) = (clear(0), clear(1), clear(2));
    if std::env::var_os("CERTIFY_DEBUG").is_some() {
        eprintln!("gaps {gap_boxes:?}: clear {clear0} {clear1} {clear2}");
        for g in &gap_boxes {
            let iv = crate::interval::Interval::new(g.a.0, g.b.0);
            eprintln!("  {g:?}: {:?}", f.ser(iv, 2).map(|s| s.to_vec()));
        }
    }
    if let Some(p) = period {
        gap_claims.push(Claim::Simplifier {
            fact: format!(
                "f(x + P) = f(x) wherever f is defined, P ∈ [{:e}, {:e}]",
                p.lo.0, p.hi.0
            ),
        });
    }
    // A row on f⁽ᵏ⁾ rests on its own gaps' clearness; one on f′ or f″ also
    // on the gaps where f⁽ᵏ⁾ is not claimed (an end f isn't
    // differentiable at) and on the kinks' claims.
    let order = |k: usize, gc: Vec<Claim>| -> Vec<Claim> {
        let mut out = gap_claims.clone();
        if k > 0 {
            out.extend(kgaps[k - 1].iter().map(|x| Claim::Gap { x: *x }));
            out.extend(kink_claims[k - 1].iter().cloned());
        }
        out.extend(gc);
        // (Each once, in linear time: a window cut by families holds
        // thousands.)
        let mut seen = std::collections::HashSet::new();
        out.retain(|c| seen.insert(format!("{c:?}")));
        out
    };
    let (claims0, claims1, claims2) = (order(0, gc0), order(1, gc1), order(2, gc2));
    let monotonicity = if decided {
        with(rows::monotonicity(&c1, &boxes, &scope, clear1), &claims1)
    } else {
        Row::unknown(rows::UNDECIDED)
    };
    let mut x_intercepts = rows::zeros(&c0, &scope, &claims0, clear0);
    let mut extrema = with(
        or_unknown(rows::extrema(f, &c1, &boxes, &scope, clear1)),
        &claims1,
    );
    let mut inflections = with(
        or_unknown(rows::inflections(f, &c2, &boxes, &scope, clear2)),
        &claims2,
    );
    if !decided {
        x_intercepts = rows::scoped(x_intercepts, &defined, false, |s| match s {
            cert::Spot::At(x) => Some(*x),
            cert::Spot::Every(_) => None,
        });
        extrema = rows::scoped(extrema, &defined, true, |e| {
            e.every.is_none().then_some(e.x)
        });
        inflections = rows::scoped(inflections, &defined, true, |e| {
            e.every.is_none().then_some(e.x)
        });
    }
    let horizontal = or_unknown(rows::horizontal(f, &dom, &scope));
    let (range, range_ends) = rows::range(f, &dom, &c0, &c1, &boxes, &scope, clear1)
        .unwrap_or_else(|s| (Row::unknown(stop(s)), Vec::new()));
    let oblique = or_unknown(rows::oblique(f, &dom, &horizontal, &scope));
    Analysis {
        source: source.to_string(),
        formula: f.expr.formula(),
        evaluated: evaluated(f),
        unit: unit_name(f.opts.trig_unit).into(),
        x_intercepts,
        y_intercept: or_unknown(rows::y_intercept(f, &dom)),
        parity: or_unknown(rows::parity(f, &dom)),
        period: or_unknown(rows::period(f, &dom, &monotonicity, w)),
        extrema,
        inflections,
        monotonicity,
        range: with(range, &claims1),
        range_ends,
        vertical: with(
            or_unknown(rows::vertical(f, &dom, &c0, &boxes, &scope)),
            &gap_claims,
        ),
        horizontal,
        oblique,
        domain: dom.row.clone(),
        binding: None,
        evals: f.evals(),
        stopped: [c0.stopped, c1.stopped, c2.stopped]
            .into_iter()
            .flatten()
            .next()
            .map(stop),
    }
}
