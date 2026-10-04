//! An independent replay of the certified analysis' certificates.
//!
//! The certifier (`graphing::certify`) proves each row of the function
//! analysis with its own interval arithmetic and writes down what it
//! proved as claims about boxes of x. The replay reads a certificate as
//! plain JSON and checks it with code of its own: an MPFR interval type
//! with directed rounding ([`iv`]), Taylor series by automatic
//! differentiation ([`series`]), the graphing language's semantics written
//! from its specification ([`eval`]), exact rationals ([`exact`]). Nothing
//! here calls the certifier, the interval core or the simplifier; only the
//! parser, for the tree of the source text.
//!
//! Each claim comes out as one of:
//!
//! * **strong**: the replay proved it on the canonical tree of the source;
//! * **weak**: proved only on a tree the certifier supplied (the
//!   simplifier's form of f, the derivative trees), or tested at points (a
//!   simplifier fact): it rests on the simplifier being right;
//! * **unconfirmed**: neither proved nor refuted within the replay's
//!   budget and precision;
//! * **unsupported**: about something the replay doesn't model;
//! * **refuted**: the replay proved it false.
//!
//! Then each row: its claims' boxes must cover the region it declares,
//! and its value must follow from them ([`rows`]).

#![allow(dead_code)]

pub mod algebra;
pub mod claims;
pub mod eval;
pub mod exact;
pub mod growth;
pub mod iv;
pub mod rows;
pub mod series;

use eval::Lits;
use graphing::ast::Expr;
use iv::Unit;
use serde_json::Value;
use std::collections::BTreeMap;

// ------------------------------------------------------------ certificates

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    Itself,
    CosOfArg,
    SinOfArg,
}

/// What a claim is about: f⁽ᵏ⁾, or a side expression of the tree.
#[derive(Clone, Debug, PartialEq)]
pub enum Subject {
    F(usize),
    G(Vec<u8>, Via),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// A closed box of x.
pub type B = (f64, f64);

#[derive(Clone, Debug, PartialEq)]
pub enum Claim {
    Defined(B),
    Continuous(B),
    Undefined(B),
    Beyond {
        x: B,
        of: Subject,
        c: f64,
        above: bool,
    },
    NoCross {
        x: B,
        of: Subject,
        c: f64,
        above: bool,
    },
    OneCross {
        x: B,
        of: Subject,
        c: f64,
    },
    ExactAt {
        x: B,
        of: Subject,
        c: f64,
        at: f64,
    },
    Factors {
        x: B,
        of: Subject,
        c: f64,
        above: bool,
    },
    Touch {
        x: B,
        of: Subject,
        c: f64,
        at: f64,
        order: usize,
        above: bool,
    },
    Value {
        x: B,
        of: Subject,
        lo: f64,
        hi: f64,
    },
    Family {
        of: Subject,
        x0: B,
        period: B,
    },
    TailBeyond {
        side: Side,
        from: f64,
        of: Subject,
        c: f64,
        above: bool,
    },
    TailChain {
        side: Side,
        from: f64,
        of: Subject,
        c: f64,
        above: bool,
        order: usize,
    },
    TailValue {
        side: Side,
        from: f64,
        lo: f64,
        hi: f64,
    },
    Unbounded {
        near: B,
        at: B,
    },
    /// f is defined and continuous on the box (a kink of f lies in it;
    /// nothing is claimed about f′, f″ there).
    Kink(B),
    /// The kink in the box is at exactly the double `at`, with f′ (of the
    /// smooth form f takes on each side) strictly positive (`true`) or
    /// negative on [x.0, at] (`left`) and on [at, x.1] (`right`).
    KinkAt {
        x: B,
        at: f64,
        left: bool,
        right: bool,
    },
    /// Wherever f is defined on `near` (which holds `at`), its values lie
    /// in [lo, hi].
    Bounded {
        near: B,
        at: B,
        lo: f64,
        hi: f64,
    },
    Removable {
        near: B,
        at: B,
        lo: f64,
        hi: f64,
    },
    Simplifier(String),
    Gap(B),
    /// The subject has no zero strictly inside the gap box, or is 0
    /// throughout it, wherever it is valid there.
    GapClear {
        x: B,
        of: Subject,
    },
    /// As x → `at`, f (or f/x, `over_x`) tends to `to`, by the tree's
    /// structure (`growth`).
    Limit {
        at: growth::At,
        over_x: bool,
        to: Toward,
    },
}

/// Where a limit claim says f goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Toward {
    PosInf,
    NegInf,
    In(f64, f64),
}

impl Claim {
    pub fn kind(&self) -> &'static str {
        match self {
            Claim::Defined(_) => "Defined",
            Claim::Continuous(_) => "Continuous",
            Claim::Undefined(_) => "Undefined",
            Claim::Beyond { .. } => "Beyond",
            Claim::NoCross { .. } => "NoCross",
            Claim::OneCross { .. } => "OneCross",
            Claim::ExactAt { .. } => "ExactAt",
            Claim::Factors { .. } => "Factors",
            Claim::Touch { .. } => "Touch",
            Claim::Value { .. } => "Value",
            Claim::Family { .. } => "Family",
            Claim::TailBeyond { .. } => "TailBeyond",
            Claim::TailChain { .. } => "TailChain",
            Claim::TailValue { .. } => "TailValue",
            Claim::Unbounded { .. } => "Unbounded",
            Claim::Bounded { .. } => "Bounded",
            Claim::Kink(_) => "Kink",
            Claim::KinkAt { .. } => "KinkAt",
            Claim::Removable { .. } => "Removable",
            Claim::Simplifier(_) => "Simplifier",
            Claim::Gap(_) => "Gap",
            Claim::GapClear { .. } => "GapClear",
            Claim::Limit { .. } => "Limit",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Region {
    Line,
    Window(f64, f64),
    Period(f64, f64),
    Points,
}

/// One row as the certificate gives it.
#[derive(Clone, Debug)]
pub struct RowCert {
    pub name: String,
    /// "Certified", "Partial" or "Unknown".
    pub status: String,
    pub value: Value,
    pub region: Option<Region>,
    pub claims: Vec<Claim>,
    /// What else the analysis says about the row (the range's
    /// `range_ends`).
    pub extra: Value,
}

pub const ROWS: [&str; 12] = [
    "domain",
    "x_intercepts",
    "y_intercept",
    "parity",
    "period",
    "extrema",
    "inflections",
    "monotonicity",
    "range",
    "vertical",
    "horizontal",
    "oblique",
];

/// A double written by the certificate (`"inf"`, `"-inf"`, `"nan"` for
/// the rest).
pub fn r(v: &Value) -> Result<f64, String> {
    match v {
        Value::Number(n) => n.as_f64().ok_or_else(|| format!("bad number {n}")),
        Value::String(s) => match s.as_str() {
            "inf" => Ok(f64::INFINITY),
            "-inf" => Ok(f64::NEG_INFINITY),
            "nan" => Ok(f64::NAN),
            _ => Err(format!("bad number {s:?}")),
        },
        _ => Err(format!("bad number {v}")),
    }
}

fn field<'v>(v: &'v Value, k: &str) -> Result<&'v Value, String> {
    v.get(k).ok_or_else(|| format!("missing {k} in {v}"))
}

fn rf(v: &Value, k: &str) -> Result<f64, String> {
    r(field(v, k)?)
}

fn bf(v: &Value, k: &str) -> Result<bool, String> {
    field(v, k)?
        .as_bool()
        .ok_or_else(|| format!("{k} not a bool"))
}

fn xbox(v: &Value) -> Result<B, String> {
    Ok((rf(v, "a")?, rf(v, "b")?))
}

fn side(v: &Value) -> Result<Side, String> {
    match v.as_str() {
        Some("Left") => Ok(Side::Left),
        Some("Right") => Ok(Side::Right),
        _ => Err(format!("bad tail side {v}")),
    }
}

fn subject(v: &Value) -> Result<Subject, String> {
    if let Some(o) = v.get("F") {
        return match o.as_str() {
            Some("F0") => Ok(Subject::F(0)),
            Some("F1") => Ok(Subject::F(1)),
            Some("F2") => Ok(Subject::F(2)),
            _ => Err(format!("bad order {o}")),
        };
    }
    if let Some(g) = v.get("G") {
        let path = field(g, "path")?
            .as_array()
            .ok_or("path not an array")?
            .iter()
            .map(|p| p.as_u64().map(|p| p as u8).ok_or("bad path"))
            .collect::<Result<Vec<u8>, _>>()?;
        let via = match field(g, "via")?.as_str() {
            Some("Itself") => Via::Itself,
            Some("CosOfArg") => Via::CosOfArg,
            Some("SinOfArg") => Via::SinOfArg,
            other => return Err(format!("bad via {other:?}")),
        };
        return Ok(Subject::G(path, via));
    }
    Err(format!("bad subject {v}"))
}

pub fn claim(v: &Value) -> Result<Claim, String> {
    let o = v
        .as_object()
        .ok_or_else(|| format!("claim not an object: {v}"))?;
    let (kind, b) = o.iter().next().ok_or("empty claim")?;
    let of = || subject(field(b, "of")?);
    let x = || xbox(field(b, "x")?);
    Ok(match kind.as_str() {
        "Defined" => Claim::Defined(x()?),
        "Continuous" => Claim::Continuous(x()?),
        "Undefined" => Claim::Undefined(x()?),
        "Beyond" => Claim::Beyond {
            x: x()?,
            of: of()?,
            c: rf(b, "c")?,
            above: bf(b, "above")?,
        },
        "NoCross" => Claim::NoCross {
            x: x()?,
            of: of()?,
            c: rf(b, "c")?,
            above: bf(b, "above")?,
        },
        "OneCross" => Claim::OneCross {
            x: x()?,
            of: of()?,
            c: rf(b, "c")?,
        },
        "ExactAt" => Claim::ExactAt {
            x: x()?,
            of: of()?,
            c: rf(b, "c")?,
            at: rf(b, "at")?,
        },
        "Factors" => Claim::Factors {
            x: x()?,
            of: of()?,
            c: rf(b, "c")?,
            above: bf(b, "above")?,
        },
        "Touch" => Claim::Touch {
            x: x()?,
            of: of()?,
            c: rf(b, "c")?,
            at: rf(b, "at")?,
            order: field(b, "order")?.as_u64().ok_or("bad order")? as usize,
            above: bf(b, "above")?,
        },
        "Value" => Claim::Value {
            x: x()?,
            of: of()?,
            lo: rf(b, "lo")?,
            hi: rf(b, "hi")?,
        },
        "Family" => Claim::Family {
            of: of()?,
            x0: xbox(field(b, "x0")?)?,
            period: xbox(field(b, "period")?)?,
        },
        "TailBeyond" => Claim::TailBeyond {
            side: side(field(b, "side")?)?,
            from: rf(b, "from")?,
            of: of()?,
            c: rf(b, "c")?,
            above: bf(b, "above")?,
        },
        "TailChain" => Claim::TailChain {
            side: side(field(b, "side")?)?,
            from: rf(b, "from")?,
            of: of()?,
            c: rf(b, "c")?,
            above: bf(b, "above")?,
            order: field(b, "order")?.as_u64().ok_or("bad order")? as usize,
        },
        "TailValue" => Claim::TailValue {
            side: side(field(b, "side")?)?,
            from: rf(b, "from")?,
            lo: rf(b, "lo")?,
            hi: rf(b, "hi")?,
        },
        "Unbounded" => Claim::Unbounded {
            near: xbox(field(b, "near")?)?,
            at: xbox(field(b, "at")?)?,
        },
        "Kink" => Claim::Kink(x()?),
        "KinkAt" => Claim::KinkAt {
            x: x()?,
            at: rf(b, "at")?,
            left: bf(b, "left")?,
            right: bf(b, "right")?,
        },
        "Bounded" => Claim::Bounded {
            near: xbox(field(b, "near")?)?,
            at: xbox(field(b, "at")?)?,
            lo: rf(b, "lo")?,
            hi: rf(b, "hi")?,
        },
        "Removable" => Claim::Removable {
            near: xbox(field(b, "near")?)?,
            at: xbox(field(b, "at")?)?,
            lo: rf(b, "lo")?,
            hi: rf(b, "hi")?,
        },
        "Simplifier" => Claim::Simplifier(
            field(b, "fact")?
                .as_str()
                .ok_or("fact not a string")?
                .to_string(),
        ),
        "Gap" => Claim::Gap(x()?),
        "GapClear" => Claim::GapClear { x: x()?, of: of()? },
        "Limit" => {
            let at = field(b, "at")?;
            let at = match at.as_str() {
                Some("PosInf") => growth::At::PosInf,
                Some("NegInf") => growth::At::NegInf,
                _ => match (at.get("Right"), at.get("Left")) {
                    (Some(p), _) => growth::At::Right(r(p)?),
                    (_, Some(p)) => growth::At::Left(r(p)?),
                    _ => return Err(format!("bad limit point {at}")),
                },
            };
            let to = field(b, "to")?;
            let to = match to.as_str() {
                Some("PosInf") => Toward::PosInf,
                Some("NegInf") => Toward::NegInf,
                _ => {
                    let i = field(to, "In")?;
                    Toward::In(rf(i, "lo")?, rf(i, "hi")?)
                }
            };
            Claim::Limit {
                at,
                over_x: bf(b, "over_x")?,
                to,
            }
        }
        other => return Err(format!("unknown claim kind {other}")),
    })
}

fn region(v: &Value) -> Result<Region, String> {
    if let Some(s) = v.as_str() {
        return match s {
            "Line" => Ok(Region::Line),
            "Points" => Ok(Region::Points),
            _ => Err(format!("bad region {s}")),
        };
    }
    if let Some(w) = v.get("Window") {
        return Ok(Region::Window(rf(w, "a")?, rf(w, "b")?));
    }
    if let Some(w) = v.get("Period") {
        return Ok(Region::Period(rf(w, "a")?, rf(w, "b")?));
    }
    Err(format!("bad region {v}"))
}

fn row(name: &str, v: &Value) -> Result<RowCert, String> {
    let o = v.as_object().ok_or("row not an object")?;
    let (status, b) = o.iter().next().ok_or("empty row")?;
    if status == "Unknown" {
        return Ok(RowCert {
            name: name.into(),
            status: status.clone(),
            value: b.get("reason").cloned().unwrap_or(Value::Null),
            region: None,
            claims: Vec::new(),
            extra: Value::Null,
        });
    }
    let cert = field(b, "cert")?;
    Ok(RowCert {
        name: name.into(),
        status: status.clone(),
        value: field(b, "value")?.clone(),
        region: Some(region(field(cert, "covers")?)?),
        claims: field(cert, "claims")?
            .as_array()
            .ok_or("claims not an array")?
            .iter()
            .map(claim)
            .collect::<Result<_, _>>()?,
        extra: Value::Null,
    })
}

// ------------------------------------------------------------ the function

/// The function a certificate is about, as the replay reads it.
pub struct Fx {
    pub source: String,
    /// The canonical tree of the source (the claims' tree).
    pub f: Expr,
    /// The simplifier's form of f the certifier enclosed values with.
    pub f_eval: Option<Expr>,
    /// The certifier's trees of f′ and f″ (a rational f's).
    pub d: [Option<Expr>; 2],
    pub unit: Unit,
    pub lits: Lits,
    pub vars: Vec<(String, f64)>,
    /// Which of the certifier's trees are exactly f's.
    pub verified: algebra::Verified,
    /// f's tree with each sum of monomials of degree ≥ 2 in Horner's form
    /// (the replay's own rearrangement of + and ·: the same function, with
    /// the same domain), where f's tree is ∞ − ∞ on a tail.
    pub f_alt: Option<Expr>,
}

impl Fx {
    pub fn ctx(&self) -> eval::Ctx<'_> {
        eval::Ctx {
            unit: self.unit,
            lits: &self.lits,
            vars: &self.vars,
        }
    }

    /// `e`'s Taylor series over `[lo, hi]` to order n.
    pub fn series(&self, e: &Expr, lo: f64, hi: f64, n: usize) -> series::S {
        eval::series(e, lo, hi, n, &self.ctx())
    }

    /// The same over an x interval of the replay's own (strictly signed,
    /// say: the reals beside an excluded 0, 0 left out).
    pub fn series_iv(&self, e: &Expr, x: iv::Iv, n: usize) -> series::S {
        let xs = series::var(x, n);
        eval::eval(e, &xs, n, &self.ctx())
    }
}

/// Reads the source and the trees the analysis names; checks that the
/// source's canonical tree is the formula the claims are about.
pub fn function(a: &Value) -> Result<Fx, String> {
    let source = field(a, "source")?.as_str().ok_or("source")?.to_string();
    let formula = field(a, "formula")?.as_str().ok_or("formula")?;
    let unit = Unit::parse(field(a, "unit")?.as_str().ok_or("unit")?).ok_or("bad unit")?;
    let text = if source.contains('=') {
        source.clone()
    } else {
        format!("y={source}")
    };
    // The source read as it was typed (a decimal comma, if the binding
    // says so).
    let comma = a
        .get("binding")
        .and_then(|b| b.get("decimal_comma"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let po = graphing::lexer::ParseOptions {
        decimal_comma: comma,
    };
    let eq = graphing::Equation::parse_with(&text, po).map_err(|e| format!("parse: {e:?}"))?;
    let Some((graphing::equation::Axis::X, expr)) = eq.explicit() else {
        return Err("the source is not y = f(x)".into());
    };
    let f = eval::canonical(expr);
    if f.formula() != formula {
        return Err(format!(
            "binding: the source's tree {} is not the certificate's formula {formula}",
            f.formula()
        ));
    }
    let mut f_eval = None;
    let mut d = [None, None];
    if let Some(ev) = a.get("evaluated").and_then(Value::as_str) {
        for part in ev.split("; ") {
            let (name, tree) = part.split_once(" = ").ok_or("bad evaluated")?;
            let t = eval::parse_formula(tree)
                .ok_or_else(|| format!("unreadable evaluated tree {tree}"))?;
            match name {
                "f" => f_eval = Some(t),
                "f′" => d[0] = Some(t),
                "f″" => d[1] = Some(t),
                _ => return Err(format!("unknown evaluated tree {name}")),
            }
        }
    }
    let vars = match a.get("binding").and_then(|b| b.get("sliders")) {
        Some(Value::Object(m)) => m
            .iter()
            .filter_map(|(k, v)| v.as_f64().map(|v| (k.clone(), v)))
            .collect(),
        _ => Vec::new(),
    };
    let lits = Lits::of_with(&text, comma);
    let f_alt = eval::horner(&f);
    let verified = algebra::verify(
        &f,
        f_eval.as_ref(),
        [d[0].as_ref(), d[1].as_ref()],
        &lits,
        &vars,
        unit,
    );
    Ok(Fx {
        lits,
        source,
        f,
        f_eval,
        d,
        unit,
        vars,
        verified,
        f_alt,
    })
}

// ------------------------------------------------------------ outcomes

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    Strong,
    Weak,
    Unconfirmed,
    Unsupported,
    Refuted,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Strong => "strong",
            Class::Weak => "weak",
            Class::Unconfirmed => "unconfirmed",
            Class::Unsupported => "unsupported",
            Class::Refuted => "refuted",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub class: Class,
    pub note: String,
}

impl Outcome {
    pub fn new(class: Class, note: impl Into<String>) -> Outcome {
        Outcome {
            class,
            note: note.into(),
        }
    }
}

/// One claim's result.
#[derive(Clone, Debug)]
pub struct ClaimResult {
    pub rows: Vec<String>,
    pub claim: Claim,
    pub outcome: Outcome,
}

/// One row's coverage and derivation.
#[derive(Clone, Debug)]
pub struct RowResult {
    pub name: String,
    pub status: String,
    /// Problems found (empty: the row follows from its claims).
    pub problems: Vec<String>,
    /// Weaker points (evidence that rests on the simplifier).
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub source: String,
    pub claims: Vec<ClaimResult>,
    pub rows: Vec<RowResult>,
    pub binding: Vec<String>,
    /// The certifier's trees against f's: identical exactly, agreeing at
    /// points, or (a problem) apart somewhere.
    pub trees: Vec<String>,
    pub tree_problems: Vec<String>,
}

impl Report {
    pub fn count(&self) -> BTreeMap<(&'static str, Class), usize> {
        let mut m = BTreeMap::new();
        for c in &self.claims {
            *m.entry((c.claim.kind(), c.outcome.class)).or_insert(0) += 1;
        }
        m
    }

    pub fn refuted(&self) -> Vec<&ClaimResult> {
        self.claims
            .iter()
            .filter(|c| c.outcome.class == Class::Refuted)
            .collect()
    }

    /// The rows' problems, and a certifier's tree shown wrong.
    pub fn row_problems(&self) -> Vec<String> {
        self.rows
            .iter()
            .flat_map(|r| r.problems.iter().map(move |p| format!("{}: {p}", r.name)))
            .chain(self.tree_problems.iter().map(|p| format!("trees: {p}")))
            .collect()
    }
}

/// Replays a whole analysis (`graphing::certify::Analysis` as JSON).
pub fn replay(a: &Value) -> Result<Report, String> {
    iv::init();
    let fx = function(a)?;
    let mut rep = Report {
        source: fx.source.clone(),
        ..Default::default()
    };
    rep.binding = binding(a)?;
    let mut rows_in = Vec::new();
    for name in ROWS {
        // (A row an older certifier didn't write is unknown.)
        let mut rc = match a.get(name) {
            Some(v) => row(name, v)?,
            None => row(
                name,
                &serde_json::json!({"Unknown": {"reason": "not in the certificate"}}),
            )?,
        };
        if name == "range" {
            rc.extra = a.get("range_ends").cloned().unwrap_or(Value::Null);
        }
        rows_in.push(rc);
    }
    // Each distinct claim once (the gaps go with every row); a gap is read
    // against all the certificate's claims (the families it may hold).
    let all: Vec<Claim> = rows_in.iter().flat_map(|rc| rc.claims.clone()).collect();
    trees(&fx, &all, &mut rep);
    let mut seen: Vec<(Claim, usize)> = Vec::new();
    for rc in &rows_in {
        for c in &rc.claims {
            if let Some((_, i)) = seen.iter().find(|(d, _)| d == c) {
                let i = *i;
                if !rep.claims[i].rows.contains(&rc.name) {
                    rep.claims[i].rows.push(rc.name.clone());
                }
                continue;
            }
            let outcome = claims::check(&fx, c, &all);
            seen.push((c.clone(), rep.claims.len()));
            rep.claims.push(ClaimResult {
                rows: vec![rc.name.clone()],
                claim: c.clone(),
                outcome,
            });
        }
    }
    for rc in &rows_in {
        rep.rows.push(rows::check(&fx, rc, &rows_in, &rep.claims));
    }
    if fx.lits.untyped() > 0 {
        rep.binding.push(format!(
            "{} numbers in the certifier's trees were not typed decimals",
            fx.lits.untyped()
        ));
    }
    Ok(rep)
}

/// The power convention the replay implements (`eval`'s rules).
pub const POWER: &str = "ti-real-1";

/// The simplifier rule set the replay's weak evidence was last reviewed
/// against (its hash in the binding); another is noted, not refused.
pub const RULES_SEEN: &str = "32045d7ad9dab698";

/// What the certificate is bound to. The source, unit and formula are
/// checked by [`function`]; here the binding: made by this certifier,
/// under the power convention the replay implements (else refused), with
/// its rule set and sliders noted. Returns the notes.
fn binding(a: &Value) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let Some(b) = a.get("binding").filter(|b| !b.is_null()) else {
        out.push("missing: no binding (certifier, rule set, power convention, sliders)".into());
        return Ok(out);
    };
    let get = |k: &str| b.get(k).ok_or_else(|| format!("binding: no {k}"));
    let certifier = get("certifier")?.as_str().unwrap_or_default();
    let ours = format!("graphing {}", env!("CARGO_PKG_VERSION"));
    if certifier != ours {
        return Err(format!(
            "binding: made by {certifier:?}, the replay is for {ours:?}"
        ));
    }
    let power = get("power")?.as_str().unwrap_or_default();
    if power != POWER {
        return Err(format!(
            "binding: power convention {power:?}, the replay implements {POWER:?}"
        ));
    }
    let rules = get("rules")?.as_str().unwrap_or_default();
    if rules.len() != 16 || !rules.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("binding: rule set {rules:?} is no hash"));
    }
    if !RULES_SEEN.is_empty() && rules != RULES_SEEN {
        out.push(format!(
            "rule set {rules} (the replay last saw {RULES_SEEN})"
        ));
    }
    if !get("sliders")?.is_object() {
        return Err("binding: sliders not a map".into());
    }
    Ok(out)
}

/// The certifier's trees (f's simplified form, f′'s and f″'s) against f:
/// shown identical exactly ([`algebra`]), else compared at points where
/// f is defined (and k times differentiable): near 0, ordinary, far out
/// (to 10²⁴⁰), and near every point the claims single out (excluded
/// points, holes, domain ends). Enclosures apart at a point prove the tree
/// wrong there.
fn trees(fx: &Fx, claims: &[Claim], rep: &mut Report) {
    use claims::{Subj, Tree};
    let names = ["f's simplified form", "f′'s tree", "f″'s tree"];
    let present = [fx.f_eval.is_some(), fx.d[0].is_some(), fx.d[1].is_some()];
    let exact = [fx.verified.f, fx.verified.d[0], fx.verified.d[1]];
    let mut xs: Vec<f64> = vec![0.0];
    let mut t = 1.0 / 1024.0;
    while t < 1e7 {
        xs.push(t);
        xs.push(-t * 1.37);
        t *= 2.9;
    }
    for far in [1.3e9, 2.9e12, 1.37e15, 3.7e30, 1.1e60, 7.7e120, 2.3e240] {
        xs.push(far);
        xs.push(-far);
    }
    // Beside the points the claims single out.
    let mut marks: Vec<f64> = Vec::new();
    for c in claims {
        match c {
            Claim::Removable { at, .. }
            | Claim::Bounded { at, .. }
            | Claim::Unbounded { at, .. }
            | Claim::Gap(at)
            | Claim::Undefined(at) => marks.extend([at.0, at.1]),
            _ => {}
        }
    }
    marks.retain(|m| m.is_finite());
    marks.sort_by(f64::total_cmp);
    marks.dedup();
    for p in marks.into_iter().take(16) {
        let s = p.abs().max(1.0);
        for d in [1e-3 * s, 1e-6 * s, 1e-9 * s] {
            xs.extend([p - d, p + d]);
        }
        let (mut lo, mut hi) = (p, p);
        for _ in 0..4 {
            lo = lo.next_down();
            hi = hi.next_up();
        }
        xs.extend([lo, hi]);
    }
    for k in 0..3 {
        if !present[k] {
            continue;
        }
        if exact[k] {
            rep.trees
                .push(format!("{}: identical to f's exactly", names[k]));
            continue;
        }
        // The tree for f⁽ᵏ⁾: f's simplified form differentiated k times
        // (k = 0), or the certifier's derivative tree.
        let tree_subj = if k == 0 {
            Subj::new(fx, &Subject::F(0), Tree::Eval)
        } else {
            let mut s = Subj::new(fx, &Subject::F(k), Tree::Orig).expect("f");
            s.e = fx.d[k - 1].clone().expect("present");
            s.k = 0;
            Some(s)
        };
        let Some(ts) = tree_subj else { continue };
        let fs = Subj::new(fx, &Subject::F(k), Tree::Orig).expect("f");
        let mut agree = 0;
        iv::set_prec(160);
        for &x in &xs {
            let (a, valid) = fs.at(x);
            let b = fx.series(&ts.e, x, x, ts.k)[ts.k].clone();
            let b = if ts.k > 0 {
                iv::mul(&b, &iv::Iv::of((1..=ts.k).product::<usize>() as f64))
            } else {
                b
            };
            if !valid || a.empty || !a.def || b.empty || !b.def {
                continue;
            }
            if a.hi < b.lo || b.hi < a.lo {
                rep.tree_problems.push(format!(
                    "{} is wrong at x = {x:e}: f{} ∈ [{:e}, {:e}], the tree gives [{:e}, {:e}]",
                    names[k],
                    ["", "′", "″"][k],
                    a.lo.to_f64(),
                    a.hi.to_f64(),
                    b.lo.to_f64(),
                    b.hi.to_f64()
                ));
                break;
            }
            agree += 1;
        }
        rep.trees
            .push(format!("{}: agrees with f's at {agree} points", names[k]));
    }
}
