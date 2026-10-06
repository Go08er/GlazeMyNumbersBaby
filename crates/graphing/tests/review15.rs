//! Round-15 review: written fractions of any size, limits of undefined
//! constants, locally constant root degrees, and exact counts.
//!
//! R15-L-02: root(−1, 3 + x⁴) at 0 had the value −1 and derivatives 0
//! through order 3, read off a degree whose first coefficients were 0;
//! beside 0 the degree is no integer and the root of −1 undefined, so 0 is
//! an isolated point of the domain, with no derivatives. The audit of the
//! other Taylor fast paths found the same inference in nCr and nPr (r's
//! zero coefficients taken for a constant r), and its kin in min and max
//! (the winner on a box taken for the winner beside it, though the loser
//! jumps there, or is defined there alone).
//!
//! Question 1: counts of whole numbers that are doubles were refused once
//! past the doubles (1026 bits), those of numbers no double holds only past
//! the 2¹⁴-bit cap: nCr(2¹⁰⁰⁰, 2) over its own value was unknown. Now one
//! cap for every count, n! and n!! too.

use graphing::Graph;
use graphing::TrigUnit;
use graphing::analysis::truth::{R, reval_typed};
use graphing::compile::CompileOptions;
use graphing::equation::Equation;
use graphing::interval::{Ctx, Dec, Interval, derivs_valid, taylor};
use graphing::lexer::ParseOptions;

/// The setting's digit limits: 5 to 20 digits, and off.
const PRECISIONS: [Option<u8>; 6] = [Some(5), Some(14), Some(15), Some(16), Some(20), None];

/// The value the app computes for `y=src` at x under `digits`.
fn value(src: &str, x: f64, digits: Option<u8>) -> f64 {
    let mut g = Graph::new();
    g.set_literal_digits(digits);
    let id = g.add_equation(&format!("y={src}"));
    g.evaluate(id, x).unwrap_or(f64::NAN)
}

/// The typed reference for `y=src` at x under `digits`.
fn reference(src: &str, x: f64, digits: Option<u8>) -> R {
    let opts = ParseOptions {
        literal_digits: digits,
        ..Default::default()
    };
    let eq = Equation::parse_with(&format!("y={src}"), opts).unwrap();
    reval_typed(eq.explicit().unwrap().1, x, TrigUnit::Radians)
}

/// f's Taylor coefficients through order 3 on the box [lo, hi].
fn series(src: &str, lo: f64, hi: f64) -> graphing::interval::Series {
    let eq = Equation::parse(&format!("y={src}")).unwrap();
    let (_, f) = eq.explicit().unwrap();
    taylor(
        f,
        Interval::new(lo, hi),
        3,
        &Ctx::new(CompileOptions::default()),
    )
}

#[test]
fn a_degree_constant_only_to_some_order_gives_no_derivatives() {
    // The value at 0 is the root of −1 of degree 3; its derivatives are
    // not those of a constant, for there is no neighbourhood to have any.
    let s = series("root(-1,3+x^4)", 0.0, 0.0);
    assert!(s[0].lo() == -1.0 && s[0].hi() == -1.0, "{:?}", s[0]);
    assert!(!derivs_valid(&s, 1), "{s:?}");
    // Beside 0: possibly undefined.
    let s = series("root(-1,3+x^4)", -0.1, 0.1);
    assert!(s[0].dec <= Dec::Trv, "{:?}", s[0]);
    // A degree free of x is constant: the cube root's derivatives, as
    // before; and a positive base's root of a varying degree is smooth.
    let s = series("root(x-2,3)", 1.0, 1.0);
    assert!(derivs_valid(&s, 3), "{s:?}");
    assert!((s[1].lo() - 1.0 / 3.0).abs() < 1e-12, "{s:?}");
    let s = series("root(2,3+x^4)", 0.0, 0.0);
    assert!(derivs_valid(&s, 3), "{s:?}");
    // The counts likewise: r = 2 + x⁴ is no constant (n(n − 1)/2's
    // derivatives are not shown for it), r = 2 is.
    let s = series("nCr(x,2+x^4)", 0.5, 0.5);
    assert!(!derivs_valid(&s, 1), "{s:?}");
    let s = series("nCr(x,2)", 0.5, 0.5);
    assert!(derivs_valid(&s, 2), "{s:?}");
}

/// The same inference in min and max: the winner on the box is f there,
/// but its derivatives are f's only if it stays the winner beside the box,
/// which a loser jumping at the box, or defined only to one side or at the
/// box alone, doesn't promise.
#[test]
fn a_winner_beside_the_box_needs_a_continuous_loser() {
    for src in [
        // Just left of 1, 2⌊x⌋ − 0.5 is −0.5: f jumps.
        "min(x,2*floor(x)-0.5)",
        // Defined for x ≥ 1, x ≤ 1 only: an end of the domain.
        "min(x,sqrt(x-1)+5)",
        "min(x,sqrt(1-x)+5)",
    ] {
        let s = series(src, 1.0, 1.0);
        assert!(s[0].lo() == 1.0 && s[0].hi() == 1.0, "{src}: {:?}", s[0]);
        assert!(!derivs_valid(&s, 1), "{src}: {s:?}");
    }
    let s = series("min(x,root(-1,3+x^4)+5)", 0.0, 0.0);
    assert!(!derivs_valid(&s, 1), "{s:?}");
    // A loser continuous beside the box, kink and all: the winner's
    // series, as before.
    for (src, x) in [
        ("min(x,2*floor(x)+0.5)", 0.999),
        ("min(x,floor(x)+5)", 0.5),
        ("max(x^2,-1-x^2)", 0.5),
        ("min(x,x+1+abs(x-3))", 3.0),
        ("max(x,x-1-abs(x-3),x-2)", 3.0),
    ] {
        let s = series(src, x, x);
        assert!(derivs_valid(&s, 3), "{src}: {s:?}");
    }
}

/// Question 1: counts of whole numbers are exact within the one cap every
/// exact value keeps (2¹⁴ bits), whether or not their numbers are doubles.
/// C(2¹⁰⁰⁰, 2) of two doubles was refused once past the doubles, so its
/// quotient by its own value was unknown; so were 171!/170! and the like.
#[test]
fn counts_of_doubles_are_exact_within_the_cap() {
    for digits in PRECISIONS {
        for (src, want) in [
            ("nCr(2^1000,2)/(2^1000*(2^1000-1)/2)", 1.0),
            ("nPr(2^1000,2)/(2^1000*(2^1000-1))", 1.0),
            ("nCr(2^1000,2^1000-2)/nCr(2^1000,2)", 1.0),
            ("171!/170!", 171.0),
            ("1700!/1699!", 1700.0),
            ("301!!/299!!", 301.0),
            ("nCr(2^1000,2)-nCr(2^1000,2)+x", 0.0),
        ] {
            assert_eq!(value(src, 0.0, digits), want, "{src} ({digits:?})");
            let r = reference(src, 0.0, digits);
            assert!(
                matches!(r, R::V(v) if v.f() == want),
                "{src} ({digits:?}): {r:?}"
            );
        }
        // + x: the line y = x + 1.
        let src = "nCr(2^1000,2)/(2^1000*(2^1000-1)/2)+x";
        assert_eq!(value(src, 2.5, digits), 3.5, "{digits:?}");
        // Alone, beyond the doubles: +∞, as before.
        assert_eq!(value("nCr(2^1000,20)", 0.0, digits), f64::INFINITY);
        assert_eq!(value("1700!", 0.0, digits), f64::INFINITY);
        // Past the cap: unknown, never what rounding makes of it.
        for src in [
            "2000!/1999!",
            "nCr(2^1000,20)/nCr(2^1000,20)",
            "4000!!/3998!!",
        ] {
            assert!(value(src, 0.0, digits).is_nan(), "{src} ({digits:?})");
            assert!(
                matches!(reference(src, 0.0, digits), R::Unknown),
                "{src} ({digits:?})"
            );
        }
    }
}
