//! The exact values of numbers, as intervals.
//!
//! The expression tree holds each number as a double and what it stands
//! for exactly ([`Lit`], per occurrence). For a certificate,
//! `1000000000000.0625` must be exactly that number (it is a double) and
//! `0.000001` must be the decimal, which lies strictly between two
//! doubles: [`Lit::enclose`] gives the tightest enclosure of the number.
//!
//! Until review 13 the exact values were looked up by the double each
//! literal parsed to, which two different decimals can share
//! (`1.0000000000000001` and `1`, R13-M-01): every number now reads as its
//! own `Lit` says.

use super::arith::Interval;
use crate::ast::Lit;
use std::cmp::Ordering;

impl Lit {
    /// The tightest enclosure of the number `Num(v, self)` stands for: v
    /// itself when it is exactly v, the doubles either side of v that hold
    /// a decimal typed or a value folded, and v's neighbours for a number
    /// not known exactly.
    pub fn enclose(&self, v: f64) -> Interval {
        match self.side(v) {
            Some(Ordering::Equal) => Interval::point(v),
            Some(Ordering::Less) => Interval::new(v.next_down(), v),
            Some(Ordering::Greater) => Interval::new(v, v.next_up()),
            None => Interval::around(v),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;

    /// Each number of `s` (a `y=` equation), in the parser's tree order.
    fn nums(s: &str) -> Vec<(f64, Lit)> {
        let eq = crate::Equation::parse(s).unwrap();
        let mut out = Vec::new();
        eq.explicit().unwrap().1.visit(&mut |e| {
            if let Expr::Num(v, lit) = e {
                out.push((*v, lit.clone()));
            }
        });
        out
    }

    #[test]
    fn representable_literals_are_points() {
        for (v, lit) in nums("y=(x-1000000000000)/(x-1000000000000.0625)+0.5") {
            assert!(lit.enclose(v).is_point(), "{v}");
        }
    }

    #[test]
    fn other_decimals_are_enclosed_strictly() {
        let n = nums("y=0.000001*x+0.1");
        let a = n[0].1.enclose(n[0].0);
        // 0.000001 the double is below the decimal.
        assert_eq!((a.lo(), a.hi()), (0.000001, 0.000001f64.next_up()));
        let b = n[1].1.enclose(n[1].0);
        // 0.1 the double is above the decimal.
        assert_eq!((b.lo(), b.hi()), (0.1f64.next_down(), 0.1));
        // A sign folded onto a literal.
        let c = n[1].1.neg().enclose(-0.1);
        assert_eq!((c.lo(), c.hi()), (-0.1, (-0.1f64).next_up()));
        // Written by the program, not typed: exact only as a small integer.
        assert!(!Lit::Near.enclose(0.3).is_point());
        assert!(Lit::Exact.enclose(4.0).is_point());
    }

    #[test]
    fn literals_in_superscripts_and_beyond_the_doubles() {
        let n = nums("y=x^(0.1)+x²");
        assert!(!n[0].1.enclose(n[0].0).is_point());
        assert!(n[1].1.enclose(n[1].0).is_point());
        let huge = "9".repeat(400);
        let n = nums(&format!("y={huge}"));
        let h = n[0].1.enclose(n[0].0);
        assert!(h.is_empty() || h.hi() == f64::INFINITY);
    }

    /// Review 13, R13-M-01: two decimals that share a double are two
    /// numbers, each read as typed, wherever it occurs.
    #[test]
    fn literals_sharing_a_double_keep_their_own_values() {
        let n = nums("y=1.0000000000000001*x-1*x");
        assert_eq!(n.len(), 2);
        assert_eq!(n[0].0, n[1].0);
        assert_eq!(n[0].1.digits(), Some("1.0000000000000001"));
        assert_eq!(n[1].1, Lit::Exact);
        let (a, b) = (n[0].1.enclose(1.0), n[1].1.enclose(1.0));
        assert_eq!((a.lo(), a.hi()), (1.0, 1.0f64.next_up()));
        assert!(b.is_point());
    }
}
