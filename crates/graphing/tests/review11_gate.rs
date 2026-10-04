//! Round 11: what the analysis shows is right or says it can't tell. The
//! reproducers of R11-M-03 and R11-M-04 and of the extended independent
//! oracle: narrow peaks the gate samples but used to discard, troughs far
//! below the doubles between two peaks, holes claimed where a positive
//! square is 0, and values only the compiled program has (a constant folded
//! from an exponent whose fraction was lost). Checked as "correct or
//! unknown"; the last test is the control that ordinary answers stay.

use graphing::analysis::{
    Interval, KeyGraphFeatures, analyze, analyze_str, analyze_ungated, flags, verify,
};
use graphing::compile::CompileOptions;
use graphing::equation::Equation;
use graphing::functions::TrigUnit;

fn k(src: &str) -> KeyGraphFeatures {
    let r = analyze_str(src);
    assert!(r.analysis_error_string().is_none(), "{src}: refused");
    r
}

/// The feature is not claimed complete: unknown, or a partial list (its
/// items proven, maybe not all of them).
fn unknown(r: &KeyGraphFeatures, flag: u32) -> bool {
    (r.too_complex_features | r.partial_features) & flag != 0
}

/// A maximum (or minimum, `maxima` false) near `x` with value near `y`.
fn has_turn(r: &KeyGraphFeatures, maxima: bool, x: f64, y: f64) -> bool {
    let list = if maxima {
        &r.data.maxima
    } else {
        &r.data.minima
    };
    list.iter().any(|(f, v)| {
        (f.x - x).abs() <= 1e-3 * x.abs().max(1.0) && (v - y).abs() <= 1e-6 * y.abs().max(1.0)
    })
}

/// R11-M-03: a peak at a far centre, much narrower than the gate's samples
/// around it, which the gate samples (f = 2 there) but used to discard as
/// the edge of a jump; and the trough between it and the peak at 0.
#[test]
fn narrow_far_peaks_are_reported_or_unknown() {
    for centre in ["1234", "1522756", "1234*1234"] {
        let c: f64 = if centre == "1234*1234" {
            1522756.0
        } else {
            centre.parse().unwrap()
        };
        for w in ["0.001", "0.000001", "0.000000001", "0.000000000001"] {
            let src = format!("y=2*exp(-(x-{centre})^2/{w})+exp(-x^2)");
            let r = k(&src);
            assert!(
                unknown(&r, flags::MAXIMA)
                    || (has_turn(&r, true, c, 2.0) && has_turn(&r, true, 0.0, 1.0)),
                "{src}: maxima {:?}",
                r.maxima
            );
            // Between the peaks f has a (tiny, positive) minimum.
            assert!(
                unknown(&r, flags::MINIMA)
                    || r.data.minima.iter().any(|(f, _)| f.x > 0.0 && f.x < c),
                "{src}: minima {:?}",
                r.minima
            );
        }
    }
    for src in [
        "y=exp(-(x-1522756)^2/0.000001)+exp(-x^2)",
        "y=2*exp(-(x-1522756)^2/0.00000001)+exp(-x^2)",
        "y=2/(1+(x-1234)^2/0.000000000001)+1/(1+x^2)",
    ] {
        let r = k(src);
        assert!(
            unknown(&r, flags::MAXIMA) || r.data.maxima.len() >= 2,
            "{src}: maxima {:?}",
            r.maxima
        );
    }
    // A peak and a trough: "no maxima" was false.
    let src = "y=exp(-(x-10^7)^2)-exp(-x^2)";
    let r = k(src);
    assert!(
        unknown(&r, flags::MAXIMA) || has_turn(&r, true, 1e7, 1.0),
        "{src}: maxima {:?}",
        r.maxima
    );
    // Mixed frames: the minimum near 10⁶ (f ≈ 1.1·10⁻⁶), not "no minima".
    let src = "y=(x-1000000)^2+0.0000001+(x/1000000000)^2";
    let r = k(src);
    assert!(
        unknown(&r, flags::MINIMA) || r.data.minima.iter().any(|(f, _)| (f.x - 1e6).abs() < 1.0),
        "{src}: minima {:?}",
        r.minima
    );
}

/// R11-M-04: a zero base under a positive power is no hole. The function
/// is defined (1, or 2) at the centre; a claimed exclusion there is false.
#[test]
fn a_positive_power_of_zero_is_no_hole() {
    for src in [
        "y=1/(1+(x-1522756)^2/0.000000000001)+exp(-x^2)",
        "y=1/(1+(x-1234*1234)^2/0.000000000001)+exp(-x^2)",
        "y=2/(1+(x-1522756)^2/0.000000000001)+1/(1+x^2)",
        "y=2/(1+(x-1234*1234)^2/0.000000000001)+1/(1+x^2)",
        "y=1/(1+(x-1522756)^2/0.000000000001)",
        "y=1/(1+(x-1234)^2/0.000000000001)+exp(-x^2)",
    ] {
        let r = k(src);
        assert!(
            unknown(&r, flags::DOMAIN) || r.domain == "x ∈ ℝ",
            "{src}: domain {}",
            r.domain
        );
        assert!(
            r.data
                .excluded
                .iter()
                .all(|f| (f.x - 1522756.0).abs() > 1.0 && (f.x - 1234.0).abs() > 1.0),
            "{src}: excluded {:?}",
            r.data.excluded
        );
    }
}

/// A "none" found by scanning where f was said to be defined goes with the
/// domain: (x − 2⁴⁰)/(x − 2⁴⁰ − 1/16) was claimed undefined at 2⁴⁰, where
/// it is exactly 0, and said it had no x-intercepts.
#[test]
fn none_answers_go_with_the_domain() {
    for src in [
        "y=(x-2^40)/(x-2^40-1/16)",
        "y=(x-1000000000000)/(x-1000000000000.0625)",
    ] {
        let r = k(src);
        assert!(
            unknown(&r, flags::ZEROS) || !r.x_intercept.is_empty(),
            "{src}: x-intercepts \"{}\"",
            r.x_intercept
        );
        assert!(
            unknown(&r, flags::VERTICAL_ASYMPTOTES) || !r.vertical_asymptotes.is_empty(),
            "{src}: vertical asymptotes {:?}",
            r.vertical_asymptotes
        );
    }
}

/// Q1: a value only the compiled program has is no truth. ln(e^(10¹⁵)⁴)
/// − 4·10¹⁵ is 0 everywhere; the compiler folds it to 1/2 (the exponent's
/// fraction is lost), and the reference can't say. Nothing it shows may
/// rest on that 1/2.
#[test]
fn a_constant_only_the_compiler_has_is_no_answer() {
    let r = analyze_str("y=ln(exp(1000000000000000)^4)-4000000000000000");
    if r.analysis_error_string().is_some() {
        return;
    }
    assert!(!r.range.contains("1/2"), "range {}", r.range);
    assert!(
        !r.y_intercept.contains("1/2"),
        "y-intercept {}",
        r.y_intercept
    );
    assert!(
        unknown(&r, flags::ZEROS) || !r.x_intercept.is_empty(),
        "every x is an intercept, not none"
    );
}

fn in_unit(src: &str, unit: TrigUnit) -> KeyGraphFeatures {
    analyze(
        &Equation::parse(src).unwrap(),
        &CompileOptions {
            trig_unit: unit,
            variables: &(),
        },
    )
}

/// Excludes something, or says it can't tell.
fn excludes_or_unknown(r: &KeyGraphFeatures) -> bool {
    unknown(r, flags::DOMAIN) || r.domain.contains('\\')
}

/// Removable trig holes: tan x · cos x is sin x except at π/2 + kπ, where
/// tan has its poles. In radians no double is such a point, so f is finite
/// at every double and sampling never sees the hole: the domain must still
/// exclude it (or say it can't tell), and nothing may be claimed there.
#[test]
fn removable_trig_holes_are_excluded_or_unknown() {
    let r = k("y=tan(x)*cos(x)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {π/2 + kπ | k ∈ ℤ}");
    assert!(r.vertical_asymptotes.is_empty() && !unknown(&r, flags::VERTICAL_ASYMPTOTES));
    // sin x's turns at π/2 + kπ are holes: no extremum there, and the
    // range (−1, 1) is not [−1, 1].
    assert!(
        r.maxima.is_empty() || unknown(&r, flags::MAXIMA),
        "{:?}",
        r.maxima
    );
    assert!(r.range != "y ∈ [−1, 1]", "{}", r.range);
    for src in [
        "y=tan(2x+1)*cos(2x+1)",
        "y=sec(x)*cos(x)",
        "y=csc(x)*sin(x)",
        "y=sin(x)/sin(x)",
        "y=tan(x^2)*cos(x^2)",
        "y=tan(x)*cos(x)+x",
        "y=cot(x)*tan(x)",
    ] {
        let r = analyze_str(src);
        assert!(
            r.analysis_error_string().is_some() || excludes_or_unknown(&r),
            "{src}: domain {}",
            r.domain
        );
    }
    // In degrees and grads the poles are doubles, sampled like any other.
    for unit in [TrigUnit::Degrees, TrigUnit::Grads] {
        let r = in_unit("y=tan(x)*cos(x)", unit);
        assert!(excludes_or_unknown(&r), "{unit:?}: {}", r.domain);
    }
    let r = in_unit("y=tan(x)", TrigUnit::Degrees);
    assert_eq!(r.domain, "x ∈ ℝ \\ {90 + 180k | k ∈ ℤ}");
}

/// The gate on its own: told that tan x · cos x is defined everywhere (what
/// the engine used to say), it finds the cosine under the tangent crossing 0
/// inside the claimed domain, at a point no double is.
#[test]
fn the_gate_finds_singular_points_no_double_hits() {
    let opts = CompileOptions::default();
    for src in ["y=tan(x)*cos(x)", "y=tan(2*x+1)*cos(2*x+1)"] {
        let eq = Equation::parse(src).unwrap();
        let mut raw = analyze_ungated(&eq, &opts);
        raw.too_complex_features &= !flags::DOMAIN;
        raw.data.excluded.clear();
        raw.data.domain = vec![Interval::all()];
        raw.domain = "x ∈ ℝ".into();
        let (_, f) = eq.explicit().unwrap();
        let (k, r, _) = verify::gate_report(raw, f, &opts);
        assert!(
            unknown(&k, flags::DOMAIN),
            "{src}: kept {} ({:?})",
            k.domain,
            r.failures.keys().collect::<Vec<_>>()
        );
    }
}

/// Ordinary answers stay: the new rules leave these as they were.
#[test]
fn ordinary_answers_stay() {
    let r = k("y=x^2");
    assert_eq!(r.minima, ["(0, 0)"]);
    assert_eq!(r.range, "y ∈ [0, ∞)");
    let r = k("y=1/(1+x^2)");
    assert_eq!(r.maxima, ["(0, 1)"]);
    assert_eq!(r.range, "y ∈ (0, 1]");
    assert!(r.minima.is_empty() && !unknown(&r, flags::MINIMA));
    // 1/e exactly: f(1) = 1·e⁻¹ in exact arithmetic.
    let r = k("y=x*exp(-x)");
    assert_eq!(r.maxima, ["(1, 1/e)"]);
    let r = k("y=sec(x)");
    assert_eq!(r.range, "y ∈ (−∞, −1] ∪ [1, ∞)");
    assert_eq!(r.minima, ["(2kπ, 1), k ∈ ℤ"]);
    let r = k("y=(x^2-1)/(x-1)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {1}");
    let r = k("y=sin(x)/x");
    assert_eq!(r.domain, "x ∈ ℝ \\ {0}");
    let r = k("y=1/x");
    assert_eq!(r.vertical_asymptotes, ["x = 0"]);
    assert!(r.maxima.is_empty() && !unknown(&r, flags::MAXIMA));
    let r = k("y=tan(x)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {π/2 + kπ | k ∈ ℤ}");
    assert!(r.minima.is_empty() && !unknown(&r, flags::MINIMA));
    let r = k("y = x + 1/(x - 2000000)");
    assert_eq!(r.maxima, ["(1999999, 1999998)"]);
    assert_eq!(r.minima, ["(2000001, 2000002)"]);
    // A spike 10⁻⁶ wide at 1522756: f′'s sign there is beyond the budget.
    let r = k("y=1/(1+(x-1522756)^2/0.000000000001)");
    assert_eq!(r.domain, "x ∈ ℝ");
    assert!(r.maxima == ["(1522756, 1)"] || unknown(&r, flags::MAXIMA));
}

/// Touched trig holes: cos² x / cos² x is 1 except at π/2 + kπ, where its
/// divisor touches 0 without changing sign. No double is such a point, so
/// neither sampling nor a sign change shows it: the domain must still leave
/// it out (or say it can't tell), and nothing may say "ℝ".
#[test]
fn touched_trig_holes_are_excluded_or_unknown() {
    for (src, domain) in [
        ("y=cos(x)^2/cos(x)^2", "x ∈ ℝ \\ {π/2 + kπ | k ∈ ℤ}"),
        ("y=sin(x)^2/sin(x)^2", "x ∈ ℝ \\ {kπ | k ∈ ℤ}"),
        ("y=sin(x)/sin(x)", "x ∈ ℝ \\ {kπ | k ∈ ℤ}"),
        ("y=cos(x)/cos(x)", "x ∈ ℝ \\ {π/2 + kπ | k ∈ ℤ}"),
        ("y=sec(x)*cos(x)", "x ∈ ℝ \\ {π/2 + kπ | k ∈ ℤ}"),
        ("y=tan(x)/tan(x)", "x ∈ ℝ \\ {kπ/2 | k ∈ ℤ}"),
        ("y=(1+cos(x))/(1+cos(x))", "x ∈ ℝ \\ {π + 2kπ | k ∈ ℤ}"),
    ] {
        let r = k(src);
        assert_eq!(r.domain, domain, "{src}");
        // {1}, or unknown where the certifier can't bound it near the holes.
        assert!(
            r.range == "y ∈ {1}" || (r.range.is_empty() && unknown(&r, flags::RANGE)),
            "{src}: {}",
            r.range
        );
        assert!(r.vertical_asymptotes.is_empty(), "{src}");
        // Not "not periodic": its holes repeat.
        assert!(!r.periodicity_expression.is_empty() || unknown(&r, flags::PERIODICITY));
    }
    // A shifted, stretched argument's holes are its own (or, where the
    // divisor is a power of the cosine and the certifier's table doesn't
    // reach it, the domain is unknown).
    let r = k("y=cos(2x+1)^3/cos(2x+1)^3");
    assert!(
        r.domain == "x ∈ ℝ \\ {0.285398 + kπ/2 | k ∈ ℤ}" || unknown(&r, flags::DOMAIN),
        "{}",
        r.domain
    );
    // 0 everywhere it is defined: every such x is an intercept, and the
    // holes are sin's, not a period sampling saw (it used to say kπ/12).
    // (Or, where the certifier doesn't prove it 0, unknown.)
    let r = k("y=0*sin(x)/sin(x)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {kπ | k ∈ ℤ}");
    assert!(r.x_intercept == r.domain || unknown(&r, flags::ZEROS));
    assert!(!r.periodicity_expression.contains("π/12"));
    // Poles where the divisor touches 0, and ones only a log shows.
    for (src, domain) in [
        ("y=1/cos(x)^2", "x ∈ ℝ \\ {π/2 + kπ | k ∈ ℤ}"),
        ("y=ln(sin(x)^2)", "x ∈ ℝ \\ {kπ | k ∈ ℤ}"),
        ("y=ln(cos(x)^2)", "x ∈ ℝ \\ {π/2 + kπ | k ∈ ℤ}"),
    ] {
        // (A log of a touching power is beyond the certifier's table: its
        // domain may be unknown, never ℝ.)
        let r = k(src);
        assert!(
            r.domain == domain || unknown(&r, flags::DOMAIN),
            "{src}: {}",
            r.domain
        );
        assert!(
            !r.vertical_asymptotes.is_empty() || unknown(&r, flags::VERTICAL_ASYMPTOTES),
            "{src}"
        );
    }
    // Where the holes can't be stated, the domain says it can't tell; it
    // never says ℝ (ln(1 + cos x) used to, with a range bounded below).
    for src in [
        "y=x/cos(x)^2",
        "y=cos(x^2)^2/cos(x^2)^2",
        "y=(1+cos(x^2))/(1+cos(x^2))",
        "y=ln(1+cos(x))",
        "y=(1-sin(x))/(1-sin(x))",
        "y=sin(x)^-2*sin(x)^2",
    ] {
        let r = analyze_str(src);
        assert!(
            r.analysis_error_string().is_some() || excludes_or_unknown(&r),
            "{src}: domain {}",
            r.domain
        );
        assert!(r.domain != "x ∈ ℝ", "{src}");
    }
    // In degrees and grads the holes are doubles, sampled like any other.
    for unit in [TrigUnit::Degrees, TrigUnit::Grads] {
        for src in ["y=cos(x)^2/cos(x)^2", "y=(1+cos(x))/(1+cos(x))"] {
            let r = in_unit(src, unit);
            assert!(
                r.analysis_error_string().is_some() || excludes_or_unknown(&r),
                "{unit:?} {src}: {}",
                r.domain
            );
        }
    }
    let r = in_unit("y=cos(x)^2/cos(x)^2", TrigUnit::Degrees);
    assert_ne!(r.domain, "x ∈ ℝ");
    // The same away from trig: a divisor touching 0 at ±√2, no double.
    for src in [
        "y=(x^2-2)^2/(x^2-2)^2",
        "y=abs(x^2-2)/abs(x^2-2)",
        "y=(x^2-3)^2/(x^2-3)^2",
        "y=x*(x^2-2)^2/(x^2-2)^2",
    ] {
        let r = analyze_str(src);
        assert!(
            r.analysis_error_string().is_some() || excludes_or_unknown(&r),
            "{src}: domain {}",
            r.domain
        );
    }
    // (Poles there were already found.)
    assert_eq!(k("y=1/(x^2-2)^2").domain, "x ∈ ℝ \\ {−√2, √2}");
}

/// The gate on its own: told that cos² x / cos² x and (1 + cos x) / (1 +
/// cos x) are defined everywhere (what the engine used to say), it finds
/// the points no double is and no sign change shows, and drops the domain;
/// and a touched factor of an argument that isn't affine leaves no definite
/// domain either.
#[test]
fn the_gate_finds_touched_singular_points() {
    let opts = CompileOptions::default();
    for src in [
        "y=cos(x)^2/cos(x)^2",
        "y=(1+cos(x))/(1+cos(x))",
        "y=1/cos(x)^2*cos(x)^2",
        "y=cos(x^2)^2/cos(x^2)^2",
    ] {
        let eq = Equation::parse(src).unwrap();
        let mut raw = analyze_ungated(&eq, &opts);
        raw.too_complex_features &= !flags::DOMAIN;
        raw.data.excluded.clear();
        raw.data.period = None;
        raw.data.domain = vec![Interval::all()];
        raw.domain = "x ∈ ℝ".into();
        let (_, f) = eq.explicit().unwrap();
        let (k, r, _) = verify::gate_report(raw, f, &opts);
        assert!(
            unknown(&k, flags::DOMAIN),
            "{src}: kept {} ({:?})",
            k.domain,
            r.failures.keys().collect::<Vec<_>>()
        );
    }
}
