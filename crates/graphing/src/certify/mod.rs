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
use cover::{Cover, Target};
use serde::{Deserialize, Serialize};

/// Every row of a certified analysis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    /// The text analysed.
    pub source: String,
    /// The canonical tree every claim refers to (prefix notation).
    pub formula: String,
    /// "radians", "degrees" or "grads".
    pub unit: String,
    pub domain: Row<DomainValue>,
    pub x_intercepts: Row<Vec<Spot>>,
    pub y_intercept: Row<Option<Enc>>,
    pub parity: Row<Parity>,
    pub period: Row<Enc>,
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
    let f = Fun::new(expr, &lits, opts, budget, cancel);
    Ok(certify(&f, &text))
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

/// Certifies every row for the function `f`.
pub fn certify(f: &Fun<'_>, source: &str) -> Analysis {
    let phase = f.left() / 6;
    f.allow(phase);
    let dom = side::domain(f);
    let w = window(f, &dom);
    let (boxes, gaps, whole) = side::interior(&dom, w);
    // A closed end at a double is decided on its own (a point box), so a
    // zero or a value right at the end isn't lost among the first few
    // doubles beside it.
    let mut spans: Vec<(f64, f64)> = Vec::new();
    for b in &boxes {
        let (mut a, mut z) = (b.a, b.b);
        let point_end = |bd: Bound, at: f64| {
            matches!(bd, Bound::At { x, closed: true } if x.is_point() && x.lo.0 == at)
        };
        if a < z && point_end(b.lo, a) && !b.lo_clipped {
            spans.push((a, a));
            a = a.next_up();
        }
        if a < z && point_end(b.hi, z) && !b.hi_clipped {
            spans.push((z, z));
            z = z.next_down();
        }
        if a <= z {
            spans.push((a, z));
        }
    }
    let cover = |k: usize| {
        f.allow(phase);
        Cover::run(f, &Target { expr: &f.expr, k }, &[0.0], &spans)
    };
    let (c0, c1, c2) = (cover(0), cover(1), cover(2));
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
    let mut gap_claims = gaps.clone();
    gap_claims.extend(dom.claims.iter().filter(|c| matches!(c, Claim::Family { .. })).cloned());
    let analysis = Analysis {
        source: source.to_string(),
        formula: f.expr.formula(),
        unit: unit_name(f.opts.trig_unit).into(),
        x_intercepts: rows::zeros(&c0, &boxes, whole, w, &gap_claims),
        y_intercept: or_unknown(rows::y_intercept(f)),
        parity: or_unknown(rows::parity(f)),
        period: Row::unknown("periods are proven only by simplification"),
        extrema: or_unknown(rows::extrema(f, &c1, &boxes, whole, w)),
        inflections: or_unknown(rows::inflections(f, &c2, &boxes, whole, w)),
        monotonicity: rows::monotonicity(&c1, &boxes, whole, w),
        range: or_unknown(rows::range(f, &dom, &c0, &c1, &boxes, whole, w)),
        vertical: or_unknown(rows::vertical(f, &dom, &c0, &boxes, whole)),
        horizontal: or_unknown(rows::horizontal(f, &dom, whole, w)),
        domain: dom.row.clone(),
        evals: f.evals(),
        stopped: [c0.stopped, c1.stopped, c2.stopped]
            .into_iter()
            .flatten()
            .next()
            .map(stop),
    };
    analysis
}
