//! Round-16 review: Taylor series built from a value alone.
//!
//! R16-L-01: ⌊x⌋⁰ at 1 reported the value 1 with valid derivatives 0, but
//! just left of 1 it is 0⁰, undefined: 1 is an end of the domain, with no
//! ordinary derivatives. min(x, ⌊x⌋⁰ + 100) took the loser's valid
//! derivatives for a loser continuous beside the box and reported f′ = 1
//! there. nCr(⌊x⌋, 0) at 0 likewise, Γ's pole just left of it. The zero
//! power and the zero count built a constant series from the value on the
//! box alone, dropping what the operand does beside it.
//!
//! The audit found the same in the step functions (⌊√x + 0.5⌋ at 0, where
//! √x ends; ⌊⌊x⌋ + 0.5⌋ at 1, where ⌊x⌋ + 0.5 jumps, though not past a
//! jump of the outer floor) and in nPr(n, 0) and a computed zero exponent
//! (⌊x⌋^(1 − 1)). Each now keeps its derivatives only where its operand is
//! differentiable on the box (so continuous beside it), or where it is
//! defined and of one value on the box an ulp wider each side. It also
//! found nCr(n, k) over a box of n below −10³⁰⁰ defined and continuous,
//! though every double there is a negative integer, a pole of Γ.

use graphing::Graph;
use graphing::compile::CompileOptions;
use graphing::equation::Equation;
use graphing::interval::{Ctx, Dec, DecInterval, Interval, derivs_valid, elem, taylor};

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

/// The value the app computes for `y=src` at x.
fn app(src: &str, x: f64) -> f64 {
    let mut g = Graph::new();
    let id = g.add_equation(&format!("y={src}"));
    g.evaluate(id, x).unwrap_or(f64::NAN)
}

/// Each function is defined at the point, with the value given, and
/// undefined just left of it: no derivatives there.
const AT_A_DOMAIN_END: &[(&str, f64, f64)] = &[
    // The review's cases and their min/max mirrors.
    ("floor(x)^0", 1.0, 1.0),
    ("min(x,floor(x)^0+100)", 1.0, 1.0),
    ("max(x,-floor(x)^0-100)", 1.0, 1.0),
    ("min(floor(x)^0+100,x)", 1.0, 1.0),
    ("nCr(floor(x),0)", 0.0, 1.0),
    ("min(x,nCr(floor(x),0)+100)", 0.0, 0.0),
    ("max(x,-nCr(floor(x),0)-100)", 0.0, 0.0),
    ("min(nCr(floor(x),0)+100,x)", 0.0, 0.0),
    // The audit's: nPr, a computed zero exponent, a smooth operand's end.
    ("nPr(floor(x),0)", 0.0, 1.0),
    ("min(x,nPr(floor(x),0)+100)", 0.0, 0.0),
    ("floor(x)^(1-1)", 1.0, 1.0),
    ("(sqrt(x)+1)^0", 0.0, 1.0),
    ("min(x,(sqrt(x)+1)^0+100)", 0.0, 0.0),
    ("mod(floor(x)^0,3)", 1.0, 1.0),
    // Steps of an argument that ends at the point.
    ("floor(sqrt(x)+0.5)", 0.0, 0.0),
    ("ceil(sqrt(x)-0.5)", 0.0, 0.0),
    ("round(sqrt(x))", 0.0, 0.0),
    ("sign(sqrt(x)+1)", 0.0, 1.0),
    ("min(x,floor(sqrt(x)+0.5)+5)", 0.0, 0.0),
    ("max(x,-floor(sqrt(x)+0.5)-5)", 0.0, 0.0),
];

#[test]
fn a_value_alone_gives_no_derivatives_at_a_domain_end() {
    for &(src, x, v) in AT_A_DOMAIN_END {
        assert_eq!(app(src, x), v, "{src} at {x}");
        assert!(app(src, x.next_down()).is_nan(), "{src} left of {x}");
        let s = series(src, x, x);
        assert!(
            s[0].lo() == v && s[0].hi() == v && s[0].dec >= Dec::Dac,
            "{src} at {x}: {:?}",
            s[0]
        );
        assert!(!derivs_valid(&s, 1), "{src} at {x}: {s:?}");
    }
}

/// A step of an argument that jumps where the step doesn't: ⌊⌊x⌋ + 0.5⌋ is
/// ⌊x⌋, which jumps at 1 though ⌊x⌋ + 0.5 = 1.5 there is clear of the outer
/// floor's jumps. On [1, 1.1] f is continuous (restricted to the box) but
/// has no derivative at 1; min(x, 2⌊⌊x⌋ + 0.5⌋ − 0.5) at 1 drops to −0.5
/// just left of 1 (review 15's min(x, 2⌊x⌋ − 0.5), disguised).
#[test]
fn a_step_of_a_jump_has_no_derivative_there() {
    for (src, lo, hi) in [
        ("floor(floor(x)+0.5)", 1.0, 1.0),
        ("floor(floor(x)+0.5)", 1.0, 1.1),
        ("round(floor(x)+0.25)", 1.0, 1.0),
        ("min(x,2*floor(floor(x)+0.5)-0.5)", 1.0, 1.0),
        ("max(-x,-2*floor(floor(x)+0.5)+0.5)", 1.0, 1.0),
    ] {
        let s = series(src, lo, hi);
        assert!(s[0].dec >= Dec::Dac, "{src} on [{lo}, {hi}]: {:?}", s[0]);
        assert!(!derivs_valid(&s, 1), "{src} on [{lo}, {hi}]: {s:?}");
    }
    assert_eq!(app("min(x,2*floor(floor(x)+0.5)-0.5)", 0.999), -0.5);
}

/// Where the operand stays clear beside the box the derivatives stand, as
/// before: from its own (differentiable) series, or from f being one value
/// on the box an ulp wider each side (a kink in the operand, not a jump).
#[test]
fn constant_beside_the_box_keeps_its_derivatives() {
    for (src, x, d1) in [
        ("floor(x)^0", 1.5, 0.0),
        ("x^0", 1.0, 0.0),
        ("(x-1)^0", 0.0, 0.0),
        ("(abs(x)+1)^0", 0.0, 0.0),
        ("floor(x)^(1-1)", 1.5, 0.0),
        ("nCr(floor(x),0)", 0.5, 0.0),
        ("nCr(x,0)", -0.5, 0.0),
        ("nPr(abs(x),0)", 0.0, 0.0),
        ("nCr(floor(x),0)", 2.0, 0.0),
        ("floor(abs(x)+0.5)", 0.0, 0.0),
        ("floor(sqrt(x)+0.5)", 1.0, 0.0),
        ("sign(abs(x)+1)", 0.0, 0.0),
        ("floor(floor(x)+0.5)", 1.5, 0.0),
        ("min(x,floor(x)^0+100)", 1.5, 1.0),
        ("min(x,nCr(floor(x),0)+100)", 0.5, 1.0),
        ("min(x,(abs(x)+1)^0+100)", 0.0, 1.0),
        ("max(x,-floor(abs(x)+0.5)-5)", 0.0, 1.0),
        ("min(x,floor(x)+5)", 0.5, 1.0),
    ] {
        let s = series(src, x, x);
        assert!(derivs_valid(&s, 3), "{src} at {x}: {s:?}");
        assert!(s[1].lo() == d1 && s[1].hi() == d1, "{src} at {x}: {s:?}");
    }
    // A non-point box clear of the jumps and of 0: the same.
    let s = series("floor(x)^0", 1.25, 1.75);
    assert!(derivs_valid(&s, 3), "{s:?}");
    let s = series("min(x,nCr(floor(x),0)+100)", 0.25, 0.75);
    assert!(derivs_valid(&s, 3) && s[1].lo() == 1.0, "{s:?}");
}

/// nCr(n, k) and nPr(n, k) are undefined at the negative integers, and
/// every double past 2⁵³ is an integer: a box of n far below −10³⁰⁰ holds
/// only poles (it was taken to hold none, the least integer looked for
/// from −10³⁰⁰ on).
#[test]
fn counts_far_below_zero_are_at_poles() {
    for n in [-1e301, -1e308, -9007199254740992.0, -1.0] {
        for k in [0.0, 1.0, 2.0] {
            for perm in [false, true] {
                let r = elem::ncr_npr(&DecInterval::point(n), &DecInterval::point(k), perm);
                assert!(r.is_empty() && r.dec <= Dec::Trv, "{n} {k} {perm}: {r:?}");
            }
        }
    }
    for (lo, hi) in [(-1e302, -1e301), (-f64::MAX, -1e300), (-1e20, -1e19)] {
        let n = DecInterval::new(Interval::new(lo, hi));
        for k in [0.0, 1.0, 2.0] {
            let r = elem::ncr_npr(&n, &DecInterval::point(k), false);
            assert!(r.dec <= Dec::Trv, "[{lo}, {hi}] {k}: {r:?}");
        }
    }
    for src in ["nCr(x,0)", "nCr(x,2)", "nPr(x,1)"] {
        let s = series(src, -1e302, -1e301);
        assert!(s[0].dec <= Dec::Trv, "{src}: {:?}", s[0]);
        assert!(!derivs_valid(&s, 1), "{src}: {s:?}");
        assert!(app(src, -1e301).is_nan(), "{src}");
    }
    // nCr(−10³⁰¹, 0) is undefined, as the app has it.
    assert!(app("nCr(-10^301,0)", 0.0).is_nan());
    let s = series("nCr(-10^301,0)", 0.0, 0.0);
    assert!(s[0].dec <= Dec::Trv, "{:?}", s[0]);
    // Between the poles, and right of them, as before.
    for (lo, hi, k) in [(-1.75, -1.25, 2.0), (-0.5, 3.0, 0.0), (2.0, 5.0, 2.0)] {
        let n = DecInterval::new(Interval::new(lo, hi));
        let r = elem::ncr_npr(&n, &DecInterval::point(k), false);
        assert!(r.dec >= Dec::Dac, "[{lo}, {hi}] {k}: {r:?}");
    }
}
