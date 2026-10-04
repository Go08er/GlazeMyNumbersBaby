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
//! Not wired into the panel yet: `crate::analysis` still produces what the
//! app shows.

pub mod cert;
pub mod cover;
pub mod fun;
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
    pub vertical: Row<Vec<Spot>>,
    pub horizontal: Row<Vec<Horizontal>>,
    /// Interval evaluations spent.
    pub evals: u64,
    /// Set if the budget ran out or the caller cancelled.
    pub stopped: Option<String>,
}

/// Evaluations per phase (domain, f, f′, f″, the rows) unless a budget is
/// given.
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
        if dom
            .families
            .iter()
            .any(|fam| !fam.members(start - margin, start + margin).is_empty())
        {
            continue;
        }
        let Ok(sr) = f.ser(crate::interval::Interval::point(start), 2) else {
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
    let Some((Axis::X, expr)) = eq.explicit() else {
        return Err("not a function of x".into());
    };
    let lits = Literals::of(&text, ParseOptions::default()).map_err(|e| format!("{e:?}"))?;
    let exact = ExactLiterals::of(&text, ParseOptions::default()).map_err(|e| format!("{e:?}"))?;
    let mut f = Fun::new(expr, &lits, opts, budget, cancel);
    f.exact = Some(&exact);
    if let Some(g) = rewrite(&f) {
        f.eval = canonical(&g);
    }
    // A symbolic derivative only for f continuous wherever defined (no
    // floor, round, sign, mod: their derivative 0 hides the jumps).
    let steps = !crate::simplify::side::jumps(&f.expr).is_empty();
    let settings = f.settings();
    f.derivs = fun::rational_derivs(&f.expr, &exact).or_else(|| {
        (!steps)
            .then(|| fun::symbolic_derivs(&f.eval, f.opts.trig_unit, settings.as_ref()))
            .flatten()
    });
    f.numerators = fun::rational_numerators(&f.expr, &exact);
    Ok(certify(&f, &text))
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
    if let Some([d1, d2]) = &f.derivs {
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
    let (win, period) = match periodic {
        Some((s, p)) => (Some((s, s + p.hi.0)), Some(p)),
        None if !dom.families.is_empty() => (Some((-w, w)), None),
        None => (None, None),
    };
    let (boxes, gaps, whole) = side::interior(&dom, win);
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
        let point_end = |bd: Bound, at: f64| {
            matches!(bd, Bound::At { x, closed: true } if x.is_point() && x.lo.0 == at)
        };
        if b.a < b.b && point_end(b.lo, b.a) && !b.lo_clipped {
            spans.push((b.a, b.a));
        }
        if b.a < b.b && point_end(b.hi, b.b) && !b.hi_clipped {
            spans.push((b.b, b.b));
        }
        spans.push((b.a, b.b));
    }
    f.allow(phase);
    let c0 = Cover::run(f, &Target { expr: &f.eval, k: 0 }, &[0.0], &spans);
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
    // f⁽ᵏ⁾ from f's Taylor coefficients; if that leaves boxes undecided and
    // f is continuous, again from the derivative's own tree (for a rational
    // f its numerator is expanded exactly, so what cancels there cancels
    // exactly), keeping whichever cover is complete.
    let cover = |k: usize| {
        let sp = &kspans[k - 1];
        f.allow(phase);
        let c = Cover::run(f, &Target { expr: &f.eval, k }, &[0.0], sp);
        match &f.derivs {
            Some(d) if continuous && !c.complete() => {
                f.allow(phase);
                let r = Cover::run(f, &Target { expr: &d[k - 1], k: 0 }, &[0.0], sp);
                if r.complete() { r } else { c }
            }
            _ => c,
        }
    };
    let (c1, c2) = (cover(1), cover(2));
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
            } else {
                let mut hist = std::collections::BTreeMap::new();
                for l in &c.leaves {
                    let (a, _) = l.span();
                    let key = if a == 0.0 { 0 } else { (a.signum() * (a.abs().log10().floor() + 400.0)) as i32 };
                    *hist.entry(key).or_insert(0) += 1;
                }
                eprintln!("  leaves by signed decade(+400): {hist:?}");
            }
        }
    }
    f.allow(2 * phase);
    // What the boxes leave out, and the families cut from them: part of
    // every certificate built on the boxes.
    let mut gap_claims = gaps.clone();
    gap_claims.extend(dom.claims.iter().filter(|c| matches!(c, Claim::Family { .. })).cloned());
    for g in kgaps.iter().flatten() {
        let c = Claim::Gap { x: *g };
        if !gap_claims.contains(&c) {
            gap_claims.push(c);
        }
    }
    // Is f⁽ᵏ⁾ free of zeros in the gaps? (Out of budget: not shown.)
    let gap_boxes: Vec<XBox> = gaps
        .iter()
        .filter_map(|c| match c {
            Claim::Gap { x } => Some(*x),
            _ => None,
        })
        .collect();
    let clear = |k: usize| {
        let mut g = gap_boxes.clone();
        if k > 0 {
            if !kcont[k - 1] {
                return false;
            }
            g.extend(kgaps[k - 1].iter().copied());
        }
        rows::gaps_clear(f, k, &g, &boxes, continuous).unwrap_or(false)
    };
    let (clear0, clear1, clear2) = (clear(0), clear(1), clear(2));
    if std::env::var_os("CERTIFY_DEBUG").is_some() {
        eprintln!("gaps {gap_boxes:?}: clear {clear0} {clear1} {clear2}");
        for g in &gap_boxes {
            let iv = crate::interval::Interval::new(g.a.0, g.b.0);
            eprintln!("  {g:?}: {:?}", f.ser(iv, 2).map(|s| s.to_vec()));
        }
    }
    if let Some(p) = period {
        gap_claims.push(Claim::Simplifier {
            fact: format!("f(x + P) = f(x) wherever f is defined, P ∈ [{:e}, {:e}]", p.lo.0, p.hi.0),
        });
    }
    let monotonicity = with(rows::monotonicity(&c1, &boxes, &scope, clear1), &gap_claims);
    Analysis {
        source: source.to_string(),
        formula: f.expr.formula(),
        evaluated: evaluated(f),
        unit: unit_name(f.opts.trig_unit).into(),
        x_intercepts: rows::zeros(&c0, &scope, &gap_claims, clear0),
        y_intercept: or_unknown(rows::y_intercept(f)),
        parity: or_unknown(rows::parity(f, &dom)),
        period: or_unknown(rows::period(f, &dom, &monotonicity)),
        extrema: with(or_unknown(rows::extrema(f, &c1, &boxes, &scope, clear1)), &gap_claims),
        inflections: with(
            or_unknown(rows::inflections(f, &c2, &boxes, &scope, clear2)),
            &gap_claims,
        ),
        monotonicity,
        range: with(
            or_unknown(rows::range(f, &dom, &c0, &c1, &boxes, &scope, clear1)),
            &gap_claims,
        ),
        vertical: with(or_unknown(rows::vertical(f, &dom, &c0, &boxes, &scope)), &gap_claims),
        horizontal: or_unknown(rows::horizontal(f, &dom, &scope)),
        domain: dom.row.clone(),
        evals: f.evals(),
        stopped: [c0.stopped, c1.stopped, c2.stopped]
            .into_iter()
            .flatten()
            .next()
            .map(stop),
    }
}
