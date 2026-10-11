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
