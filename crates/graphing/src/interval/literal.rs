//! The exact values of numeric literals.
//!
//! The expression tree keeps each literal as the double nearest to it
//! (`Expr::Num`). For a certificate, `1000000000000.0625` must be exactly
//! that number (it is a double) and `0.000001` must be the decimal, which
//! lies strictly between two doubles. [`Literals`] maps each literal's
//! double back to the tightest enclosure of the decimal it was typed as.

use super::arith::Interval;
use crate::error::EquationError;
use crate::lexer::{ParseOptions, literal_texts};
use std::cmp::Ordering;
use std::collections::HashMap;

/// Where a typed decimal lies relative to the double it was parsed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Exact,
    Below,
    Above,
    /// Several literals parsed to this double from both sides.
    Both,
}

/// The literals of one input text, by the double each parsed to.
#[derive(Clone, Debug, Default)]
pub struct Literals {
    by_bits: HashMap<u64, Side>,
    /// The decimal typed, when every literal that parsed to this double
    /// was typed the same (a trailing `0` aside).
    digits: HashMap<u64, Option<String>>,
}

impl Literals {
    /// Scans the literals of `text` (as [`crate::Equation::parse_with`]
    /// would read it).
    pub fn of(text: &str, opts: ParseOptions) -> Result<Literals, EquationError> {
        let mut by_bits: HashMap<u64, Side> = HashMap::new();
        let mut texts: HashMap<u64, Option<String>> = HashMap::new();
        for (v, digits) in literal_texts(text, opts)? {
            let side = side_of(&digits, v);
            by_bits
                .entry(v.to_bits())
                .and_modify(|s| {
                    if *s != side {
                        *s = Side::Both;
                    }
                })
                .or_insert(side);
            let (i, f) = split_decimal(&digits);
            let canonical = format!("{i}.{f}");
            texts
                .entry(v.to_bits())
                .and_modify(|t| {
                    if t.as_deref() != Some(canonical.as_str()) {
                        *t = None;
                    }
                })
                .or_insert(Some(canonical));
        }
        Ok(Literals {
            by_bits,
            digits: texts,
        })
    }

    /// The exact value `Num(v)` stands for, when it is known: the decimal
    /// typed (or its negation, a folded sign), or an integer below 2⁵³
    /// that no literal typed otherwise parsed to (one the parser or an
    /// analysis wrote). `None` for anything else, and where literals typed
    /// differently parsed to `v`.
    pub(crate) fn exact(&self, v: f64) -> Option<crate::big::Rat> {
        let typed = |bits: u64| -> Option<Option<crate::big::Rat>> {
            let t = self.digits.get(&bits)?;
            Some(t.as_deref().and_then(crate::big::Rat::from_decimal))
        };
        if let Some(r) = typed(v.to_bits()) {
            return r;
        }
        if let Some(r) = typed((-v).to_bits()) {
            return r.map(crate::big::Rat::neg);
        }
        if v == v.trunc() && v.abs() <= 9007199254740992.0 {
            return crate::big::Rat::from_f64(v);
        }
        None
    }

    /// Reads `Num(v)` (v ≥ 0) as the exact value `r` too: a subtree of
    /// literals folded exactly and rounded once to v
    /// (`compile::fold_literals`). If something else already reads as v
    /// from another side, `v` is enclosed by its neighbours; its exact
    /// value is not known any more either way.
    pub(crate) fn fold_in(&mut self, v: f64, r: &crate::big::Rat) {
        use crate::big::Round;
        let side = if r.to_f64(Round::Down) == r.to_f64(Round::Up) {
            Side::Exact
        } else if r.to_f64(Round::Up) == v {
            Side::Below
        } else {
            Side::Above
        };
        self.by_bits
            .entry(v.to_bits())
            .and_modify(|s| {
                if *s != side {
                    *s = Side::Both;
                }
            })
            .or_insert(side);
        self.digits.insert(v.to_bits(), None);
    }

    /// Whether a literal typed as some other decimal parsed to the whole
    /// number `v` (`2.0000000000000001` to 2): an integer written into a
    /// tree (`a·a` as a²) would then read as that decimal.
    pub(crate) fn shadows(&self, v: f64) -> bool {
        [v, -v].iter().any(|w| {
            self.by_bits
                .get(&w.to_bits())
                .is_some_and(|s| *s != Side::Exact)
        })
    }

    /// No literals known: every number is taken at face value only when it
    /// is a small integer (see [`Literals::enclose`]).
    pub fn none() -> Literals {
        Literals::default()
    }

    /// An enclosure of the number the tree's `Num(v)` stands for.
    ///
    /// A typed literal gives its exact decimal's enclosure; `−v` of a typed
    /// literal (a folded sign) likewise. A number that isn't a typed literal
    /// (an integer the parser itself wrote, as `root(x, 4)`'s 4) is exact
    /// when it's an integer below 2⁵³; anything else could be any decimal
    /// that rounds to it.
    pub fn enclose(&self, v: f64) -> Interval {
        let side = |s: Side, v: f64| -> Interval {
            match s {
                Side::Exact => Interval::point(v),
                Side::Below => Interval::new(v.next_down(), v),
                Side::Above => Interval::new(v, v.next_up()),
                Side::Both => Interval::around(v),
            }
        };
        if let Some(s) = self.by_bits.get(&v.to_bits()) {
            return side(*s, v);
        }
        if let Some(s) = self.by_bits.get(&(-v).to_bits()) {
            return -side(*s, -v);
        }
        if v == v.trunc() && v.abs() <= 9007199254740992.0 {
            return Interval::point(v);
        }
        Interval::around(v)
    }

    /// True if the literal that parsed to `v` is exactly `v`.
    pub fn is_exact(&self, v: f64) -> bool {
        self.enclose(v).is_point()
    }
}

/// Compares the decimal `digits` (non-negative, digits and at most one
/// `.`) with the double `v` ≥ 0, exactly.
fn side_of(digits: &str, v: f64) -> Side {
    if !v.is_finite() {
        // Beyond the doubles: the decimal is below +∞ (and above MAX).
        return Side::Below;
    }
    let (ti, tf) = split_decimal(digits);
    // A double's exact decimal expansion has at most 1074 fractional digits.
    let exact = format!("{:.1074}", v.abs());
    let (vi, vf) = split_decimal(&exact);
    match cmp_decimal((&ti, &tf), (&vi, &vf)) {
        Ordering::Equal => Side::Exact,
        Ordering::Less => Side::Below,
        Ordering::Greater => Side::Above,
    }
}

/// Integer digits without leading zeros, fraction digits without trailing
/// zeros.
fn split_decimal(s: &str) -> (String, String) {
    let (i, f) = s.split_once('.').unwrap_or((s, ""));
    let i = i.trim_start_matches('0').to_string();
    let f = f.trim_end_matches('0').to_string();
    (i, f)
}

fn cmp_decimal(a: (&str, &str), b: (&str, &str)) -> Ordering {
    a.0.len()
        .cmp(&b.0.len())
        .then_with(|| a.0.cmp(b.0))
        .then_with(|| {
            // Fractions compare digit by digit, the shorter padded with 0s.
            let n = a.1.len().max(b.1.len());
            let pad = |s: &str| format!("{s:0<n$}");
            pad(a.1).cmp(&pad(b.1))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lits(s: &str) -> Literals {
        Literals::of(s, ParseOptions::default()).unwrap()
    }

    #[test]
    fn representable_literals_are_points() {
        let l = lits("y=(x-1000000000000)/(x-1000000000000.0625)+0.5");
        assert!(l.enclose(1000000000000.0625).is_point());
        assert!(l.enclose(1e12).is_point());
        assert!(l.enclose(0.5).is_point());
    }

    #[test]
    fn other_decimals_are_enclosed_strictly() {
        let l = lits("y=0.000001*x+0.1");
        let a = l.enclose(0.000001);
        // 0.000001 the double is below the decimal.
        assert_eq!((a.lo(), a.hi()), (0.000001, 0.000001f64.next_up()));
        let b = l.enclose(0.1);
        // 0.1 the double is above the decimal.
        assert_eq!((b.lo(), b.hi()), (0.1f64.next_down(), 0.1));
        // A sign folded onto a literal.
        let c = l.enclose(-0.1);
        assert_eq!((c.lo(), c.hi()), (-0.1, (-0.1f64).next_up()));
        // Not a typed literal and not a small integer.
        assert!(!l.enclose(0.3).is_point());
        assert!(l.enclose(4.0).is_point());
    }

    #[test]
    fn literals_in_superscripts_and_beyond_the_doubles() {
        let l = lits("y=x^(0.1)+x²");
        assert!(!l.enclose(0.1).is_point());
        let huge = "9".repeat(400);
        let l = lits(&format!("y={huge}"));
        let h = l.enclose(f64::INFINITY);
        assert!(h.is_empty() || h.hi() == f64::INFINITY);
    }
}
