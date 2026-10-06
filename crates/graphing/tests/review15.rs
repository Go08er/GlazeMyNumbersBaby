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

use graphing::compile::CompileOptions;
use graphing::equation::Equation;
use graphing::interval::{Ctx, Dec, Interval, derivs_valid, taylor};

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
