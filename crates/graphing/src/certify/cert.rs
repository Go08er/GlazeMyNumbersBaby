//! Certificates: what each certified row rests on, written as claims about
//! boxes of x that a separate checker can replay with its own interval
//! arithmetic.
//!
//! Every claim is about the canonical tree of the function (see
//! [`super::canonical`]) under the analysis' angle unit, with the typed
//! literals read back exactly (`interval::Literals`). A box is a closed
//! interval of doubles `[a, b]`; a point is `a = b`. Sub-expressions are
//! named by their *path*: child indices from the root (`Neg`/`Degrees`
//! have child 0, a binary node 0 and 1, a call its arguments in order).
//!
//! A row's certificate also says what region its claims must cover: the
//! whole line (a complete answer) or a window (a list that is correct but
//! may not be complete). The replay checks every claim and that the
//! claims' boxes and tails cover the region, gaps included.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A double that survives JSON: finite values as numbers, infinities and
/// NaN as the strings `"inf"`, `"-inf"`, `"nan"`.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct R(pub f64);

impl Serialize for R {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let v = self.0;
        if v.is_finite() {
            s.serialize_f64(v)
        } else if v.is_nan() {
            s.serialize_str("nan")
        } else if v > 0.0 {
            s.serialize_str("inf")
        } else {
            s.serialize_str("-inf")
        }
    }
}

impl<'de> Deserialize<'de> for R {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<R, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            N(f64),
            S(String),
        }
        Ok(R(match Raw::deserialize(d)? {
            Raw::N(v) => v,
            Raw::S(s) => match s.as_str() {
                "inf" => f64::INFINITY,
                "-inf" => f64::NEG_INFINITY,
                "nan" => f64::NAN,
                other => {
                    return Err(serde::de::Error::custom(format!("not a number: {other}")));
                }
            },
        }))
    }
}

/// A closed box of x, `[a, b]` with `a ≤ b` (finite).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct XBox {
    pub a: R,
    pub b: R,
}

impl XBox {
    pub fn new(a: f64, b: f64) -> XBox {
        XBox { a: R(a), b: R(b) }
    }

    pub fn point(p: f64) -> XBox {
        XBox::new(p, p)
    }
}

/// Which function a claim is about: f, f′ or f″.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Order {
    F0,
    F1,
    F2,
}

impl Order {
    pub fn k(self) -> usize {
        match self {
            Order::F0 => 0,
            Order::F1 => 1,
            Order::F2 => 2,
        }
    }

    pub fn of(k: usize) -> Order {
        match k {
            0 => Order::F0,
            1 => Order::F1,
            _ => Order::F2,
        }
    }
}

/// Which tail: `(−∞, from]` or `[from, ∞)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tail {
    Left,
    Right,
}

/// How a side expression g is read off the node at its path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Via {
    /// g is the node itself.
    Itself,
    /// g = cos(u) for the node's argument u (the node is tan or sec), in
    /// the analysis' unit.
    CosOfArg,
    /// g = sin(u) for the node's argument u (the node is cot or csc).
    SinOfArg,
}

/// What a claim is about: f, f′ or f″, or a side expression g whose value
/// decides where f is defined.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Subject {
    F(Order),
    G { path: Vec<u8>, via: Via },
}

impl Subject {
    pub fn f(k: usize) -> Subject {
        Subject::F(Order::of(k))
    }

    /// The derivative order the claim needs (0 for a side expression).
    pub fn k(&self) -> usize {
        match self {
            Subject::F(o) => o.k(),
            Subject::G { .. } => 0,
        }
    }
}

/// One fact about a box (or a tail) of x. "s is valid on the box" means:
/// for f⁽ᵏ⁾, f is defined and continuous there and its derivatives up to
/// k exist (for k = 0: f is defined); for a side expression g, g is
/// defined there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Claim {
    /// f is defined everywhere on the box (decoration at least *def*).
    Defined { x: XBox },
    /// f is defined nowhere on the box (empty enclosure).
    Undefined { x: XBox },
    /// The subject is valid on the box and strictly above `c` (`above`) or
    /// strictly below it, from its enclosure there.
    Beyond {
        x: XBox,
        of: Subject,
        c: R,
        above: bool,
    },
    /// The subject is continuous and strictly monotone on the box (its
    /// derivative excludes 0) and strictly on the same side of `c` at both
    /// ends: it never reaches `c` there.
    NoCross {
        x: XBox,
        of: Subject,
        c: R,
        above: bool,
    },
    /// The subject is continuous and strictly monotone on the box and
    /// strictly on opposite sides of `c` at its two ends: it equals `c`
    /// exactly once there.
    OneCross { x: XBox, of: Subject, c: R },
    /// The subject equals `c` exactly at the double `at`, and is
    /// continuous and strictly monotone on the box: `at` is the only
    /// place in the box where it equals `c`.
    ExactAt {
        x: XBox,
        of: Subject,
        c: R,
        at: R,
    },
    /// The subject equals `c` exactly at `at`, an end of the box, and is
    /// strictly above (`above`) or below `c` everywhere else on the box:
    /// its Taylor coefficients at `at` of orders 1 to `order` − 1 are
    /// exactly 0, and its order-`order` coefficient is valid over the box
    /// and away from 0 there (Taylor's theorem, Lagrange's remainder).
    Touch {
        x: XBox,
        of: Subject,
        c: R,
        at: R,
        order: u8,
        above: bool,
    },
    /// The subject is valid on the box with values in `[lo, hi]`.
    Value {
        x: XBox,
        of: Subject,
        lo: R,
        hi: R,
    },
    /// Table fact: the side expression (`of` is a `G`) is s(a·x + b) for s
    /// one of sin, cos, tan, cot, 1 ± sin, 1 ± cos, cos − 1, sin − 1 (in
    /// the analysis' unit), and is 0 exactly at `x0 + k·period`, k ∈ ℤ
    /// (both enclosed).
    Family {
        of: Subject,
        x0: XBox,
        period: XBox,
    },
    /// The subject is valid and strictly on one side of `c` on the whole
    /// tail.
    TailBeyond {
        side: Tail,
        from: R,
        of: Subject,
        c: R,
        above: bool,
    },
    /// On the tail the subject's derivative of order `order` (≥ 1) is
    /// valid and strictly of one sign σ, and at `from` the subject minus
    /// `c` and its derivatives below `order` are strictly of the signs σ
    /// forces (all σ on the right tail; on the left, σ·(−1)^(order − i) for
    /// the i-th): each keeps that sign on the whole tail, by monotonicity
    /// one order up. So the subject is strictly on side `above` of `c`
    /// there. (Order 1: monotone and moving away from `c`.)
    TailChain {
        side: Tail,
        from: R,
        of: Subject,
        c: R,
        above: bool,
        order: u8,
    },
    /// f is defined on the whole tail with values in `[lo, hi]`.
    TailValue {
        side: Tail,
        from: R,
        lo: R,
        hi: R,
    },
    /// |f| → ∞ at the one excluded point enclosed by `at`, approached
    /// inside `near` (from both sides, or only from the side of `near`
    /// that `at` doesn't end). Proven from the tree's structure: a divisor
    /// with a simple zero there and a numerator away from 0, tan/sec at a
    /// simple zero of their cosine, a logarithm of an argument falling to
    /// 0, combined with factors bounded (and away from 0) on `near`.
    Unbounded { near: XBox, at: XBox },
    /// Nothing is claimed inside this box: it holds an excluded point or a
    /// domain end that is not a double (the box encloses it), or the reals
    /// strictly between an excluded double and the next double. A row is
    /// complete everywhere outside its gaps; each is a few doubles wide at
    /// most.
    Gap { x: XBox },
}

/// What a certificate's claims must cover.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Region {
    /// The whole real line (tails included): a complete answer.
    Line,
    /// Only `[a, b]`: a correct but possibly incomplete answer.
    Window { a: R, b: R },
    /// Only the points the claims name (a value at x = 0, say).
    Points,
}

/// The claims a row rests on, and the region they cover.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Certificate {
    pub covers: Region,
    pub claims: Vec<Claim>,
}

impl Certificate {
    pub fn new(covers: Region) -> Certificate {
        Certificate {
            covers,
            claims: Vec::new(),
        }
    }

    pub fn push(&mut self, c: Claim) {
        self.claims.push(c);
    }

    pub fn extend(&mut self, cs: impl IntoIterator<Item = Claim>) {
        self.claims.extend(cs);
    }
}

// ------------------------------------------------------------------ values

/// A real number known only to lie in `[lo, hi]` (a point when exact).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Enc {
    pub lo: R,
    pub hi: R,
}

impl Enc {
    pub fn new(lo: f64, hi: f64) -> Enc {
        Enc { lo: R(lo), hi: R(hi) }
    }

    pub fn point(v: f64) -> Enc {
        Enc::new(v, v)
    }

    pub fn is_point(&self) -> bool {
        self.lo.0 == self.hi.0
    }

    pub fn mid(&self) -> f64 {
        if self.is_point() {
            self.lo.0
        } else {
            self.lo.0 / 2.0 + self.hi.0 / 2.0
        }
    }

    pub fn contains(&self, v: f64) -> bool {
        self.lo.0 <= v && v <= self.hi.0
    }
}

/// One end of an interval of the line.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Bound {
    NegInf,
    PosInf,
    /// At a point (enclosed), included when `closed`.
    At { x: Enc, closed: bool },
}

/// An interval of the line between two bounds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    pub lo: Bound,
    pub hi: Bound,
}

/// `x0 + k·period` for k ∈ ℤ.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Family {
    pub x0: Enc,
    pub period: Enc,
}

/// The domain: the union of `pieces`, minus the families.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DomainValue {
    pub pieces: Vec<Piece>,
    pub excluded: Vec<Family>,
}

/// An x where something happens: one point, or a periodic family.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Spot {
    At(Enc),
    Every(Family),
}

/// Minimum or maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtKind {
    Min,
    Max,
}

/// A local extremum.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Extremum {
    pub x: Enc,
    pub y: Enc,
    pub kind: ExtKind,
}

/// An inflection point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Inflection {
    pub x: Enc,
    pub y: Enc,
}

/// Increasing, decreasing or constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dir {
    Increasing,
    Decreasing,
}

/// f is strictly monotone on the piece.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Monotone {
    pub on: Piece,
    pub dir: Dir,
}

/// Even, odd, or neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Parity {
    Even,
    Odd,
    Neither,
}

/// A horizontal asymptote: f → a limit in `y` as x → ±∞ on `side`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Horizontal {
    pub side: Tail,
    /// The proven enclosure of the limit ("y ∈ [lo, hi]").
    pub y: Enc,
    /// A recognised closed form (0, a fraction, a multiple of π or e, a
    /// surd) inside `y`, when `y` is narrower than anything the panel
    /// shows. Recognised, not proven: the limit is only known to lie in `y`.
    pub looks_like: Option<R>,
}

/// The outcome of certifying one row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Row<T> {
    /// Proven complete and correct.
    Certified { value: T, cert: Certificate },
    /// Every item proven, but there may be more (`cert.covers` is a
    /// window).
    Partial { value: T, cert: Certificate },
    /// Not proven.
    Unknown { reason: String },
}

impl<T> Row<T> {
    pub fn unknown(reason: impl Into<String>) -> Row<T> {
        Row::Unknown {
            reason: reason.into(),
        }
    }

    pub fn value(&self) -> Option<&T> {
        match self {
            Row::Certified { value, .. } | Row::Partial { value, .. } => Some(value),
            Row::Unknown { .. } => None,
        }
    }

    pub fn is_certified(&self) -> bool {
        matches!(self, Row::Certified { .. })
    }

    pub fn is_partial(&self) -> bool {
        matches!(self, Row::Partial { .. })
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Row::Certified { .. } => "certified",
            Row::Partial { .. } => "partial",
            Row::Unknown { .. } => "unknown",
        }
    }
}
