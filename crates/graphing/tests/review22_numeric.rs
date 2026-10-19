//! Review 22: a far exponent must not be formed by cancelling a rounded
//! log2(m) against -1 when a power's base is just below 1; a power within
//! reach of such a base is its value; and a cube root next to 2⁵³ binades
//! below 1 keeps a whole exponent (`tests/far_invariant.rs` found it).
use graphing::compile::{CompileOptions, Input, Program};
use graphing::lexer::ParseOptions;
use graphing::plot::{PlotOptions, plot};
use graphing::{Equation, Viewport};

fn program(src: &str, digits: Option<u8>) -> Program {
    let eq = Equation::parse_with(
        &format!("y={src}"),
        ParseOptions {
            literal_digits: digits,
            decimal_comma: false,
        },
    )
    .unwrap_or_else(|e| panic!("{src}: {e:?}"));
    Program::compile_typed(eq.explicit().unwrap().1, &CompileOptions::default()).unwrap()
}

/// Decided, within `tol` of `want`, with literals off and at 14 digits.
fn decided(src: &str, want: f64, tol: f64) {
    decided_at(src, want, tol, &[None, Some(14)]);
}

fn decided_at(src: &str, want: f64, tol: f64, settings: &[Option<u8>]) {
    for &digits in settings {
        let p = program(src, digits);
        let v = p.eval(0.0, 0.0);
        assert!((v - want).abs() <= tol, "{src}: {v} for {want}");
        assert!(!p.undecided(0.0, 0.0), "{src}: decided");
    }
}

#[test]
fn a_far_power_just_below_one_keeps_its_logarithm() {
    // Each base 1 - 2^-k is exactly a double, cos(0) exactly 1. The
    // powers' true natural logarithms are about -6.243e15, above the
    // comparison's -6.2432e15: every function is +1. Rounded log2(m) - 1
    // gave a far exponent too large in magnitude, so these drew -1.
    // A correct exponent lands back within reach here: Unknown, not a
    // mantissa recovered from the earlier lost value.
    for (k, scale) in [
        (44, "1.0982801747529*10^-271"),
        (46, "4.3931206990115*10^-271"),
        (47, "8.7862413980231*10^-271"),
        (50, "7.0289931184185*10^-270"),
    ] {
        let src = format!("y=sign(((cos(0)-2^-{k})^(10^300))^({scale})-exp(-6.2432*10^15))");
        for digits in [None, Some(14)] {
            let eq = Equation::parse_with(
                &src,
                ParseOptions {
                    literal_digits: digits,
                    decimal_comma: false,
                },
            )
            .unwrap();
            let opts = CompileOptions::default();
            let p = Program::compile_typed(eq.explicit().unwrap().1, &opts).unwrap();
            assert!(p.eval(0.0, 0.0).is_nan(), "{src}: {}", p.eval(0.0, 0.0));
            assert!(p.undecided(0.0, 0.0), "{src}");
            let mut batch = [0.0; 2];
            p.eval_batch(Input::Slice(&[0.0, 1.0]), Input::Scalar(0.0), &mut batch);
            assert!(batch.iter().all(|v| v.is_nan()), "{src}: {batch:?}");
            let drawn = plot(
                &eq.compile(&opts).unwrap(),
                &Viewport::default_for_size(760.0, 500.0),
                &PlotOptions::default(),
            );
            assert!(drawn.curves.is_empty(), "{src}: false curve");
            assert!(drawn.has_missing_data, "{src}");
        }
    }
}

#[test]
fn ordinary_powers_below_one_keep_their_values() {
    for src in [
        "((cos(0)-0.5)^1000)/(2^-1000)",
        "((cos(0)-0.25)^1000)/(0.75^1000)",
        "sign((cos(0)-2^-44)^(10^300))",
    ] {
        decided(src, 1.0, 1e-12);
    }
}

/// A power of a base just below 1 whose value is within reach is that
/// value, from the base's logarithm: (1 − 2⁻⁵²)^(2⁷⁰) is
/// e^(−262144 − 2⁻³⁵ − …), though t·e alone is −2⁷⁰ binades (that made it
/// "can't tell"); t·e + t·log₂|m| in doubles is up to 2¹⁶ binades off.
#[test]
fn a_power_just_below_one_within_reach_is_its_value() {
    decided(
        "(cos(0)-2^-52)^(2^70)/exp(-262144)",
        1.0 - 2f64.powi(-35),
        1e-14,
    );
    decided(
        "(cos(0)-2^-44)^(2^60)/exp(-65536)",
        1.0 - 2f64.powi(-29),
        1e-13,
    );
    // Just above 1 too (e is 0 there, nothing cancelled).
    decided(
        "(cos(0)+2^-52)^(2^70)/exp(262144)",
        1.0 - 2f64.powi(-35),
        1e-14,
    );
}

/// A cube root of a value 2⁵³ − 1 binades below 1 keeps a whole exponent.
/// e^(−6243314768165358) is about 2^(−(2⁵³ − 1)), and its cube root
/// e^(−2081104922721786) exactly; but e − r, the exponent less its
/// remainder by 3, is no double there, and rounding it gave the root an
/// exponent with a half in it: √2 times itself, drawn as 2/1024 for
/// 1/1024 below, and a logarithm a quarter off. (Literals off: at 14
/// digits these 16-digit exponents are other numbers.)
#[test]
fn a_cube_root_next_to_2_to_the_53_binades_keeps_a_whole_exponent() {
    let decided = |src: &str, want: f64, tol: f64| decided_at(src, want, tol, &[None]);
    decided(
        "cbrt(exp(-6243314768165358))/exp(-2081104922721786)/1024",
        1.0 / 1024.0,
        1e-18,
    );
    decided(
        "ln(cbrt(exp(-6243314768165358)))+2081104922721786",
        0.0,
        0.0,
    );
    decided(
        "cbrt(-exp(-6243314768165358))/exp(-2081104922721786)/1024",
        -1.0 / 1024.0,
        1e-18,
    );
    // The square root and a root through the power were whole already.
    decided(
        "sqrt(exp(-6243314768165358))/exp(-3121657384082679)/1024",
        1.0 / 1024.0,
        1e-18,
    );
    decided(
        "root(exp(-6243314768165358),3)/exp(-2081104922721786)/1024",
        1.0 / 1024.0,
        1e-15,
    );
}
