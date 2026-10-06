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
//! R15-M-01: an exponent written as a ratio of integers was read as one
//! only while each part was below 10⁶, before reduction: x^(1000001/3),
//! x^(1/1000001) and even x^(1000001/3000003), the cube root, took the
//! positive-base rule (undefined at −1; domain [0, ∞), neither, a minimum
//! at 0), in the evaluator, the interval core, the certifier and the
//! replay alike. Each part is now an integer of any size, reduced first.
//!
//! The deferred follow-up: whole-number exponents of 10⁶ or more
//! (`x^1000001`) take the same reading, q = 1, in the side conditions,
//! the Taylor core, the simplifier and pole detection, as the scalar and
//! the replay already did.
//!
//! R15-L-01: with c = ⌊√(sin²4 + cos²4 − 1 − 10⁻³⁰)⌋, nowhere defined
//! (its radicand is −10⁻³⁰), `limit_at` gave c + 1 the limit 1 both ways
//! and e^(ln x + c)/x the limit 1 at +∞: a constant's undecorated
//! enclosure, the point 0, was taken for a known 0, and the tail check
//! asked only for a nonempty enclosure.
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

/// The panel of `y=src` under `digits`, and the app's value at each x.
fn panel(
    src: &str,
    xs: &[f64],
    digits: Option<u8>,
) -> (graphing::analysis::KeyGraphFeatures, Vec<f64>) {
    let mut g = Graph::new();
    g.set_literal_digits(digits);
    let id = g.add_equation(&format!("y={src}"));
    let vs = xs
        .iter()
        .map(|&x| g.evaluate(id, x).unwrap_or(f64::NAN))
        .collect();
    (g.analyze(id), vs)
}

/// R15-M-01: an exponent written as a ratio of integers is a real root's
/// power at any size, reduced first, by the parities of p and q as the
/// digit limit leaves them. To 5 digits 1000001 is the even 1000000:
/// x^(1000000/3) is even, defined on ℝ; x^(1/1000000) an even root, x ≥ 0.
#[test]
fn written_ratios_of_any_size_are_real_roots() {
    use graphing::analysis::Parity;
    // (f(−1), f(−8), domain, range, parity, minima)
    type Want = (
        f64,
        f64,
        &'static str,
        &'static str,
        Parity,
        &'static [&'static str],
    );
    let odd: Want = (-1.0, f64::NAN, "x ∈ ℝ", "y ∈ ℝ", Parity::Odd, &[]);
    let even: Want = (
        1.0,
        f64::NAN,
        "x ∈ ℝ",
        "y ∈ [0, ∞)",
        Parity::Even,
        &["(0, 0)"],
    );
    let root: Want = (
        f64::NAN,
        f64::NAN,
        "x ∈ [0, ∞)",
        "y ∈ [0, ∞)",
        Parity::Neither,
        &["(0, 0)"],
    );
    let cube: Want = (-1.0, -2.0, "x ∈ ℝ", "y ∈ ℝ", Parity::Odd, &[]);
    for digits in PRECISIONS {
        let five = digits == Some(5);
        for (src, want) in [
            ("x^(1000001/3)", if five { even } else { odd }),
            ("x^(1/1000001)", if five { root } else { odd }),
            ("x^(1000001/3000003)", cube),
            ("x^(2000002/3)", even),
        ] {
            let (k, v) = panel(src, &[-1.0, -8.0, 1.0], digits);
            let same = |a: f64, b: f64| (a.is_nan() && b.is_nan()) || a == b;
            assert!(same(v[0], want.0), "{src} ({digits:?}) at −1: {}", v[0]);
            if !want.1.is_nan() {
                assert!(
                    (v[1] - want.1).abs() < 1e-12,
                    "{src} ({digits:?}) at −8: {}",
                    v[1]
                );
            }
            assert_eq!(v[2], 1.0, "{src} ({digits:?}) at 1");
            let r = reference(src, -1.0, digits);
            let ok = match r {
                R::V(x) => x.f() == want.0,
                R::Undef => want.0.is_nan(),
                R::Unknown => false,
            };
            assert!(ok, "{src} ({digits:?}): reference {r:?}");
            assert_eq!(
                (k.domain.as_str(), k.range.as_str(), k.parity),
                (want.2, want.3, want.4),
                "{src} ({digits:?})"
            );
            assert_eq!(k.minima, want.5, "{src} ({digits:?})");
            assert_eq!(k.y_intercept, "0", "{src} ({digits:?})");
        }
        // Odd over odd: increasing through 0, its inflection there.
        if !five {
            let (k, _) = panel("x^(1000001/3)", &[], digits);
            assert_eq!(k.inflection_points, ["(0, 0)"], "{digits:?}");
            assert_eq!(k.maxima, Vec::<String>::new(), "{digits:?}");
            assert_eq!(k.monotonicity.len(), 1, "{digits:?}");
        }
        // A negative odd ratio: undefined at 0 alone.
        let (k, v) = panel("x^(-1000001/3)", &[-1.0], digits);
        let want = if five { 1.0 } else { -1.0 };
        assert_eq!(v[0], want, "x^(-1000001/3) ({digits:?})");
        assert_eq!(k.domain, "x ∈ ℝ \\ {0}", "{digits:?}");
    }
    // Off, a part past 2⁵³: 1/9007199254740993 is an odd root (to 14
    // digits its denominator is the even 9007199254741000).
    let (k, v) = panel("x^(1/9007199254740993)", &[-1.0], None);
    assert_eq!(
        (v[0], k.domain.as_str(), k.parity),
        (-1.0, "x ∈ ℝ", Parity::Odd)
    );
    let (k, v) = panel("x^(1/9007199254740993)", &[-1.0], Some(14));
    assert!(v[0].is_nan());
    assert_eq!(k.domain, "x ∈ [0, ∞)");
}

/// The curve drawn: x^(1000001/3) and x^(1/1000001) have their negative
/// halves (to 14 digits, the apps' default), and the cube root as
/// 1000001/3000003; to 5 digits x^(1/1000000) has none.
#[test]
fn written_ratios_draw_their_negative_half() {
    let curve = |src: &str, digits: Option<u8>| -> Vec<(f64, f64)> {
        let mut g = Graph::new();
        g.set_literal_digits(digits);
        let id = g.add_equation(&format!("y={src}"));
        let v = graphing::Viewport::new(-2.0, 2.0, -2.0, 2.0, 400.0, 400.0);
        let p = g.plot_equation(id, &v).unwrap();
        p.curves.iter().flatten().map(|p| (p.x, p.y)).collect()
    };
    for (src, f) in [
        (
            "x^(1000001/3)",
            (|x: f64| x.signum() * x.abs().powf(1000001.0 / 3.0)) as fn(f64) -> f64,
        ),
        ("x^(1/1000001)", |x: f64| {
            x.signum() * x.abs().powf(1.0 / 1000001.0)
        }),
        ("x^(1000001/3000003)", |x: f64| x.cbrt()),
    ] {
        for digits in [Some(14), Some(20), None] {
            let pts = curve(src, digits);
            let neg: Vec<_> = pts.iter().filter(|(x, _)| *x < -0.01).collect();
            // (x^(1000001/3) is −0 to the doubles on most of (−1, 0).)
            assert!(
                neg.len() > 10
                    && neg.iter().all(|(_, y)| *y <= 0.0)
                    && neg.iter().any(|(_, y)| *y < -0.5),
                "{src} ({digits:?}): {} points left of 0",
                neg.len()
            );
            for &&(x, y) in &neg {
                let want = f(x);
                if want.abs() < 2.0 {
                    assert!(
                        (y - want).abs() < 1e-6,
                        "{src} ({digits:?}) at {x}: {y} vs {want}"
                    );
                }
            }
        }
    }
    let pts = curve("x^(1/1000001)", Some(5));
    assert!(!pts.is_empty() && pts.iter().all(|(x, _)| *x >= 0.0));
}

/// Whole-number exponents of 10⁶ or more (`x^1000001`, `x^(−1000002)`,
/// `x^2000000`) by the same written-ratio reading, q = 1: domain ℝ (ℝ \ {0}
/// for a negative one), the parity the exponent's, every row (they took
/// the old value-based path in the side conditions and the Taylor core,
/// and the panel left all but parity and the y-intercept unknown). To 5
/// digits 1000001 is the even 1000000; 9007199254740993 is odd only from
/// 16 digits on.
#[test]
fn whole_exponents_of_any_size() {
    use graphing::analysis::Parity;
    // (f(−1), domain, range, parity, minima, inflections, vertical)
    type Want = (
        f64,
        &'static str,
        &'static str,
        Parity,
        &'static [&'static str],
        &'static [&'static str],
        &'static [&'static str],
    );
    let odd: Want = (-1.0, "x ∈ ℝ", "y ∈ ℝ", Parity::Odd, &[], &["(0, 0)"], &[]);
    let even: Want = (
        1.0,
        "x ∈ ℝ",
        "y ∈ [0, ∞)",
        Parity::Even,
        &["(0, 0)"],
        &[],
        &[],
    );
    let neg_even: Want = (
        1.0,
        "x ∈ ℝ \\ {0}",
        "y ∈ (0, ∞)",
        Parity::Even,
        &[],
        &[],
        &["x = 0"],
    );
    let neg_odd: Want = (
        -1.0,
        "x ∈ ℝ \\ {0}",
        "y ∈ ℝ \\ {0}",
        Parity::Odd,
        &[],
        &[],
        &["x = 0"],
    );
    for digits in PRECISIONS {
        let five = digits == Some(5);
        let past_doubles = matches!(digits, None | Some(16) | Some(20));
        for (src, want) in [
            ("x^1000001", if five { even } else { odd }),
            ("x^2000000", even),
            ("x^(-1000002)", neg_even),
            ("x^(-1000001)", if five { neg_even } else { neg_odd }),
            ("x^9007199254740993", if past_doubles { odd } else { even }),
        ] {
            let (k, v) = panel(src, &[-1.0, 1.0], digits);
            assert_eq!((v[0], v[1]), (want.0, 1.0), "{src} ({digits:?})");
            assert!(
                matches!(reference(src, -1.0, digits), R::V(r) if r.f() == want.0),
                "{src} ({digits:?})"
            );
            assert_eq!(
                (k.domain.as_str(), k.range.as_str(), k.parity),
                (want.1, want.2, want.3),
                "{src} ({digits:?})"
            );
            assert_eq!(k.minima, want.4, "{src} ({digits:?})");
            assert_eq!(k.inflection_points, want.5, "{src} ({digits:?})");
            assert_eq!(k.vertical_asymptotes, want.6, "{src} ({digits:?})");
            assert_eq!(k.too_complex_features, 0, "{src} ({digits:?})");
        }
    }
    // The Taylor core: the integer power's series on either side of 0 and
    // through it (it was any value for a base ≤ 0, possibly undefined).
    for (src, x, lo, hi) in [
        ("x^1000001", -1.0, -1.0, -1.0),
        ("x^1000001", 0.0, 0.0, 0.0),
        ("x^(-1000002)", -1.0, 1.0, 1.0),
    ] {
        let s = series(src, x, x);
        assert!(
            s[0].lo() == lo && s[0].hi() == hi,
            "{src} at {x}: {:?}",
            s[0]
        );
        assert!(derivs_valid(&s, 3), "{src} at {x}: {s:?}");
    }
    let s = series("x^(-1000002)", -0.5, 0.5);
    assert!(s[0].dec <= Dec::Trv, "{:?}", s[0]);
}

/// R15-L-01: a limit only of a function proven defined on a tail, each of
/// its constants proven defined (Def or better), not merely enclosed.
#[test]
fn a_limit_needs_its_constants_defined() {
    use graphing::simplify::{Dir, Limit, PiQ, Q, Settings, limit_at};
    let lim = |src: &str, dir: Dir| {
        let eq = Equation::parse(&format!("y={src}")).unwrap();
        let opts = CompileOptions::default();
        limit_at(eq.explicit().unwrap().1, dir, &Settings::new(&opts))
    };
    let c = "floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-30)))";
    for (src, dirs) in [
        (format!("{c}+1"), &[Dir::NegInf, Dir::PosInf][..]),
        (format!("exp(ln(x)+{c})/x"), &[Dir::PosInf][..]),
        (format!("{c}*x+1/x"), &[Dir::NegInf, Dir::PosInf][..]),
        (format!("exp(-x)+{c}"), &[Dir::PosInf][..]),
        (format!("atan(x)^({c}+2)"), &[Dir::NegInf, Dir::PosInf][..]),
    ] {
        for &dir in dirs {
            assert_eq!(lim(&src, dir), Limit::Unknown, "{src} at {dir:?}");
        }
    }
    // Its value nowhere: the scalar and the reference agree it is undefined.
    let c1 = format!("{c}+1");
    assert!(value(&c1, 0.0, None).is_nan());
    assert!(matches!(reference(&c1, 0.0, None), R::Undef));
    // Constants proven defined: as before.
    let one = Limit::Exact(PiQ { q: Q::ONE, k: 0 });
    assert_eq!(lim("1/(x^2-x)+1", Dir::PosInf), one);
    for (src, want) in [
        ("exp(-x)+1", 1.0),
        ("floor(sqrt(2))+1/x", 1.0),
        ("exp(ln(x)+1)/x", std::f64::consts::E),
        (
            "atan(x)+sqrt(2)",
            std::f64::consts::FRAC_PI_2 + std::f64::consts::SQRT_2,
        ),
    ] {
        match lim(src, Dir::PosInf) {
            Limit::Approx(i) => assert!(i.lo() <= want && want <= i.hi(), "{src}: {i:?}"),
            Limit::Exact(q) => assert!((q.to_f64() - want).abs() < 1e-12, "{src}: {q:?}"),
            other => panic!("{src}: {other:?}"),
        }
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
