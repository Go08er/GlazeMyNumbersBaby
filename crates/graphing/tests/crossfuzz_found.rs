//! What the random cross-checking tester (`tests/crossfuzz.rs`) found
//! and was fixed: each case as the tester minimised it, checked directly.

use graphing::Equation;
use graphing::analysis::truth::{R, bind_variables, reval_typed};
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

/// [`scalar`], [`reference`] and [`enclosure`] with sliders and an angle
/// unit, as the tester had a case.
fn scalar_in(src: &str, x: f64, opts: CompileOptions<'_>) -> f64 {
    Program::compile_typed(&ast(src), &opts)
        .expect("compiles")
        .eval(x, 0.0)
}

fn reference_in(src: &str, x: f64, opts: CompileOptions<'_>) -> R {
    reval_typed(&bind_variables(&ast(src), &opts), x, opts.trig_unit)
}

fn enclosure_in(src: &str, lo: f64, hi: f64, opts: CompileOptions<'_>) -> DecInterval {
    enclose(&ast(src), Interval::new(lo, hi), &Ctx::new(opts))
}

/// Within `ulps` ulps of `want`.
fn within(got: f64, want: f64, ulps: f64) -> bool {
    let ulp = want.abs().next_up() - want.abs();
    (got - want).abs() <= ulps * ulp
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

/// nCr and nPr through Γ far out and next to zero, where the count has a
/// value: Γ's quotient in doubles was undefined there (an overflow times
/// an underflow, two underflows, n − r + 1 rounded onto a pole) or wrong
/// (two logarithms of Γ near 3·10¹⁶ subtracted), in the compiled
/// evaluator and the reference both, and nPr(x, 62)'s enclosure was ℝ,
/// possibly undefined, beside each of its zeros. (Values from MPFR.)
#[test]
fn counts_through_gamma_have_their_values_far_out_and_next_to_zero() {
    let within = |got: f64, want: f64, ulps: f64| {
        let ulp = want.abs().next_up() - want.abs();
        (got - want).abs() <= ulps * ulp
    };
    let x = -562_341_325_190_349.06_f64;
    for (f, v, want) in [
        // x − 61 rounds onto −61 next to 0, onto −60 next to 1.
        ("nPr(1e-90,62)", npr(1e-90, 62.0), -5.075_802_138_772_248e-7),
        (
            "nPr(-1e-90,62)",
            npr(-1e-90, 62.0),
            5.075_802_138_772_248e-7,
        ),
        (
            "nPr(5e-324,62)",
            npr(5e-324, 62.0),
            -2.507_779_461_854_891_4e-240,
        ),
        (
            "nPr(1+ε,62)",
            npr(1.0 + f64::EPSILON, 62.0),
            1.847_630_296_034_937_4e66,
        ),
        (
            "nCr(1e-300,62)",
            ncr(1e-300, 62.0),
            -1.612_903_225_806_451_7e-302,
        ),
        // Γ(123457.7) overflows, 1/Γ(−123448.7) underflows.
        (
            "nCr(7,123456.7)",
            ncr(7.0, 123_456.7),
            -2.405_578_290_225_185_4e-38,
        ),
        (
            "nCr(-0.5,123456.7)",
            ncr(-0.5, 123_456.7),
            -9.438_130_482_289_52e-4,
        ),
        ("nCr(0.5,1e6)", ncr(0.5, 1e6), -2.820_948_975_594_801_3e-10),
        ("nCr(0.1,1e20)", ncr(0.1, 1e20), -9.357_787_209_128_726e-24),
        (
            "nCr(4,x-0.5)",
            ncr(4.0, -1e15 - 0.5),
            7.639_437_268_410_881e-75,
        ),
        (
            "nCr(1,x-0.5)",
            ncr(1.0, -1e15 - 0.5),
            3.183_098_861_837_9e-31,
        ),
        // Two underflows.
        ("nPr(2x,0.5)", npr(2.0 * x, 0.5), 8.096_376_267_324_966e7),
        (
            "nPr(-200.5,100.25)",
            npr(-200.5, 100.25),
            9.327_598_513_520_478e239,
        ),
        // Two logarithms of Γ near 3·10¹⁶, and past the doubles' Γ.
        (
            "nPr(1e15+0.5,0.5)",
            npr(1e15 + 0.5, 0.5),
            3.162_277_660_168_380_7e7,
        ),
        (
            "nCr(1e300,0.5)",
            ncr(1e300, 0.5),
            1.128_379_167_095_512_7e150,
        ),
        (
            "nPr(172,173-2^-45)",
            npr(172.0, 172.999_999_999_999_97),
            6.066_759_058_212_55e297,
        ),
        // n − r + 1 rounded: 297 ulps.
        ("nPr(169.3,0.7)", npr(169.3, 0.7), 36.335_348_186_115_574),
    ] {
        assert!(within(v, want, 4.0), "{f} = {v:e}, not {want:e}");
    }
    // Beyond the doubles, at a pole, below the doubles: as they are.
    assert_eq!(npr(x, 62.0), f64::INFINITY);
    assert!(npr(-2e15, 0.5).is_nan() && ncr(-100.5, -200.0).is_nan());
    assert!(ncr(0.5, 1e15 + 0.5).is_nan(), "n − r + 1 = −10¹⁵, a pole");
    assert_eq!(npr(10.5, -1e6 - 0.25), 0.0);
    // The compiled program and the reference have them, and the interval
    // form encloses nPr(x, 62) next to its zeros, continuous.
    assert_eq!(scalar("x*nCr(7,123456.7)", 2.0), 2.0 * ncr(7.0, 123_456.7));
    // A count of a subnormal argument computed in x (redone in extended
    // range, where it was no double to take the count of: "can't tell").
    assert_eq!(scalar("nCr(7,2*x)", -5e-324), 1.0);
    assert_eq!(scalar("nPr(3.5,x/2)", 1e-320), 1.0);
    for (src, at) in [("nPr(x,62)", 1e-90), ("nPr(2*x,0.5)", x)] {
        let v = scalar(src, at);
        assert!(v.is_finite(), "{src} at {at:e}");
        match reference(src, at) {
            R::V(r) => assert_eq!(r.f(), v, "{src} at {at:e}"),
            r => panic!("{src} at {at:e}: {r:?}"),
        }
    }
    for at in [1e-90, -1e-300, 1.0 + f64::EPSILON, 17.0 - 1e-14] {
        let v = npr(at, 62.0);
        let e = enclosure("nPr(x,62)", at, at);
        assert!(
            e.dec >= Dec::Dac && e.lo() <= v && v <= e.hi() && e.hi() - e.lo() <= 1e-12 * v.abs(),
            "nPr({at:e}, 62) = {v:e}: {e:?}"
        );
    }
    let e = enclosure("nPr(x,62)", -0.01, 0.01);
    assert!(e.dec >= Dec::Dac && e.lo() < 0.0 && 0.0 < e.hi(), "{e:?}");
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
    // The function has no inflections, but generic continuity of its
    // kink box does not prove f″ has no other event inside. Until an
    // order-two proof covers that box, an unknown row is sound; a listed
    // inflection would be wrong. The exact minimum above stays proven.
    assert!(
        k.data.inflection_points.is_empty(),
        "{:?}",
        k.inflection_points
    );
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

/// A min or max of three or more arguments has its kinks where two of
/// them meet, and the certifier listed kinks for two arguments only: for
/// min(x, 1, 2 − x) no row knew of the kink at 1, and "no inflection
/// points" rested on the claim that f″, which is 0 off the kink and
/// doesn't exist at it, is 0 on the whole line (the replay left it
/// open, so the genuine certificate failed the strict replay).
#[test]
fn a_min_of_three_arguments_has_its_kinks() {
    use graphing::certify::{Claim, DEFAULT_BUDGET, Row, Subject, certify_text};
    let f = "min(x,1,2-x)";
    let a = certify_text(f, CompileOptions::default(), DEFAULT_BUDGET, None).expect("certifies");
    let mut claims = Vec::new();
    if let Row::Certified { cert, .. } | Row::Partial { cert, .. } = &a.inflections {
        claims.extend(cert.claims.iter().cloned());
    }
    if let Row::Certified { cert, .. } | Row::Partial { cert, .. } = &a.extrema {
        claims.extend(cert.claims.iter().cloned());
    }
    // Nothing is said of f″ (or f′) as a value on a box with the kink
    // strictly inside it.
    for c in &claims {
        if let Claim::Value { x, of, .. } = c
            && (*of == Subject::f(1) || *of == Subject::f(2))
        {
            assert!(
                !(x.a.0 < 1.0 && 1.0 < x.b.0),
                "{f}: {c:?}, across the kink at 1"
            );
        }
    }
    // The maximum is at the kink, and is found (or the row is not shown).
    if let Row::Certified { value, .. } = &a.extrema {
        assert_eq!(value.len(), 1, "{value:?}");
        let e = &value[0];
        assert!(e.x.lo.0 <= 1.0 && 1.0 <= e.x.hi.0, "{e:?}");
        assert!(e.y.lo.0 <= 1.0 && 1.0 <= e.y.hi.0, "{e:?}");
    }
    // Three arguments, the third never the greatest: the two-argument
    // function's extrema, as many.
    let three = certify_text(
        "max(x^2,2-x^2,-5)",
        CompileOptions::default(),
        DEFAULT_BUDGET,
        None,
    )
    .expect("certifies");
    let two = certify_text(
        "max(x^2,2-x^2)",
        CompileOptions::default(),
        DEFAULT_BUDGET,
        None,
    )
    .expect("certifies");
    if let (Row::Certified { value: a, .. }, Row::Certified { value: b, .. }) =
        (&three.extrema, &two.extrema)
    {
        assert_eq!(a.len(), b.len(), "{a:?} against {b:?}");
    }
}

/// A negative whole power was 1/bⁿ, and where bⁿ is past the largest
/// double that is 1/∞: (9007199254740993¹⁰)⁻², 8.1·10⁻³²⁰ and a
/// (subnormal) double, was 0 in the compiled evaluator and right in the
/// reference.
#[test]
fn a_negative_whole_power_past_the_doubles_is_not_zero_too_soon() {
    use graphing::functions::pow_int;
    let f = "(9007199254740993^x)^(-2)";
    let v = scalar(f, 10.0);
    assert!(v > 0.0 && v < f64::MIN_POSITIVE, "{f} at 10: {v:e}");
    match reference(f, 10.0) {
        R::V(r) => assert_eq!(r.f(), v, "{r:?}"),
        r => panic!("{r:?}"),
    }
    // b⁻ⁿ where bⁿ overflows and the power is a double; and where it is
    // below them all.
    let v = pow_int(3e159, -2);
    assert!(v > 1.1e-319 && v < 1.2e-319, "{v:e}");
    assert_eq!(pow_int(-3e159, -2), v);
    let v = pow_int(-1e105, -3);
    assert!(v < -0.9e-315 && v > -1.1e-315, "{v:e}");
    assert_eq!(pow_int(1e200, -2), 0.0);
    // Where bⁿ is a double the power is the division, as it was.
    for (b, n) in [(3.0f64, -2), (-7.5, -3), (1e100, -3), (2.5e-200, -1)] {
        let p = match n {
            -1 => b,
            -2 => b * b,
            _ => b * b * b,
        };
        assert_eq!(pow_int(b, n), 1.0 / p, "{b}^{n}");
    }
}

/// round is half away from zero (round(−2.5) is −3: the compiled
/// evaluator's, the reference's and the certificate's), and the panel's
/// exact arithmetic on constants had ⌊r + ½⌋: for a slider b at −2.5,
/// 8 + 1.5 − (round(b) − 0.99999999999999999) was shown with the range
/// {≈12.5} and drawn at 13.5, and round(−2.5) alone as y = −2.
#[test]
fn the_panels_round_of_a_negative_half_is_away_from_zero() {
    use graphing::analysis::analyze;
    for (src, want) in [
        ("round(-2.5)", -3.0),
        ("round(-2.5)+0*x", -3.0),
        ("round(-0.5)", -1.0),
        ("round(-7/2)", -4.0),
        ("round(2.5)", 3.0),
        ("round(-2.4)", -2.0),
        ("round(-2.6)", -3.0),
        ("round(0.5)", 1.0),
        ("8+1.5-(round(-2.5)-1)", 13.5),
    ] {
        assert_eq!(scalar(src, 0.0), want, "{src}");
        let eq = Equation::parse(&format!("y={src}")).expect("parses");
        let k = analyze(&eq, &CompileOptions::default());
        assert_eq!(k.data.y_intercept, Some(want), "{src}: {}", k.y_intercept);
        // (The range where the panel has one: of a constant with no x.)
        assert!(
            src.contains('x') || k.data.range.len() == 1,
            "{src}: {:?}",
            k.range
        );
        if let Some(r) = k.data.range.first() {
            assert_eq!(r.lo.value, want, "{src}: {:?}", k.range);
        }
    }
}

/// A kink box is claimed continuous (a `Kink`) so that the rows may read
/// f′ on either side of it, and the certifier asked its simplified form,
/// which has values where f has none: |x|·(2x + 0·x^0.5) is |x|·2x to it,
/// the kink of |x| is at 0, the closed end of the domain [0, ∞), and f is
/// undefined on the left half of the box. The replay refuted the claim.
#[test]
fn a_kink_box_is_continuous_by_fs_own_tree_too() {
    use graphing::certify::{Claim, DEFAULT_BUDGET, Row, certify_text};
    for src in [
        "abs(x)*(x+x+0*x^0.5)",
        "abs(x)*(0*x^0.5)",
        "abs(x)+0*sqrt(x)",
    ] {
        let a = certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None).expect(src);
        let mut claims = Vec::new();
        for row in [
            &a.extrema as &dyn std::any::Any,
            &a.inflections,
            &a.monotonicity,
        ] {
            if let Some(Row::Certified { cert, .. } | Row::Partial { cert, .. }) =
                row.downcast_ref::<Row<Vec<graphing::certify::Extremum>>>()
            {
                claims.extend(cert.claims.iter().cloned());
            }
            if let Some(Row::Certified { cert, .. } | Row::Partial { cert, .. }) =
                row.downcast_ref::<Row<Vec<graphing::certify::Monotone>>>()
            {
                claims.extend(cert.claims.iter().cloned());
            }
        }
        for c in &claims {
            if let Claim::Kink { x } = c {
                for end in [x.a.0, x.b.0] {
                    assert!(
                        !scalar(src, end).is_nan(),
                        "{src}: {c:?}, and f is undefined at {end:e}"
                    );
                }
            }
        }
        assert!(a.domain.is_certified(), "{src}");
    }
    // A kink inside the domain is claimed as before.
    let a = certify_text(
        "abs(x)*(x+1)",
        CompileOptions::default(),
        DEFAULT_BUDGET,
        None,
    )
    .expect("certifies");
    assert!(a.extrema.is_certified(), "{:?}", a.extrema);
}

/// A degree below every double and not 0 has the double 0, and the root
/// of degree 0 is undefined: root(x, 0.2ˣ) at 562, whose degree is
/// 10⁻³⁹³, was "undefined" to the reference and "can't tell" to the
/// compiled evaluator, where the interval form has the root past every
/// double. It keeps its side; a degree that is 0 stays undefined.
#[test]
fn a_root_of_a_degree_below_the_doubles_keeps_its_side() {
    let f = "root(x,0.2^x)";
    for x in [562.0, 1000.0] {
        assert_eq!(scalar(f, x), f64::INFINITY, "{f} at {x}");
        assert!(!matches!(reference(f, x), R::Undef), "{f} at {x}");
    }
    assert_eq!(scalar("1^(2/3)-root(x-8,0.2^x)", 1000.0), f64::NEG_INFINITY);
    // Below 1 the root is below every double; of 1 it is 1; a negative
    // degree turns the two about; a negative x has no such root.
    assert_eq!(scalar("root(x/1000,0.2^x)", 562.0), 0.0);
    match reference("root(x/1000,0.2^x)", 562.0) {
        R::V(v) => assert!(v.sign() > 0.0 && v.f() == 0.0, "{v:?}"),
        r => panic!("{r:?}"),
    }
    assert_eq!(scalar("root(x/x,0.2^x)", 562.0), 1.0);
    assert_eq!(scalar("root(x,-(0.2^x))", 562.0), 0.0);
    assert_eq!(scalar("root(x/1000,-(0.2^x))", 562.0), f64::INFINITY);
    assert!(scalar("root(-x,0.2^x)", 562.0).is_nan());
    assert!(matches!(reference("root(-x,0.2^x)", 562.0), R::Undef));
    // A degree that is 0 is no degree.
    for g in ["root(x,x-x)", "root(x,0*x)", "root(x,0)"] {
        assert!(scalar(g, 562.0).is_nan(), "{g}");
        assert!(matches!(reference(g, 562.0), R::Undef), "{g}");
    }
    // And a degree the doubles hold is as it was.
    let v = scalar("root(x,0.2^x)", 2.0);
    assert!((v - 2f64.powi(25)).abs() < 1e-3, "{v}");
}

/// What a limit "looks like" is a closed form recognised in its proven
/// enclosure, and was taken with two ulps of slack: beside y ∈
/// [0.30000000000000004, 0.3000000000000001] the certificate said "looks
/// like 0.3", which the enclosure rules out.
#[test]
fn what_a_limit_looks_like_lies_in_its_enclosure() {
    use graphing::certify::{DEFAULT_BUDGET, Row, certify_text_with};
    let twenty = graphing::lexer::ParseOptions {
        decimal_comma: false,
        literal_digits: Some(20),
    };
    let mut hinted = 0;
    for src in [
        "min(18014398509481985*x,0.30000000000000004441)",
        "min(18014398509481985*x,0.3)",
        "min(18014398509481985*x,0.29999999999999998)",
        "max(-18014398509481985*x,0.5000000000000001)",
    ] {
        let a = certify_text_with(src, twenty, CompileOptions::default(), DEFAULT_BUDGET, None)
            .expect(src);
        let Row::Certified { value, .. } = &a.horizontal else {
            panic!("{src}: {:?}", a.horizontal);
        };
        assert!(!value.is_empty(), "{src}");
        for h in value {
            if let Some(v) = h.looks_like {
                assert!(h.y.lo.0 <= v.0 && v.0 <= h.y.hi.0, "{src}: {h:?}");
                hinted += 1;
            }
        }
    }
    // (The hint is still given where the closed form is in the enclosure.)
    assert!(hinted > 0);
}

/// x·ln(x)² takes its least value 0 at 1 and has it for its limit at 0⁺
/// as well. The range's low end, closed, named the limit alone as where
/// it comes from (the two ends were one value, and the first's sources
/// were kept), and the replay read "a range end at 0 closed but its
/// sources say otherwise". A closed end's sources are each a value taken.
/// (Seed 1409, case 3532.)
#[test]
fn a_closed_range_end_names_where_it_is_taken() {
    use graphing::certify::rows::EndSrc;
    use graphing::certify::{Bound, DEFAULT_BUDGET, Row, certify_text};
    for src in ["x*ln(x)^2", "x^2*ln(x)^2", "x*ln(x)"] {
        let a = certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None).expect(src);
        let Row::Certified { value, .. } = &a.range else {
            panic!("{src}: {:?}", a.range);
        };
        for (i, p) in value.iter().enumerate() {
            for (j, b) in [p.lo, p.hi].into_iter().enumerate() {
                let Bound::At { closed, .. } = b else {
                    continue;
                };
                let srcs = &a.range_ends[i][j];
                let taken = srcs.iter().filter(|s| matches!(s, EndSrc::At(_))).count();
                assert!(
                    !srcs.is_empty()
                        && (if closed {
                            taken == srcs.len()
                        } else {
                            taken == 0
                        }),
                    "{src}: end {j} of piece {i}, closed {closed}: {srcs:?}"
                );
            }
        }
    }
}

/// A count below the doubles inside a larger expression was taken as its
/// double: nCr(x, 3) at ±5·10⁻³²⁴ is ±1.7·10⁻³²⁴, which rounds to 0, so
/// a/nCr(x, 3) (a = −2.5, degrees) was undefined there to the compiled
/// evaluator and the reference both, where it is ∓1.5·10³²⁴ and the
/// interval form encloses it past the doubles; and ln nCr(x, 2) at
/// −5·10⁻³²⁴, of a count of 2.5·10⁻³²⁴, was undefined for −745.13. (Seed
/// 1733; values: the doubles nearest MPFR's.)
#[test]
fn a_count_below_the_doubles_inside_an_expression_has_its_value() {
    let sliders = [("a", -2.5), ("b", 0.0)];
    let deg = CompileOptions {
        trig_unit: TrigUnit::Degrees,
        variables: &sliders,
    };
    // a/nCr(x, 3) = −15/(x(x − 1)(x − 2)) is ∓7.5·2¹⁰⁷⁴ to every digit a
    // double holds: beyond the doubles, and that to the reference.
    let f = "a/nCr(x,3)";
    for (x, side) in [(5e-324, -1.0), (-5e-324, 1.0)] {
        let far = side * f64::INFINITY;
        assert_eq!(scalar_in(f, x, deg), far, "{f} at {x:e}");
        match reference_in(f, x, deg) {
            R::V(v) => assert!(v.m == side * 0.9375 && v.e == 1077, "{f} at {x:e}: {v:?}"),
            r => panic!("{f} at {x:e}: {r:?}"),
        }
        let (lo, hi) = if x > 0.0 {
            (x, f64::MIN_POSITIVE)
        } else {
            (-f64::MIN_POSITIVE, x)
        };
        for e in [enclosure_in(f, x, x, deg), enclosure_in(f, lo, hi, deg)] {
            assert!(
                e.dec >= Dec::Dac && e.lo() <= far && far <= e.hi(),
                "{f} at {x:e}: {e:?}"
            );
        }
    }
    // nCr(x, 2) = 2⁻¹⁰⁷⁵·(1 + 2⁻¹⁰⁷⁴) there: its logarithm is −1075·ln 2.
    let g = "ln(nCr(x,2))";
    let want = -745.133_219_101_941_2;
    let v = scalar_in(g, -5e-324, deg);
    assert!(within(v, want, 1.0), "{g}: {v:e}");
    assert!(
        matches!(reference_in(g, -5e-324, deg), R::V(r) if within(r.f(), want, 1.0)),
        "{g}: {:?}",
        reference_in(g, -5e-324, deg)
    );
    let e = enclosure_in(g, -5e-324, -5e-324, deg);
    assert!(e.dec >= Dec::Dac && e.lo() <= v && v <= e.hi(), "{e:?}");
    // The value itself, not only its side: x/nCr(x, 3) is 3 next to 0,
    // where the count is below the doubles or a subnormal, whose rounding
    // was up to 2⁻¹¹ of it off.
    for x in [5e-324, -5e-324, 1e-320, -3e-310] {
        assert_eq!(scalar("x/nCr(x,3)", x), 3.0, "at {x:e}");
        assert!(
            matches!(reference("x/nCr(x,3)", x), R::V(v) if v.f() == 3.0),
            "at {x:e}: {:?}",
            reference("x/nCr(x,3)", x)
        );
    }
    // Times a factor that brings it back: nCr(2⁻¹⁰⁷⁴, 3) is 2⁻¹⁰⁷⁴/3 to
    // every digit a double holds. (By 2¹⁰⁰⁰, a double, or 2¹¹⁰⁰, past
    // them, it was 0, not 2⁻⁷⁴/3 or 2²⁶/3.)
    for (src, want) in [
        ("nCr(x,3)*2^1000", 2f64.powi(-74) / 3.0),
        ("nCr(x,3)*2^1100", 2f64.powi(26) / 3.0),
    ] {
        let v = scalar(src, 5e-324);
        assert!(within(v, want, 2.0), "{src}: {v:e}, not {want:e}");
        assert!(
            matches!(reference(src, 5e-324), R::V(r) if within(r.f(), want, 2.0)),
            "{src}: {:?}",
            reference(src, 5e-324)
        );
    }
    // A count that is 0, or at a pole of Γ, has no reciprocal or logarithm.
    for (src, x) in [
        ("1/nCr(x,3)", 2.0),
        ("ln(nCr(x,3))", 1.0),
        ("1/nPr(x,62)", 61.0),
        ("1/nPr(x,62)", -3.0),
        ("ln(nCr(x,0.5))", -1.0),
    ] {
        assert!(scalar(src, x).is_nan(), "{src} at {x}");
        assert!(matches!(reference(src, x), R::Undef), "{src} at {x}");
    }
}

/// A count past the doubles inside a larger expression was +∞ to the
/// compiled evaluator (and unknown to the reference): nPr(x, 62)/x⁶² near
/// −5.6·10¹⁴, where both overflow, was +∞ for about 1 (the README's
/// known-open note). And Γ there was e^(ln Γ) with ln Γ a double, about 13
/// digits (x!/(x·(x − 1)!) at 10⁶ was 1.00000000000017; the tester's
/// known-open table had it at Γ(171.5)); x!! past 300 was beyond even the
/// extended range, so x!!/x!! was "can't tell". Each is its own value now,
/// within an ulp or so. (Values: the doubles nearest MPFR's.)
#[test]
fn a_count_past_the_doubles_inside_an_expression_has_its_value() {
    let f = "nPr(x,62)/x^62";
    for (x, want) in [
        (-562_341_325_190_349.06, 1.000_000_000_003_362_6),
        (-559_999_999_999_999.5, 1.000_000_000_003_376_9),
        (-560_000_000_000_000.25, 1.000_000_000_003_376_9),
        (560_000_000_000_000.25, 0.999_999_999_996_623_3),
    ] {
        let v = scalar(f, x);
        assert!(within(v, want, 2.0), "{f} at {x:e}: {v:e}, not {want:e}");
        assert!(
            matches!(reference(f, x), R::V(r) if within(r.f(), want, 2.0)),
            "{f} at {x:e}: {:?}",
            reference(f, x)
        );
    }
    // −5.6·10¹⁴ itself is a whole number: Γ(x + 1) is at a pole there, a
    // hole in the polynomial.
    assert!(scalar(f, -5.6e14).is_nan());
    assert!(matches!(reference(f, -5.6e14), R::Undef));
    assert!(enclosure(f, -5.6e14, -5.6e14).is_empty());
    for (g, x, want) in [
        ("x!/(x*(x-1)!)", 1e6, 1.0),
        ("x!/(x-1)!", 1e10 + 0.5, 1e10 + 0.5),
        ("(x+1)!/x!", 1000.5, 1001.5),
        ("1/(x-0.5)!*10^300", 172.0, 6.148_558_251_064_136e-11),
        // Γ(−199.5), below the doubles: it was "can't tell".
        ("x!*10^300", -200.5, 5.636_995_190_178_567e-74),
        ("x!!/x!!", 400.0, 1.0),
        ("x!!/(x-2)!!", 401.0, 401.0),
    ] {
        let v = scalar(g, x);
        assert!(within(v, want, 2.0), "{g} at {x:e}: {v:e}, not {want:e}");
        assert!(
            matches!(reference(g, x), R::V(r) if within(r.f(), want, 2.0)),
            "{g} at {x:e}: {:?}",
            reference(g, x)
        );
        let e = enclosure(g, x, x);
        assert!(e.lo() <= v && v <= e.hi(), "{g} at {x:e}: {v:e}, {e:?}");
    }
}

/// A count far past the extended range had its side from the sign of
/// Σ ±(x/t)·(ln x − 1) in doubles, x each argument of Γ and t the
/// largest, and for C(10²⁰⁰, 10¹⁰⁰) the first two parts cancel exactly
/// there (10²⁰⁰ − 10¹⁰⁰ rounds to 10²⁰⁰): what was left, the lone 10¹⁰⁰'s,
/// put a count of about e^(2.3·10¹⁰²) below the range. So nCr(x², x) at
/// 10¹⁰⁰ was below the range to the reference where the compiled program
/// had its double, +∞ (seed 2026), and to the compiled program
/// 1/nCr(x², x) was +∞ for 0, and ln nCr(x², x) −∞. Its side is proven
/// now (`functions::far_side`). nPr(x², x), x! and x!! there had theirs
/// right, and keep them.
#[test]
fn a_count_far_past_the_range_has_its_side() {
    use graphing::functions::count_beyond;
    let above = |v: Option<(f64, f64)>| matches!(v, Some((m, e)) if m > 0.0 && e == f64::INFINITY);
    // C(n, r) and P(n, r) for whole 1 ≤ r < n are whole numbers of at least
    // n: never below the range. With r and n − r past 10¹⁵ and n past
    // 10⁶⁰, ln C(n, r) > r·ln(n/r) is past 2⁴⁷, beyond the extended range.
    for a in [61, 100, 150, 200, 250, 300, 308] {
        let n = 10f64.powi(a);
        for b in [15, 30, a / 2, a - 15, a - 1] {
            let k = 10f64.powi(b);
            for r in [k, n - k] {
                if n - r < 1e15 {
                    continue;
                }
                for perm in [false, true] {
                    let v = count_beyond(n, r, perm);
                    assert!(above(v), "{n:e}, {r:e} ({perm}): {v:?}");
                }
            }
        }
    }
    // Beyond the range above: a size, not a value, to the reference; its
    // reciprocal 0. Its logarithm (about 2.3·10¹⁰² for C(10²⁰⁰, 10¹⁰⁰)) is
    // a double, which a size can't give: "can't tell" (review 20: it was
    // +∞, and ln of e^(1.3·10³⁰⁸) +∞ for 1.3·10³⁰⁸).
    fn huge(r: R) -> bool {
        matches!(r, R::Unknown) || matches!(r, R::V(v) if v.m > 0.0 && v.e > 1024)
    }
    fn tiny(r: R) -> bool {
        matches!(r, R::Unknown) || matches!(r, R::V(v) if v.m > 0.0 && v.f() == 0.0)
    }
    for x in [1e70, 1e100, 1e150] {
        for f in ["nCr(x^2,x)", "nPr(x^2,x)", "x!", "x!!"] {
            let checks = [
                (f.to_string(), f64::INFINITY, huge as fn(R) -> bool),
                (format!("1/{f}"), 0.0, tiny),
                (format!("{f}*{f}"), f64::INFINITY, huge),
            ];
            for (g, want, side) in checks {
                let v = scalar(&g, x);
                assert_eq!(v, want, "{g} at {x:e}");
                assert!(
                    side(reference(&g, x)),
                    "{g} at {x:e}: {:?}",
                    reference(&g, x)
                );
                let e = enclosure(&g, x, x);
                assert!(e.lo() <= v && v <= e.hi(), "{g} at {x:e}: {e:?}");
            }
            if f.starts_with('n') {
                let g = format!("ln({f})");
                let p = Program::compile_typed(&ast(&g), &CompileOptions::default()).unwrap();
                assert!(
                    p.eval(x, 0.0).is_nan() && p.undecided(x, 0.0),
                    "{g} at {x:e}"
                );
                assert!(!matches!(reference(&g, x), R::V(v) if v.f() < 1e100 || v.m < 0.0));
            }
            // Two such sizes: their quotient (1) can't be told.
            let g = format!("{f}/{f}");
            assert!(scalar(&g, x).is_nan(), "{g} at {x:e}");
            assert!(matches!(reference(&g, x), R::Unknown), "{g} at {x:e}");
        }
    }
    // And below it: P(½, −10³⁰⁰) = Γ(3/2)/Γ(10³⁰⁰ + 3/2).
    let (f, x) = ("nPr(0.5,-x)", 1e300);
    assert_eq!(count_beyond(0.5, -x, true), Some((1.0, f64::NEG_INFINITY)));
    assert_eq!(scalar(f, x), 0.0);
    assert!(tiny(reference(f, x)), "{:?}", reference(f, x));
    assert_eq!(scalar(&format!("1/{f}"), x), f64::INFINITY);
}

/// Review 20 (R20-M-01's family) against MPFR. A value beyond the doubles
/// of which only its sign and side are held (lost past 2⁵³ binades, or
/// beyond even the extended range) decides nothing else: each of these
/// was decided, at a value MPFR contradicts (from the doubles the literals
/// are; in logarithms, where the value is past MPFR's own range). Now each
/// is "can't tell", or MPFR's value; what the side does decide stays.
#[cfg(feature = "mpfr-oracle")]
#[test]
fn far_values_decide_nothing_mpfr_contradicts() {
    use rug::ops::Pow;
    use rug::{Float, Integer, Rational};
    const P: u32 = 4000;
    let ten = |k: u32| Integer::from(10).pow(k);
    // A typed literal: its exact value rounded once to a double.
    let lit = |r: Rational| Float::with_val(P, Float::with_val(53, r));
    let two = |k: i32| Float::with_val(P, Float::i_exp(1, k));
    // e^l for a logarithm l, as the double nearest it (0 or ±∞ past them).
    let exp_of = |l: &Float| -> f64 {
        if *l > 710 {
            f64::INFINITY
        } else if *l < -746 {
            0.0
        } else {
            Float::with_val(P, l.exp_ref()).to_f64()
        }
    };
    let sign = |v: f64| {
        if v > 0.0 {
            1.0
        } else if v < 0.0 {
            -1.0
        } else {
            0.0
        }
    };
    let a = lit(Rational::from(Integer::from(7801888238375471u64) << 944));
    let b = lit(Rational::from(Integer::from(6783390357013892u64) << 944));
    let c = lit(Rational::from(Integer::from(7292639297694682u64) << 945));
    let abc = Float::with_val(P, &a + &b) - &c;
    let big = lit(Rational::from(Integer::from(13) * ten(307)));
    let seven = lit(Rational::from(Integer::from(7) * ten(307)));
    // (2^1001)! = Γ(2^1001 + 1), less 2^1011: its logarithm.
    let mut lg = Float::with_val(P, two(1001) + 1u32);
    lg.ln_gamma_mut();
    let lg = lg - two(1011);
    let base = lit(Rational::from((10000001, 10000000)));
    let t309 = lit(Rational::from(ten(309)));
    let small = lit(Rational::from((1, ten(306))));
    // ln of e^1000 as the program holds it (rounded once), rounded once.
    let e1000 = Float::with_val(53, Float::with_val(P, 1000u32).exp_ref());
    let ln_e1000 = Float::with_val(53, Float::with_val(P, e1000.ln_ref()));
    let power = Float::with_val(P, base.ln_ref()) * &t309 - lit(Rational::from(ten(305)));
    let root = Float::with_val(P, &big / 2u32) - &seven;
    let e130 = Float::with_val(P, &big * &small)
        - Float::with_val(P, lit(Rational::from(ten(60))).ln_ref());
    // (5·10¹²)! = Γ(5·10¹² + 1), less e^(2·10¹⁴): its logarithm's side.
    let mut fact = Float::with_val(P, 5_000_000_000_001u64);
    fact.ln_gamma_mut();
    let steps = Float::with_val(P, &fact - 144_000_000_000_000u64);
    let fact = fact - 200_000_000_000_000u64;
    let pi2 = std::f64::consts::FRAC_PI_2;
    let cases: Vec<(&str, f64, f64)> = vec![
        (
            "sign(exp(7801888238375471*2^944)*exp(6783390357013892*2^944)-exp(7292639297694682*2^945))",
            sign(abc.to_f64()),
            0.0,
        ),
        (
            "atan(exp(7801888238375471*2^944)*exp(6783390357013892*2^944)/exp(7292639297694682*2^945))",
            exp_of(&abc).atan(),
            0.0,
        ),
        (
            "atan(ln(ln(exp(exp(1000))))-1000)",
            (ln_e1000.to_f64() - 1000.0).atan(),
            1e-12,
        ),
        ("atan(ln(exp(1.3*10^308))-1.3*10^308)", 0.0, 1e-12),
        ("atan((2^1001)!*exp(-2^1011))", exp_of(&lg).atan(), 0.0),
        (
            "sign((2^1001)!*exp(-2^1011)-1)",
            sign(exp_of(&lg) - 1.0),
            0.0,
        ),
        (
            "atan(1.0000001^(10^309)*exp(-10^305))",
            exp_of(&power).atan(),
            0.0,
        ),
        (
            "atan(sqrt(exp(1.3*10^308))/exp(0.7*10^308))",
            exp_of(&root).atan(),
            0.0,
        ),
        (
            "sign(exp(1.3*10^308)^(10^-306)-10^60)",
            sign(e130.to_f64()),
            0.0,
        ),
        ("sign((5*10^12)!-exp(2*10^14))", sign(fact.to_f64()), 0.0),
        ("atan((5*10^12)!/exp(2*10^14))", exp_of(&fact).atan(), 0.0),
        (
            "sign((5*10^12)!*exp(-4.8*10^13)*exp(-4.8*10^13)*exp(-4.8*10^13)-1)",
            sign(steps.to_f64()),
            0.0,
        ),
        ("sign(exp(10^300)-exp(10^10))", 1.0, 0.0),
        ("sign(exp(-10^300)-exp(-10^10))", -1.0, 0.0),
        ("atan(exp(10^300)-exp(10^10))", pi2, 0.0),
    ];
    let mut decided = 0;
    for (src, want, tol) in cases {
        let p = Program::compile_typed(&ast(src), &CompileOptions::default()).unwrap();
        if p.undecided(0.0, 0.0) {
            continue;
        }
        decided += 1;
        let v = p.eval(0.0, 0.0);
        assert!(
            (v - want).abs() <= tol,
            "{src}: {v} decided, and MPFR has {want}"
        );
    }
    // The controls are decided; main's answers were each the other sign,
    // or π/2 for each atan at 0.
    assert_eq!(decided, 3);
    assert!(abc < 0 && lg < 0 && power < 0 && root < 0 && e130 < 0 && fact < 0 && steps < 0);
}
