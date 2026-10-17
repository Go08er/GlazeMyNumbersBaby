//! Review 19: what the plotter draws where f's point value and f's
//! enclosure don't settle it between them.
//!
//! * A vertex is never outside f's enclosure at its x. Where the point
//!   evaluator has no number (or one outside the enclosure) and the
//!   enclosure is no wider than the drawing tolerance, the enclosure
//!   itself places the pixel, and the vertex is drawn from it; a point
//!   with no number is drawn so only where f is proven defined
//!   (`plot::drawn_value`, review 12).
//! * A step of something free of x is one number along the whole curve:
//!   continuous, whichever side of a jump its enclosure can't place it on
//!   (`interval::Ctx::steady_constants`). R19-M-04's missing curve was
//!   this: 5 + ⌈1.00000000000000000001⌉ had 6 for its point value, inside
//!   the enclosure [6, 7], and no curve because nothing proved a constant
//!   continuous.

use graphing::compile::{CompileOptions, Program};
use graphing::interval::{Ctx, Interval, enclose};
use graphing::lexer::ParseOptions;
use graphing::plot::{Plot, PlotOptions, plot};
use graphing::{Equation, TrigUnit, Viewport};

/// `y = text` (numbers to 14 digits, radians): its plot in `vp`, its
/// program, and its enclosure at a point on the tree as typed.
struct Drawn {
    plot: Plot,
    program: Program,
    eq: Equation,
}

fn drawn(text: &str, vp: &Viewport) -> Drawn {
    let parse = ParseOptions {
        decimal_comma: false,
        literal_digits: Some(14),
    };
    let eq = Equation::parse_with(&format!("y={text}"), parse).expect("parses");
    let opts = CompileOptions {
        trig_unit: TrigUnit::Radians,
        variables: &(),
    };
    let compiled = eq.compile(&opts).expect("compiles");
    let program =
        Program::compile_typed(eq.explicit().expect("y = f(x)").1, &opts).expect("compiles");
    Drawn {
        plot: plot(&compiled, vp, &PlotOptions::default()),
        program,
        eq,
    }
}

impl Drawn {
    fn enclosure(&self, x: f64) -> (f64, f64) {
        let opts = CompileOptions {
            trig_unit: TrigUnit::Radians,
            variables: &(),
        };
        let e = enclose(
            self.eq.explicit().unwrap().1,
            Interval::point(x),
            &Ctx::new(opts),
        );
        assert!(!e.is_empty(), "{x}");
        (e.lo(), e.hi())
    }
}

fn reset_view() -> Viewport {
    Viewport::default_for_size(760.0, 500.0)
}

/// One curve across the whole view, nothing missing, every vertex inside
/// f's enclosure at its x (to a rounding).
#[track_caller]
fn one_whole_curve(text: &str, vp: &Viewport) -> Drawn {
    let d = drawn(text, vp);
    assert!(!d.plot.has_missing_data, "{text}: missing data");
    assert_eq!(d.plot.curves.len(), 1, "{text}: pieces");
    let line = &d.plot.curves[0];
    let px = vp.x_per_px();
    assert!(
        line[0].x <= vp.x_min + px && line[line.len() - 1].x >= vp.x_max - px,
        "{text}: drawn from {} to {}",
        line[0].x,
        line[line.len() - 1].x
    );
    for q in line {
        if q.y <= vp.y_min || q.y >= vp.y_max {
            continue;
        }
        let (lo, hi) = d.enclosure(q.x);
        let slack = 1e-12 * (1.0 + q.y.abs());
        assert!(
            lo - slack <= q.y && q.y <= hi + slack,
            "{text}: ({}, {}) drawn outside [{lo}, {hi}]",
            q.x,
            q.y
        );
    }
    d
}

/// A step of something free of x is constant along the curve, so the
/// curve is whole, at the value the program computes, whichever side of a
/// jump the enclosure can't place the argument on. Each of these had no
/// curve at all (a constant), or one stroke a pixel column (with x beside
/// it), and "missing data".
#[test]
fn a_step_of_a_constant_the_enclosure_cannot_place_is_still_one_curve() {
    let vp = reset_view();
    for (text, value) in [
        // ln e encloses 1 either side: its ceiling is 1 or 2.
        ("ceil(ln(e))", 1.0),
        // √2² is 2 either side: its floor is 1 or 2.
        ("floor(sqrt(2)^2)", 2.0),
        ("round(sqrt(2)^2/4)", 1.0),
    ] {
        let d = one_whole_curve(text, &vp);
        let c = d.program.eval(0.0, 0.0);
        assert_eq!(c, value, "{text}");
        // (The enclosure really doesn't say.)
        let (lo, hi) = d.enclosure(0.0);
        assert!(hi - lo >= 1.0, "{text}: [{lo}, {hi}]");
        assert!(d.plot.curves[0].iter().all(|q| q.y == c), "{text}");
    }
    // With x beside it: the line, in one piece.
    for (text, constant) in [
        ("x+floor(sqrt(2)^2)", "floor(sqrt(2)^2)"),
        ("x*sign(sin(pi))", "sign(sin(pi))"),
        ("sin(x)+floor(e^(ln(5)))", "floor(e^(ln(5)))"),
        ("x+mod(pi,pi/3)", "mod(pi,pi/3)"),
    ] {
        let d = one_whole_curve(text, &vp);
        let c = drawn(constant, &vp).program.eval(0.0, 0.0);
        assert!(c.is_finite(), "{constant}");
        for q in &d.plot.curves[0] {
            let want = d.program.eval(q.x, 0.0);
            assert!(
                (q.y - want).abs() <= 1e-9 * (1.0 + want.abs()),
                "{text}: ({}, {}), the program's value is {want}",
                q.x,
                q.y
            );
        }
    }
}

/// Only a step of something free of x: a step that moves with x still
/// breaks at its jumps.
#[test]
fn a_step_with_x_in_it_still_breaks_at_its_jumps() {
    let vp = reset_view();
    for text in [
        "floor(x)",
        "x-floor(sqrt(x^2))",
        "sign(sin(x))",
        "mod(x,pi)",
    ] {
        let d = drawn(text, &vp);
        assert!(
            d.plot.curves.len() >= 9,
            "{text}: {} pieces",
            d.plot.curves.len()
        );
        for line in &d.plot.curves {
            for w in line.windows(2) {
                // No chord climbs a jump: within a piece the curve moves
                // by less than half its jump from one vertex to the next.
                assert!(
                    (w[1].y - w[0].y).abs() < 0.5 || (w[1].x - w[0].x).abs() > 0.2,
                    "{text}: a chord from ({}, {}) to ({}, {})",
                    w[0].x,
                    w[0].y,
                    w[1].x,
                    w[1].y
                );
            }
        }
    }
}

/// Where the point evaluator has no number and f's enclosure is within the
/// drawing tolerance, on a box where f is proven defined, the enclosure
/// places the vertex: the curve is whole.
#[test]
fn a_vertex_is_drawn_from_an_enclosure_within_the_tolerance() {
    // Too long to fold exactly, so the program's constant is unknown
    // (never what rounding step by step would make): no number at any x.
    // The interval form has it: 1.00076… and its floor, 1.
    let vp = reset_view();
    for (text, shift) in [
        ("x+floor((1+2^(-20))^800)", 1.0),
        ("x+(1+2^(-20))^800-(1+2^(-20))^800", 0.0),
    ] {
        let d = one_whole_curve(text, &vp);
        assert!(d.program.eval(1.0, 0.0).is_nan(), "{text}");
        for q in &d.plot.curves[0] {
            assert!(
                (q.y - q.x - shift).abs() <= 1e-9,
                "{text}: ({}, {})",
                q.x,
                q.y
            );
        }
    }
    // A power far below every double, where the point evaluator gives up
    // in a band of x (a known gap of its extended range, about 0.02 here:
    // `wide::pow_mag`): 0 to the enclosure, [0, 5·10⁻³²⁴], and drawn
    // there, whatever the point evaluator says.
    let vp = Viewport::new(-0.1, 0.1, -1.0, 1.0, 760.0, 500.0);
    let d = drawn("x^(9007199254740993/6)", &vp);
    assert!(!d.plot.has_missing_data);
    assert_eq!(d.plot.curves.len(), 1);
    let line = &d.plot.curves[0];
    assert!(line.len() >= 600, "{} points", line.len());
    assert!(line[0].x <= -0.099 && line[line.len() - 1].x >= 0.099);
    assert!(line.iter().all(|q| q.y.abs() <= 1e-300));
}

/// A point with no number is drawn from its enclosure only where f is
/// proven defined: an enclosure of f where it is defined says nothing of
/// whether it is.
#[test]
fn nothing_is_drawn_from_an_enclosure_that_does_not_prove_f_defined() {
    // x^⌊√(−10⁻³⁰)⌋ is defined nowhere; where it is "defined" it would be
    // x⁰ = 1, and 1 is all its enclosure holds.
    let vp = reset_view();
    let d = drawn("x^floor(sqrt(-10^(-30)))", &vp);
    assert!(d.program.eval(2.0, 0.0).is_nan());
    assert!(d.plot.curves.is_empty(), "{} pieces", d.plot.curves.len());
    assert!(d.plot.holes.is_empty());
}

/// A column with more breaks than are located is drawn as a stroke, and
/// the column right after it, in its run, gets a short allowance. Only
/// that one: root(x, 7 + ln(4 − x²)) + x has such columns where its degree
/// runs through the whole numbers, beside −2, and its curve starts at 0,
/// fifty pixels on, rising from 0 with a vertical tangent. The start was
/// a stroke down its first column from 0.49 up, the rise below it gone.
#[test]
fn a_dense_column_far_off_does_not_cut_a_curve_s_start_short() {
    let vp = reset_view();
    let d = drawn("root(x,7+ln(4-x^2))+x", &vp);
    let px = vp.x_per_px();
    // The piece that starts at the domain's end.
    let piece = d
        .plot
        .curves
        .iter()
        .find(|line| line[0].x >= 0.0 && line[0].x < px)
        .expect("a piece starting in the first column right of 0");
    // Within a hundredth of a pixel of 0, low on the rise (f is 0.17
    // there; the stroke began at 0.49), and on through the middle of the
    // domain in one piece.
    assert!(piece[0].x < 0.01 * px, "starts at {}", piece[0].x);
    assert!(piece[0].y < 0.2, "starts at height {}", piece[0].y);
    assert!(
        piece[piece.len() - 1].x > 1.9,
        "ends at {}",
        piece[piece.len() - 1].x
    );
    for q in piece.iter().filter(|q| q.y > vp.y_min && q.y < vp.y_max) {
        let (lo, hi) = d.enclosure(q.x);
        assert!(lo - 1e-12 <= q.y && q.y <= hi + 1e-12, "({}, {})", q.x, q.y);
    }
}
