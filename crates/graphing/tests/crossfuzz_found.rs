//! What the random cross-checking tester (`tests/crossfuzz.rs`) found
//! and was fixed: each case as the tester minimised it, checked directly.

use graphing::Equation;
use graphing::analysis::truth::{R, reval_typed};
use graphing::compile::{CompileOptions, Program};
use graphing::functions::{TrigUnit, ncr, npr};
use graphing::interval::{Ctx, Dec, DecInterval, Interval, enclose};

fn ast(src: &str) -> graphing::ast::Expr {
    let eq = Equation::parse(&format!("y={src}")).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    eq.explicit().expect("a function of x").1.clone()
}

fn scalar(src: &str, x: f64) -> f64 {
    Program::compile_typed(&ast(src), &CompileOptions::default())
        .expect("compiles")
        .eval(x, 0.0)
}

fn reference(src: &str, x: f64) -> R {
    reval_typed(&ast(src), x, TrigUnit::Radians)
}

fn enclosure(src: &str, lo: f64, hi: f64) -> DecInterval {
    enclose(
        &ast(src),
        Interval::new(lo, hi),
        &Ctx::new(CompileOptions::default()),
    )
}

/// The claims the asymptote rows of a certificate rest on.
fn asymptote_claims(a: &graphing::certify::Analysis) -> Vec<graphing::certify::Claim> {
    use graphing::certify::Row;
    let mut all = Vec::new();
    if let Row::Certified { cert, .. } | Row::Partial { cert, .. } = &a.horizontal {
        all.extend(cert.claims.iter().cloned());
    }
    if let Row::Certified { cert, .. } | Row::Partial { cert, .. } = &a.oblique {
        all.extend(cert.claims.iter().cloned());
    }
    all
}

/// The reference evaluator called a power undefined where y·ln|a| leaves
/// the doubles: the product overflowed to a NaN, read as "undefined". The
/// compiled program had it right both times.
#[test]
fn a_power_whose_logarithm_overflows_is_not_undefined_to_the_reference() {
    // 0.2913^(143^143) is 10^(−10³⁰⁷): far below the doubles, and positive.
    let f = "tanh(0.3)^(x^x)+(x+x)";
    assert_eq!(scalar(f, 143.0), 286.0);
    match reference(f, 143.0) {
        R::V(v) => assert_eq!(v.f(), 286.0),
        r => panic!("{f} at 143: {r:?}"),
    }
    match reference("tanh(0.3)^(x^x)", 143.0) {
        R::V(v) => assert!(v.sign() > 0.0 && v.f() == 0.0, "{v:?}"),
        r => panic!("{r:?}"),
    }
    // (−6.8·10¹⁰)^(170!): 170! is an even integer, so the power is beyond
    // the largest double: +∞ compiled, and no value the reference can
    // hold — but not undefined.
    let g = "x^(170!)+cos(x)";
    assert_eq!(scalar(g, -68129206905.79622), f64::INFINITY);
    assert!(
        matches!(reference(g, -68129206905.79622), R::Unknown),
        "{:?}",
        reference(g, -68129206905.79622)
    );
    // A base below 1 to that power is below every double; a non-integer
    // exponent of a negative base stays undefined.
    assert!(matches!(reference("0.5^(170!)", 0.0), R::V(v) if v.sign() > 0.0 && v.f() == 0.0));
    assert!(matches!(
        reference("x^(10^308+0.5)", -2.0),
        R::Undef | R::Unknown
    ));
}

/// The reference evaluator, as its powers: a root whose ln|x|/n leaves the
/// doubles is beyond them, not undefined.
#[test]
fn a_root_of_a_degree_near_zero_is_not_undefined_to_the_reference() {
    let f = "root(2*x,2*x)";
    let x = f64::MIN_POSITIVE;
    assert_eq!(scalar(f, x), 0.0);
    assert!(
        matches!(reference(f, x), R::V(v) if v.sign() > 0.0 && v.f() == 0.0),
        "{:?}",
        reference(f, x)
    );
}

/// The reference evaluator: a root of a degree beyond the doubles is 1
/// for a positive x a double holds (1/n is below every double), not
/// undefined. root(u, 6 + xˣ) from x = 144 on, where xˣ overflows.
#[test]
fn a_root_of_a_degree_beyond_the_doubles_is_not_undefined_to_the_reference() {
    let f = "root(x^(3/2),6+x^x)+(9+x)";
    match reference(f, 170.0) {
        R::V(v) => assert_eq!(v.f(), 180.0),
        r => panic!("{f} at 170: {r:?}"),
    }
    let e = enclosure(f, 170.0, 170.0);
    assert!(e.lo() <= 180.0 && 180.0 <= e.hi() && e.dec >= Dec::Def);
    // A negative x there: the degree's parity isn't known.
    assert!(matches!(
        reference("root(x,6+(-x)^(-x))", -170.0),
        R::Unknown
    ));
}

/// The extended-range power kept a mantissa of noise where t·log₂|b| is
/// past 2⁵³ through b's mantissa alone (a base in [1, 2), whose binary
/// exponent is 0): 1.5^x + 1 was ∞ at −10³⁰⁰ and 1 at 10³⁰⁰.
#[test]
fn a_power_far_beyond_the_doubles_keeps_its_side() {
    for (f, x, want) in [
        ("1.5^x+1", -1e300, 1.0),
        ("1.5^x+1", 1e300, f64::INFINITY),
        ("1.5^x+1", -1e17, 1.0),
        ("1.5^x+1", 1e17, f64::INFINITY),
        ("1.75^x+1", -1e300, 1.0),
        ("1.25^(-x)+1", -1e300, f64::INFINITY),
        ("1/(1.5^x)+1", 1e300, 1.0),
        ("0.75^x+1", 1e300, 1.0),
        ("0.75^x+1", -1e300, f64::INFINITY),
        ("2^x+1", -1e300, 1.0),
        ("1.5^x+1", -1e20, 1.0),
    ] {
        assert_eq!(scalar(f, x), want, "{f} at {x:e}");
    }
    assert_eq!(scalar("1.5^x", -1e300), 0.0);
}

/// nCr(n, k) and nPr(n, k) of a whole 0 ≤ k ≤ 60 are polynomials in n.
/// Γ's quotient lost them in doubles: undefined beside a whole n, wrong
/// in the twelfth digit at −172.5, 2.15 times too large at 5.6·10¹⁴.
#[test]
fn counts_of_a_small_whole_r_are_the_polynomial_in_n() {
    // n − k + 1 rounded onto Γ's pole.
    let n = 1.0 - f64::EPSILON / 2.0;
    assert_eq!(ncr(n, 2.0), n * (n - 1.0) / 2.0);
    assert!(scalar("nCr(abs(x),2)+sign(0.3)", -n).is_finite());
    // nCr(n, 1) is n.
    for n in [-172.5, -171.5, -0.5, 0.5, 1e-300, -5.5e-17, 12345.678] {
        assert_eq!(ncr(n, 1.0), n, "nCr({n}, 1)");
        assert_eq!(npr(n, 1.0), n, "nPr({n}, 1)");
        assert_eq!(ncr(n, 0.0), 1.0);
        assert_eq!(npr(n, 0.0), 1.0);
    }
    // Far out: n(n − 1)/2 to the last digits, not e^(a difference of two
    // logarithms of Γ near 10¹⁶).
    let n = 562_341_325_190_349.06_f64;
    let want = n * (n - 1.0) / 2.0;
    assert_eq!(ncr(n, 2.0), want);
    assert!((ncr(-n, 2.0) - (-n) * (-n - 1.0) / 2.0).abs() <= want * 1e-15);
    // A negative integer n is Γ's pole still: undefined.
    for k in [0.0, 1.0, 2.0, 60.0] {
        assert!(ncr(-3.0, k).is_nan() && npr(-3.0, k).is_nan());
    }
    // Whole n ≥ 0 are counted exactly, as before.
    assert_eq!(ncr(61.0, 62.0), 0.0);
    assert_eq!(ncr(10.0, 3.0), 120.0);
    assert_eq!(npr(10.0, 3.0), 720.0);
    // A product that leaves the doubles before k! brings it back.
    let big = ncr(1e30 + 0.5e15, 11.0);
    assert!(big.is_infinite() || big > 1e300, "{big:e}");
    let fits = ncr(1.5e27, 11.0);
    assert!(
        fits.is_finite() && (fits / (1.5e27f64.powi(11) / 39916800.0) - 1.0).abs() < 1e-12,
        "{fits:e}"
    );

    // The value lies in the interval form's point enclosure, whatever n:
    // the same operations in the same order.
    for (f, perm) in [("nCr", false), ("nPr", true)] {
        for k in [0u32, 1, 2, 3, 7, 20, 60] {
            let src = format!("{f}(x,{k})");
            for i in -400..=400 {
                let x = f64::from(i) * 0.37 + 0.013;
                let v = if perm {
                    npr(x, f64::from(k))
                } else {
                    ncr(x, f64::from(k))
                };
                let e = enclosure(&src, x, x);
                assert!(
                    v.is_finite() && e.lo() <= v && v <= e.hi(),
                    "{src} at {x}: {v:e} outside [{:e}, {:e}]",
                    e.lo(),
                    e.hi()
                );
                assert_eq!(scalar(&src, x), v);
            }
        }
    }
}

/// nPr of whole numbers n ≥ 0 and r < 0 is the count 0, where Γ's
/// quotient beside them is 1/((n + 1)…(n − r)): a box holding such a pair
/// holds that 0. nPr(|x|, ⌊x⌋) over [−62, −61] was enclosed as
/// [3·10⁻¹²⁴, 3·10⁻¹¹⁸], defined, though it is 0 at both ends.
#[test]
fn a_box_holding_a_zero_count_of_npr_encloses_it() {
    let f = "nPr(abs(x),floor(x))";
    assert_eq!(scalar(f, -62.0), 0.0);
    assert_eq!(scalar(f, -61.0), 0.0);
    let e = enclosure(f, -62.0, -61.0);
    assert!(
        e.lo() <= 0.0 && 0.0 <= e.hi(),
        "[{:e}, {:e}]",
        e.lo(),
        e.hi()
    );
    assert!(e.dec <= Dec::Def, "{:?}", e.dec);
    // A point r: 1/(n + 1) between the whole n, 0 at them.
    let g = "nPr(x,-1)";
    assert_eq!(scalar(g, 5.0), 0.0);
    assert!((scalar(g, 5.5) - 1.0 / 6.5).abs() < 1e-15);
    let e = enclosure(g, 5.0, 6.0);
    assert!(
        e.lo() <= 0.0 && e.hi() >= 1.0 / 6.0,
        "[{:e}, {:e}]",
        e.lo(),
        e.hi()
    );
    assert!(e.dec <= Dec::Def, "{:?}", e.dec);
    // Clear of the whole numbers nothing changes: continuous, away from 0.
    let e = enclosure(g, 5.25, 5.75);
    assert!(
        e.lo() > 0.0 && e.dec >= Dec::Dac,
        "[{:e}, {:e}] {:?}",
        e.lo(),
        e.hi(),
        e.dec
    );
    // nCr of a whole n ≥ 0 and r < 0 is 0 too, and undefined beside it
    // (Γ(r + 1) is at a pole): a box holding the whole n is not empty.
    assert_eq!(scalar("exp(nCr(x,-1))", 0.0), 1.0);
    let e = enclosure("exp(nCr(x,-1))", -5e-324, 0.0);
    assert!(
        !e.is_empty() && e.lo() <= 1.0 && 1.0 <= e.hi() && e.dec <= Dec::Trv,
        "{:?}",
        e
    );
    assert!(enclosure("nCr(x,-1)", 0.25, 0.75).dec <= Dec::Trv);
}

/// root(x, n) of a whole degree other than 2 and 3 went through `pow`
/// with 1/n rounded, which |ln x|/n magnifies: 14 ulps at 10⁸⁵, 40 at
/// 10²⁰⁰, outside f's enclosure. It is the power 1/n as `x^(1/n)` has it.
#[test]
fn a_root_of_a_whole_degree_keeps_its_digits_far_out() {
    use graphing::functions::root;
    let ulps = |v: f64, lo: f64, hi: f64| {
        let u = f64::from_bits(v.abs().to_bits() + 1) - v.abs();
        if v < lo {
            (lo - v) / u
        } else if v > hi {
            (v - hi) / u
        } else {
            0.0
        }
    };
    for n in [4, 5, 6, 7, 10, 11, 100, 1001, -4, -5, -7] {
        let src = format!("root(x,{n})");
        for x in [
            1e-300, 1e-100, 1e-17, 0.5, 2.0, 10.0, 1e17, 1e85, 1e100, 1e200, 1e300,
        ] {
            let v = root(x, f64::from(n));
            let e = enclosure(&src, x, x);
            assert!(
                ulps(v, e.lo(), e.hi()) <= 2.0,
                "{src} at {x:e}: {v:e} against [{:e}, {:e}]",
                e.lo(),
                e.hi()
            );
            assert_eq!(scalar(&src, x), v, "{src} at {x:e}");
            // The same real root as the power 1/n written out.
            let p = scalar(&format!("x^(1/{n})"), x);
            assert_eq!(v, p, "{src} against x^(1/{n}) at {x:e}");
        }
        if n % 2 != 0 {
            let x = -1e100;
            let v = root(x, f64::from(n));
            let e = enclosure(&src, x, x);
            assert!(
                v < 0.0 && ulps(v, e.lo(), e.hi()) <= 2.0,
                "{src} at {x:e}: {v:e}"
            );
        } else {
            assert!(root(-2.0, f64::from(n)).is_nan());
        }
    }
    assert_eq!(root(-1e100, 5.0), -1e20);
    assert_eq!(root(1e85, 5.0), 1e17);
    // A case found.
    let v = scalar("root(-(x^2),5)+0.25", -1e100);
    assert!((v / -1e40 - 1.0).abs() < 4e-16, "{v:e}");
    // What root always was: square and cube roots, 0, a degree of 0.
    assert_eq!(root(9.0, 2.0), 3.0);
    assert_eq!(root(-27.0, 3.0), -3.0);
    assert_eq!(root(0.0, 5.0), 0.0);
    assert!(root(0.0, -5.0).is_nan() && root(2.0, 0.0).is_nan() && root(-2.0, 2.0).is_nan());
    assert_eq!(root(32.0, 5.0), 2.0);
    assert_eq!(root(-32.0, 5.0), -2.0);
    assert_eq!(root(2.0, 1.0), 2.0);
    assert_eq!(root(4.0, -2.0), 0.5);
}

/// √(0.1 + 0.9 + x) is 0 at −1, the closed end of its domain: the sum of
/// the two literals is exactly 1. The certifier encloses that end by the
/// doubles either side, and took the gap about it for free of zeros
/// because the radicand is exactly 0 at a double of it: the panel said
/// "no x-intercepts".
#[test]
fn a_zero_at_a_domain_end_between_two_doubles_is_not_certified_away() {
    use graphing::analysis::{analyze, flags};
    for src in [
        "sqrt(0.1+0.9+x)",
        "sqrt((0.1+0.9)+(x+x))",
        "sqrt(x+0.1+0.9)",
        "(0.1+0.9+x)^(1/2)",
        "sqrt(0.1+0.9+x)+0*x",
    ] {
        for digits in [None, Some(14)] {
            let opts = graphing::lexer::ParseOptions {
                decimal_comma: false,
                literal_digits: digits,
            };
            let eq = Equation::parse_with(&format!("y={src}"), opts).expect("parses");
            let k = analyze(&eq, &CompileOptions::default());
            let unknown = k.too_complex_features & flags::ZEROS != 0;
            assert!(
                unknown || !k.data.zeros.is_empty(),
                "{src} at {digits:?}: x-intercepts {:?} (none), but f is 0 at its domain's end",
                k.x_intercept
            );
            for z in &k.data.zeros {
                assert!((scalar(src, z.x)).abs() < 1e-7, "{src}: a zero at {}", z.x);
            }
        }
    }
    // A zero at an end the certifier knows exactly is still found.
    let eq = Equation::parse("y=sqrt(0.25+0.75+x)").expect("parses");
    let k = analyze(&eq, &CompileOptions::default());
    assert_eq!(k.x_intercept, "−1", "{k:?}");
    // And a factor that is 0 at a double where f is undefined is no zero
    // of f's: the pole of cot(2x) at 0, where the sin(x + x) its second
    // derivative's tree keeps is 0, and the hole of x·cot(2x) there. (An
    // enclosure of either at 0 isn't empty: the cover's `Undefined` claim
    // is what says so.)
    use graphing::certify::{Claim, DEFAULT_BUDGET, Row, certify_text};
    for src in ["cot(2*x)", "cot(x/0.5)", "cot(x/0.001)"] {
        let a = certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None).expect(src);
        assert!(a.inflections.is_certified(), "{src}: {:?}", a.inflections);
    }
    let a = certify_text(
        "x*cot(2*x)",
        CompileOptions::default(),
        DEFAULT_BUDGET,
        None,
    )
    .expect("certifies");
    let (Row::Certified { cert, .. } | Row::Partial { cert, .. }) = &a.x_intercepts else {
        panic!("x*cot(2*x): {:?}", a.x_intercepts);
    };
    assert!(
        cert.claims.iter().any(|c| matches!(
            c,
            Claim::GapClear { x, .. } if x.a.0 < 0.0 && 0.0 < x.b.0
        )),
        "x*cot(2*x): no claim for the gap about its hole at 0"
    );
}

/// A row on f′ reads a kink box as one point, by f′'s signs on either
/// side. x^(2/1000003) crosses 0.01 at ±10⁻¹⁰⁰⁰⁰⁰³, between 0 and the
/// doubles beside it, so min(x^(2/1000003), 0.01) is 0.01 at every double
/// but 0, where it is 0: with f′ ≡ 0 on both sides of the box about 0 it
/// was certified constant with range {0}, and x + that, which drops by
/// 0.01 inside the box, "increasing on ℝ, no extrema".
#[test]
fn a_kink_box_f_dips_inside_is_not_read_as_a_point() {
    use graphing::analysis::{Monotonicity, analyze, flags};
    let panel = |src: &str| {
        let eq = Equation::parse(&format!("y={src}")).expect("parses");
        analyze(&eq, &CompileOptions::default())
    };
    let f = "min(x^(2/1000003),0.01)";
    assert_eq!(scalar(f, 0.0), 0.0);
    assert_eq!(scalar(f, 5e-324), 0.01);
    assert_eq!(scalar(f, -5e-324), 0.01);
    assert_eq!(scalar(f, 3.0), 0.01);
    let k = panel(f);
    let unknown = |flag: u32| k.too_complex_features & flag != 0;
    assert!(
        unknown(flags::RANGE)
            || k.data
                .range
                .iter()
                .any(|i| i.lo.value <= 0.0 && i.hi.value >= 0.01),
        "{f}: range {:?}",
        k.range
    );
    assert!(
        unknown(flags::MONOTONE_INTERVALS)
            || !k.monotonicity.iter().any(|m| m.1 == Monotonicity::Constant),
        "{f}: {:?}",
        k.monotonicity
    );

    let g = "x+min(x^(2/1000003),0.01)";
    assert!(scalar(g, -5e-324) > scalar(g, 0.0));
    let k = panel(g);
    assert!(
        k.too_complex_features & flags::MONOTONE_INTERVALS != 0 || k.monotonicity.len() > 1,
        "{g}: {:?}",
        k.monotonicity
    );
    assert!(
        (k.too_complex_features | k.partial_features) & flags::MINIMA != 0
            || !k.data.minima.is_empty(),
        "{g}: no minima, but f drops to 0 at 0"
    );

    // One kink in its box, f monotone on each side of it: as before.
    // (About 1, f's enclosure over the box alone is past its end values:
    // the test is of f at the box's doubles.)
    for src in ["x*abs(x)", "(x-1)*abs((x-1))"] {
        let k = panel(src);
        assert_eq!(
            k.too_complex_features & flags::MONOTONE_INTERVALS,
            0,
            "{src}"
        );
        assert_eq!(k.too_complex_features & flags::RANGE, 0, "{src}");
        assert_eq!(k.monotonicity.len(), 1, "{src}: {:?}", k.monotonicity);
    }
    let k = panel("x*abs(x)");
    assert_eq!(k.too_complex_features & flags::MONOTONE_INTERVALS, 0);
    assert_eq!(k.monotonicity.len(), 1, "{:?}", k.monotonicity);
    assert_eq!(k.monotonicity[0].1, Monotonicity::Increasing);
    let k = panel("abs(x)");
    assert_eq!(k.data.minima.len(), 1, "{:?}", k.minima);
    let k = panel("x^(2/3)");
    assert_eq!(k.data.minima.len(), 1, "{:?}", k.minima);
    assert_eq!(k.too_complex_features & flags::INFLECTION_POINTS, 0);
    let k = panel("x^(1/3)");
    assert_eq!(k.too_complex_features & flags::MONOTONE_INTERVALS, 0);
    assert_eq!(k.monotonicity.len(), 1, "{:?}", k.monotonicity);
}

/// The certifier's simplified form of f is f only where f is defined, and
/// has values where f has none. Its values at −15 and −13 were taken for
/// min(0·acosh x, x), whose domain is [1, ∞): they "showed" a function
/// that is 0 on all its domain to be no constant, gave its certificate the
/// asymptote y = 0, and were refuted by the replay.
#[test]
fn a_value_of_the_simplified_form_outside_the_domain_is_no_witness() {
    use graphing::certify::{Claim, DEFAULT_BUDGET, Row, certify_text};
    let f = "min(0*acosh(x),x)";
    let a = certify_text(f, CompileOptions::default(), DEFAULT_BUDGET, None).expect("certifies");
    for c in asymptote_claims(&a) {
        if let Claim::Value { x, .. } = &c {
            assert!(x.a.0 >= 1.0, "{f}: {c:?}, outside the domain [1, ∞)");
        }
    }
    // A constant on its domain has no asymptote, or the row is unknown.
    if let Row::Certified { value, .. } = &a.horizontal {
        assert!(value.is_empty(), "{f}: {value:?}");
    }
    // Where f is defined the witnesses are taken as before.
    let a = certify_text(
        "e^(-x)+0*x",
        CompileOptions::default(),
        DEFAULT_BUDGET,
        None,
    )
    .expect("certifies");
    assert!(a.horizontal.is_certified(), "{:?}", a.horizontal);
}

/// The tails' starts took no account of the domain, and the claims on a
/// tail are made from the simplified form: f″ beyond a bound on [1, ∞)
/// for 0·(x − 8)⁰ + 2ˣ, across its hole at 8, and the values of
/// 0.2/x·(4x)^(0·log(x, 2)) on [1, ∞), through 1 where the base of its
/// logarithm is 1. The replay refuted both.
#[test]
fn a_tail_starts_past_the_last_end_of_the_domain() {
    use graphing::certify::{Claim, DEFAULT_BUDGET, Tail, certify_text};
    let g = "0*(x-8)^0+2^x";
    let a = certify_text(g, CompileOptions::default(), DEFAULT_BUDGET, None).expect("certifies");
    assert!(a.oblique.is_certified(), "{g}: {:?}", a.oblique);
    for c in asymptote_claims(&a) {
        if let Claim::TailBeyond {
            side: Tail::Right,
            from,
            ..
        } = &c
        {
            assert!(from.0 > 8.0, "{g}: {c:?}, a tail across the hole at 8");
        }
    }
    let h = "0.2/x*(4*x)^(0*log(x,2))";
    let a = certify_text(h, CompileOptions::default(), DEFAULT_BUDGET, None).expect("certifies");
    for c in asymptote_claims(&a) {
        match &c {
            Claim::TailValue {
                side: Tail::Right,
                from,
                ..
            }
            | Claim::TailBeyond {
                side: Tail::Right,
                from,
                ..
            } => assert!(from.0 > 1.0, "{h}: {c:?}, a tail through 1"),
            _ => {}
        }
    }
    // Without a break in the domain the tails start where they did.
    let a =
        certify_text("2^x", CompileOptions::default(), DEFAULT_BUDGET, None).expect("certifies");
    assert!(a.oblique.is_certified() && a.horizontal.is_certified());
    assert!(asymptote_claims(&a).iter().any(|c| matches!(
        c,
        Claim::TailBeyond {
            side: Tail::Right,
            from,
            ..
        } if from.0 == 1.0
    )));
}

/// A closed end of the domain that no double holds is known to the
/// certifier by an enclosure, and a box about it reaches outside the
/// domain. (4 + 5x)^(x⁰) is (4 + 5x)¹ to the certifier, which has values
/// there: its minimum at −0.8 was certified on "f is continuous on
/// [−0.8000000000000003, −5·10⁻³²⁴]" and on f's values over the end's
/// enclosure, though f is undefined left of −0.8. The replay refuted both
/// claims. √(4 + 5x), whose tree is its own, never had the row.
#[test]
fn a_claim_about_f_at_an_enclosed_end_needs_f_defined_there() {
    use graphing::certify::{Claim, DEFAULT_BUDGET, Row, Subject, certify_text};
    for src in [
        "(4+5*x)^(x^0)",
        "1*x-(x^4+5*x)^((x-6)^0)",
        "(x-0.1)^(x^0)",
        "(x-0.3)^((x-6)^0)+x",
    ] {
        let a = certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None).expect(src);
        let mut claims = Vec::new();
        if let Row::Certified { cert, .. } | Row::Partial { cert, .. } = &a.extrema {
            claims.extend(cert.claims.iter().cloned());
        }
        if let Row::Certified { cert, .. } | Row::Partial { cert, .. } = &a.range {
            claims.extend(cert.claims.iter().cloned());
        }
        for c in &claims {
            let x = match c {
                Claim::Continuous { x } => x,
                Claim::Value { x, of, .. } if *of == Subject::f(0) => x,
                _ => continue,
            };
            for end in [x.a.0, x.b.0] {
                assert!(
                    !scalar(src, end).is_nan(),
                    "{src}: {c:?}, and f is undefined at {end:e}"
                );
            }
        }
        // What doesn't rest on the end's value is certified as before.
        assert!(
            a.domain.is_certified() && a.monotonicity.is_certified(),
            "{src}"
        );
    }
    // An end at a double is a point of the domain: its minimum as before.
    let a = certify_text(
        "(4+x)^(x^0)",
        CompileOptions::default(),
        DEFAULT_BUDGET,
        None,
    )
    .expect("certifies");
    assert!(a.extrema.is_certified(), "{:?}", a.extrema);
    assert!(a.range.is_certified(), "{:?}", a.range);
}

/// A power's written ratio went into pow as the double nearest it, and
/// pow(1000, 1/3) is 9.999999999999998: (x^(1/3))! had a value at −1000,
/// where the cube root is −10 and the factorial undefined (the interval
/// form, which takes cube roots exactly, had it right), ⌊x^(1/3)⌋ was 9 at
/// 1000, and ⌊x^(1/7)⌋ and ⌊root(x, 7)⌋ were 9 at 10⁷.
#[test]
fn a_power_with_a_written_ratio_is_the_root_where_that_is_a_double() {
    use graphing::functions::pow_rational;
    // The tester's case, and its minimised form.
    assert!(scalar("(x^(1/3))!", -1000.0).is_nan());
    assert!(scalar("1/(x^(1/3))!", -1000.0).is_nan());
    assert!(matches!(reference("(x^(1/3))!", -1000.0), R::Undef));
    // One function, three spellings: the same number to the bit.
    for x in [
        1000.0, -1000.0, 27.0, 0.001, 2.0, -7.5, 0.1, 1e-5, 1e10, 123456.789, 1e45, -1e-300, 3e200,
        5e-324,
    ] {
        let (a, b, c) = (
            scalar("x^(1/3)", x),
            scalar("root(x,3)", x),
            scalar("cbrt(x)", x),
        );
        assert_eq!(
            a.to_bits(),
            c.to_bits(),
            "x^(1/3) and cbrt(x) at {x:e}: {a:e}, {c:e}"
        );
        assert_eq!(
            b.to_bits(),
            c.to_bits(),
            "root(x,3) and cbrt(x) at {x:e}: {b:e}, {c:e}"
        );
    }
    for (f, x, want) in [
        ("floor(x^(1/3))", 1000.0, 10.0),
        ("floor(x^(1/7))", 1e7, 10.0),
        ("floor(root(x,7))", 1e7, 10.0),
        ("x^(2/3)", 8.0, 4.0),
        ("x^(2/3)", -1000.0, 100.0),
        ("x^(1/5)", 100000.0, 10.0),
        ("x^(1/5)", -100000.0, -10.0),
        ("x^(3/5)", 32.0, 8.0),
        ("x^(7/5)", 243.0, 2187.0),
        ("x^(-1/5)", 32.0, 0.5),
        ("x^(-2/3)", 8.0, 0.25),
        ("x^(3/2)", 1e300, 1e300f64.powf(1.5)),
    ] {
        assert_eq!(scalar(f, x), want, "{f} at {x:e}");
    }
    // Every q-th power of an odd g (and of g times a power of two) a
    // double holds has g for its root, and g² for the power 2/q.
    for q in [3i32, 5, 6, 7, 9, 10, 11, 12, 13, 15, 20, 33] {
        let most = 2f64.powi(53);
        let mut g = 3.0f64;
        let mut checked = 0;
        while g.powi(q) < most && checked < 3000 {
            for k in [-3i32, 0, 2] {
                let a = g.powi(q) * 2f64.powi(q * k);
                let root = g * 2f64.powi(k);
                assert_eq!(pow_rational(a, 1, q), root, "({a:e})^(1/{q})");
                if q % 2 != 0 {
                    assert_eq!(pow_rational(-a, 1, q), -root, "(-{a:e})^(1/{q})");
                    assert_eq!(pow_rational(a, 2, q), root * root, "({a:e})^(2/{q})");
                }
                if q > 3 {
                    assert_eq!(pow_rational(a, 3, q), root * root * root, "({a:e})^(3/{q})");
                }
            }
            checked += 1;
            // (Every g while there are few, then a spread of them.)
            g += if checked < 600 {
                2.0
            } else {
                2.0 * (1.0 + (g / 300.0).floor())
            };
        }
        assert!(checked > 0, "q = {q}");
    }
}

/// The rational power against MPFR: within an ulp at random bases, for
/// ratios of every kind (a double, a cube root, small and large parts,
/// near 1 and far).
#[cfg(feature = "mpfr-oracle")]
#[test]
fn a_power_with_a_written_ratio_is_within_an_ulp() {
    use graphing::functions::pow_rational;
    use rug::Float;
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    for (p, q) in [
        (1, 3),
        (2, 3),
        (-1, 3),
        (-2, 3),
        (4, 3),
        (1, 5),
        (2, 5),
        (-3, 5),
        (7, 5),
        (1, 6),
        (5, 6),
        (1, 7),
        (3, 4),
        (7, 4),
        (37, 100),
        (-9, 10),
        (22, 7),
        (1, 33),
        (32, 33),
        (1, 34),
        (999_999, 1_000_000),
        (1, 999_983),
        (123_457, 3),
    ] {
        for i in 0..4000 {
            // Near 1, ordinary sizes, and all of the doubles' range.
            let a = match i % 4 {
                0 => 0.5 + next(),
                1 => 1000.0 * next() + 1e-3,
                2 => 2f64.powf(60.0 * next() - 30.0),
                _ => 2f64.powf(2000.0 * next() - 1000.0),
            };
            let got = pow_rational(a, p, q);
            let mut l = Float::with_val(240, a);
            l.ln_mut();
            l *= p;
            l /= q;
            l.exp_mut();
            let want = l.to_f64();
            if !(want.is_finite() && want > f64::MIN_POSITIVE) {
                continue;
            }
            assert!(
                got == want || got == want.next_up() || got == want.next_down(),
                "({a:e})^({p}/{q}): {got:e}, and MPFR has {want:e}"
            );
        }
    }
}

/// A quotient of polynomials N/D has its oblique asymptote where N's
/// degree is D's and one more, and the certifier took "the exact
/// arithmetic has no line to give" for "the degrees differ": x²/(0.75 − 9007200000000000·x), whose line is
/// y = −x/9007200000000000 − 0.75/9007200000000000², the intercept's
/// denominator past 64 bits, was certified to have no oblique asymptote.
/// The replay refuted the fact (deg N − deg D = 1).
#[test]
fn no_oblique_asymptote_is_said_by_the_degrees_alone() {
    use graphing::certify::{Claim, DEFAULT_BUDGET, Row, certify_text, certify_text_with};
    let wrong = "f = N/D exactly with deg N ≠ deg D + 1";
    let says_degrees_differ = |row: &Row<Vec<graphing::certify::Oblique>>| {
        matches!(row, Row::Certified { cert, .. } | Row::Partial { cert, .. }
            if cert.claims.iter().any(|c| matches!(c, Claim::Simplifier { fact } if fact.starts_with(wrong))))
    };
    // The tester's case (seed 152, case 3071: five digits), and as rounded.
    let five = graphing::lexer::ParseOptions {
        decimal_comma: false,
        literal_digits: Some(5),
    };
    let a = certify_text_with(
        "x^2/((0.25+0.5)-x*9007199254740992)",
        five,
        CompileOptions::default(),
        DEFAULT_BUDGET,
        None,
    )
    .expect("certifies");
    assert!(!says_degrees_differ(&a.oblique), "{:?}", a.oblique);
    for src in [
        "x^2/(0.75-x*9007200000000000)",
        "x^2/(3-x*9007199254740993)",
        "(x^3+1)/(x^2*10^12-7)",
    ] {
        let a = certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None).expect(src);
        assert!(!says_degrees_differ(&a.oblique), "{src}: {:?}", a.oblique);
        // Its line, or nothing said: never "none".
        if let Row::Certified { value, .. } = &a.oblique {
            assert!(
                !value.is_empty(),
                "{src}: certified to have no oblique asymptote"
            );
        }
    }
    // Where the degrees do differ, and where the line is short, as before.
    let a = certify_text("x^3/(x-1)", CompileOptions::default(), DEFAULT_BUDGET, None)
        .expect("certifies");
    assert!(says_degrees_differ(&a.oblique), "{:?}", a.oblique);
    let a = certify_text(
        "x^2/(0.75-x*1000)",
        CompileOptions::default(),
        DEFAULT_BUDGET,
        None,
    )
    .expect("certifies");
    assert!(
        matches!(&a.oblique, Row::Certified { value, .. } if value.len() == 2),
        "{:?}",
        a.oblique
    );
}

/// A function constant on the whole line is "constant" in the panel's
/// period row (every shift is a period). The panel knew a constant by its
/// value, and e^(10¹⁶) has none in the doubles: sin x − sin x + e^(10¹⁶)
/// was "not periodic" there, beside "(−∞, ∞) constant" in the
/// monotonicity row, while e^(10¹⁶) alone was "constant".
#[test]
fn a_constant_whose_value_is_past_the_doubles_is_constant_in_the_period_row() {
    use graphing::analysis::{Periodicity, analyze};
    for src in [
        "sin(x)-sin(x)+exp(10^16)",
        "exp(10^16)",
        "cos(x)-cos(x)+10^400",
        "sin(x)-sin(x)+exp(2)",
        "exp(2)+0*x",
    ] {
        let eq = Equation::parse(&format!("y={src}")).expect("parses");
        let k = analyze(&eq, &CompileOptions::default());
        assert_eq!(k.periodicity_direction, Periodicity::Constant, "{src}");
    }
    // A function that isn't constant keeps its row.
    let eq = Equation::parse("y=x+exp(10^16)-exp(10^16)+x^2").expect("parses");
    let k = analyze(&eq, &CompileOptions::default());
    assert_ne!(k.periodicity_direction, Periodicity::Constant);
}
