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

/// A strict sign.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sign {
    Pos,
    Neg,
}

/// Which tail: `(−∞, from]` or `[from, ∞)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tail {
    Left,
    Right,
}

/// One fact about a box (or a tail) of x.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Claim {
    /// f is defined everywhere on the box (decoration at least *def*).
    Defined { x: XBox },
    /// f is defined and continuous on the box (at least *dac*).
    Continuous { x: XBox },
    /// f is defined nowhere on the box (empty enclosure).
    Undefined { x: XBox },
    /// f⁽ᵏ⁾ exists (f is defined and continuous, the derivatives up to k
    /// are valid) and is strictly of one sign on the box. For k = 0 only
    /// "defined" is needed.
    Sign { x: XBox, of: Order, sign: Sign },
    /// f⁽ᵏ⁾ has exactly one zero in the box: valid there, f⁽ᵏ⁺¹⁾ ≠ 0 on
    /// the box, and strict opposite signs at its two ends.
    OneCrossing { x: XBox, of: Order },
    /// f⁽ᵏ⁾ has no zero in the box: valid there, f⁽ᵏ⁺¹⁾ ≠ 0 on the box,
    /// and the same strict sign `sign` at its two ends.
    NoCrossing { x: XBox, of: Order, sign: Sign },
    /// f⁽ᵏ⁾(at) = 0 exactly at the double `at` in the box, and f⁽ᵏ⁺¹⁾ ≠ 0
    /// on the box: `at` is its only zero there.
    ExactZero { x: XBox, at: R, of: Order },
    /// f⁽ᵏ⁾ is defined on the box with values in `[lo, hi]`.
    Value {
        x: XBox,
        of: Order,
        lo: R,
        hi: R,
    },
    /// The sub-expression at `path`, minus `c`, has exactly one zero in
    /// the box (continuous there, its derivative ≠ 0, strict opposite
    /// signs at the ends), or is exactly 0 at the double `at`, with
    /// strict opposite signs (or a nonzero derivative) around it.
    SideZero {
        x: XBox,
        path: Vec<u8>,
        c: R,
        at: Option<R>,
    },
    /// The sub-expression at `path` is defined and continuous on the box
    /// with values in `[lo, hi]` (an empty `lo > hi` meaning: defined
    /// nowhere on the box).
    SideValue {
        x: XBox,
        path: Vec<u8>,
        lo: R,
        hi: R,
    },
    /// Table fact: the sub-expression at `path` is `s(a·x + b)` for s one
    /// of sin, cos, 1 ± sin, 1 ± cos (in the analysis' unit), whose zeros
    /// in x are exactly `x0 + k·period`, k ∈ ℤ; `x0` and `period` are
    /// enclosed. The replay checks the form, the enclosures, and that the
    /// sub-expression changes sign or vanishes at the members it samples.
    Family {
        path: Vec<u8>,
        x0: XBox,
        period: XBox,
    },
    /// f⁽ᵏ⁾ is valid and strictly of one sign on the whole tail.
    TailSign {
        side: Tail,
        from: R,
        of: Order,
        sign: Sign,
    },
    /// f is defined on the whole tail with values in `[lo, hi]`.
    TailValue {
        side: Tail,
        from: R,
        lo: R,
        hi: R,
    },
    /// Nothing is claimed inside this box: it holds an excluded point that
    /// is not a double, and is one ulp wide either side of it.
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

/// A horizontal asymptote: f → `y` as x → ±∞ on `side`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Horizontal {
    pub side: Tail,
    pub y: Enc,
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
