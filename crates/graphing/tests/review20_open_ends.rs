//! A monotone piece or an extremum is listed at an end of the domain only
//! where the claims on f′ reach it (round 20). Beside an open end, or a
//! closed one f isn't differentiable at, the claims on f′ start a double
//! in, and the reals between are a gap: the piece or extremum is placed at
//! the end only across a gap shown clear of f′'s zeros (its `GapClear`
//! claim, with f′'s strict sign beside it), and otherwise the piece starts
//! where the claims do, the row partial, and the extremum isn't listed.

use graphing::analysis::{analyze, flags};
use graphing::certify::{
    Analysis, Bound, Claim, DEFAULT_BUDGET, Dir, Enc, Row, Subject, Tail, certify_text,
};
use graphing::compile::CompileOptions;
use graphing::{Equation, TrigUnit};

const UNITS: [TrigUnit; 3] = [TrigUnit::Radians, TrigUnit::Degrees, TrigUnit::Grads];

fn certify(src: &str, unit: TrigUnit) -> Analysis {
    let opts = CompileOptions {
        trig_unit: unit,
        ..CompileOptions::default()
    };
    certify_text(src, opts, DEFAULT_BUDGET, None).expect("certified")
}

fn f1(s: &Subject) -> bool {
    *s == Subject::f(1)
}

/// The boxes of x where some claim of the row decides f′ (its sign, a
/// zero, a kink's sides) or clears a gap of f′'s zeros.
fn decided(claims: &[Claim]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for c in claims {
        match c {
            Claim::Beyond { x, of, .. }
            | Claim::NoCross { x, of, .. }
            | Claim::OneCross { x, of, .. }
            | Claim::ExactAt { x, of, .. }
            | Claim::Factors { x, of, .. }
            | Claim::Touch { x, of, .. }
            | Claim::GapClear { x, of }
                if f1(of) =>
            {
                out.push((x.a.0, x.b.0))
            }
            Claim::TailBeyond { side, from, of, .. } | Claim::TailChain { side, from, of, .. }
                if f1(of) =>
            {
                out.push(match side {
                    Tail::Left => (f64::NEG_INFINITY, from.0),
                    Tail::Right => (from.0, f64::INFINITY),
                })
            }
            Claim::KinkAt { x, .. } | Claim::KinkSides { x, .. } => out.push((x.a.0, x.b.0)),
            _ => {}
        }
    }
    out
}

/// Each finite end of each monotone piece, and each extremum, lies in a
/// box a claim on f′ decides (an open end at a double: such a box starts
/// or ends at it). What a listing placed where no claim reaches looks like.
fn unreached(a: &Analysis) -> Vec<String> {
    let mut out = Vec::new();
    let holds = |d: &[(f64, f64)], x: Enc| d.iter().any(|&(a, b)| a <= x.lo.0 && x.hi.0 <= b);
    if let Row::Certified { value, cert } | Row::Partial { value, cert } = &a.monotonicity {
        let d = decided(&cert.claims);
        for m in value {
            for b in [m.on.lo, m.on.hi] {
                if let Bound::At { x, .. } = b
                    && !holds(&d, x)
                {
                    out.push(format!("a {:?} piece ends at {x:?}", m.dir));
                }
            }
        }
    }
    if let Row::Certified { value, cert } | Row::Partial { value, cert } = &a.extrema {
        let d = decided(&cert.claims);
        for e in value {
            if !holds(&d, e.x) {
                out.push(format!("an extremum at {:?}", e.x));
            }
        }
    }
    out
}

fn gap_cleared(claims: &[Claim], a: f64, b: f64) -> bool {
    claims
        .iter()
        .any(|c| matches!(c, Claim::GapClear { x, of } if f1(of) && x.a.0 == a && x.b.0 == b))
}

fn at(x: f64, closed: bool) -> Bound {
    Bound::At {
        x: Enc::point(x),
        closed,
    }
}

/// √(x − 1000)·ln(x − 1000) + x falls from its open end 1000 (f′ → −∞
/// there), but f′'s tree is ∞ − ∞ over the gap [1000, 1000 + ulp] and
/// nothing shows it clear: the piece was listed from 1000 on no claim, and
/// the strict replay refused it. It now starts at the first double, the
/// row partial; so do its shifts.
#[test]
fn a_piece_starts_where_its_claims_on_the_slope_do() {
    for (src, end) in [
        ("sqrt(x-1000)*ln(x-1000)+x", 1000.0),
        ("sqrt(x+3)*ln(x+3)+x", -3.0),
        ("sqrt(x-0.5)*ln(x-0.5)+x", 0.5),
    ] {
        for unit in UNITS {
            let a = certify(src, unit);
            let Row::Partial { value, cert } = &a.monotonicity else {
                panic!("{src} [{unit:?}]: {:?}", a.monotonicity);
            };
            let first = f64::next_up(end);
            assert!(!gap_cleared(&cert.claims, end, first), "{src}");
            assert_eq!(value[0].dir, Dir::Decreasing, "{src} [{unit:?}]");
            assert_eq!(value[0].on.lo, at(first, true), "{src} [{unit:?}]");
            assert_eq!(unreached(&a), Vec::<String>::new(), "{src} [{unit:?}]");
        }
    }
    // The panel: a piece from about 1000, some of them.
    let eq = Equation::parse("y=sqrt(x-1000)*ln(x-1000)+x").unwrap();
    let k = analyze(&eq, &CompileOptions::default());
    assert_eq!(k.monotonicity[0].0, "(≈1000, ≈1000.08)");
    assert_ne!(k.partial_features & flags::MONOTONE_INTERVALS, 0);
}

/// atan(ln|x|) in degrees (and grads) had no derivative trees: the
/// differentiator writes π/180 as its double, which no tree may hold. So
/// nothing cleared the gaps beside the excluded 0, and its pieces were
/// listed up to 0 on no claim (the strict replay refused the falling one,
/// [−5·10⁻³²⁴, 0] undecided). With the factor written exactly, the gaps
/// are cleared in every unit, and the rows are complete as in radians.
#[test]
fn atan_of_ln_abs_x_is_complete_in_every_angle_unit() {
    for src in [
        "atan(ln(abs(x)))",
        "atan(ln(abs(x-1000)))",
        "atan(ln(abs(x+3)))",
    ] {
        for unit in UNITS {
            let a = certify(src, unit);
            let Row::Certified { value, cert } = &a.monotonicity else {
                panic!("{src} [{unit:?}]: {:?}", a.monotonicity);
            };
            assert_eq!(value.len(), 2, "{src} [{unit:?}]");
            let Bound::At {
                x: p,
                closed: false,
            } = value[0].on.hi
            else {
                panic!("{src} [{unit:?}]: {value:?}");
            };
            assert!(p.is_point(), "{src} [{unit:?}]");
            let p = p.lo.0;
            assert_eq!(
                (value[0].on.lo, value[0].dir),
                (Bound::NegInf, Dir::Decreasing)
            );
            assert_eq!(
                (value[1].on, value[1].dir),
                (
                    graphing::certify::Piece {
                        lo: at(p, false),
                        hi: Bound::PosInf
                    },
                    Dir::Increasing
                ),
                "{src} [{unit:?}]"
            );
            assert!(
                gap_cleared(&cert.claims, f64::next_down(p), p),
                "{src} [{unit:?}]"
            );
            assert!(
                gap_cleared(&cert.claims, p, f64::next_up(p)),
                "{src} [{unit:?}]"
            );
            assert!(a.extrema.is_certified(), "{src} [{unit:?}]");
            assert!(a.range.is_certified(), "{src} [{unit:?}]");
            assert_eq!(unreached(&a), Vec::<String>::new(), "{src} [{unit:?}]");
            if unit != TrigUnit::Radians {
                let trees = a.evaluated.as_deref().unwrap_or_default();
                assert!(trees.contains("f′ = ") && trees.contains("pi"), "{trees}");
            }
        }
    }
    let eq = Equation::parse("y=atan(ln(abs(x)))").unwrap();
    let opts = CompileOptions {
        trig_unit: TrigUnit::Degrees,
        ..CompileOptions::default()
    };
    let k = analyze(&eq, &opts);
    let pieces: Vec<&str> = k.monotonicity.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(pieces, ["(−∞, 0)", "(0, ∞)"]);
    assert_eq!(k.partial_features & flags::MONOTONE_INTERVALS, 0);
}

/// √(1 − x²) − ∛(1 − x²) falls from −1 and rises to 1 (f′ → ∓∞ there), and
/// is 0 at both: the ends are maxima. But f′'s claims start a double in
/// and its tree is ∞ − ∞ over the gaps, which nothing shows clear: the
/// pieces from ±1 were refused by the strict replay, and the two end
/// maxima were listed on no claim (the replay asked a gap's clearness of
/// a complete row only). Now the pieces start a double in and the end
/// maxima aren't listed; the turns inside still are.
#[test]
fn an_end_extremum_needs_the_gap_beside_it_cleared() {
    let a = certify("sqrt(1-x^2)-(1-x^2)^(1/3)", TrigUnit::Radians);
    let Row::Partial { value, .. } = &a.extrema else {
        panic!("{:?}", a.extrema);
    };
    assert!(
        value.iter().all(|e| e.x.hi.0 < 1.0 && e.x.lo.0 > -1.0),
        "{value:?}"
    );
    assert_eq!(value.len(), 3, "{value:?}");
    let Row::Partial { value, .. } = &a.monotonicity else {
        panic!("{:?}", a.monotonicity);
    };
    assert_eq!(value[0].on.lo, at(f64::next_up(-1.0), true));
    assert_eq!(value[value.len() - 1].on.hi, at(f64::next_down(1.0), true));
    assert_eq!(unreached(&a), Vec::<String>::new());
    // √x's gap beside 0 is cleared (f′ = 1/(2√x) > 0 there): its minimum
    // at the closed end 0 and its piece from 0 stay, complete.
    let a = certify("sqrt(x)", TrigUnit::Radians);
    let Row::Certified { value, .. } = &a.extrema else {
        panic!("{:?}", a.extrema);
    };
    assert_eq!(value[0].x, Enc::point(0.0));
    let Row::Certified { value, .. } = &a.monotonicity else {
        panic!("{:?}", a.monotonicity);
    };
    assert_eq!(value[0].on.lo, at(0.0, true));
    assert_eq!(unreached(&a), Vec::<String>::new());
}

/// A gap clear of f′'s zeros alone doesn't carry a constant piece across
/// (f′ may be 0 at the double and not inside); f′'s own tree exactly 0
/// does: x/x and 0/x keep (−∞, 0) and (0, ∞) constant, complete, and
/// their one-value range.
#[test]
fn a_constant_piece_reaches_across_a_gap_where_the_slope_is_zero_throughout() {
    for src in ["x/x", "0/x", "(x-1000)/(x-1000)", "0/exp(1/x)"] {
        let a = certify(src, TrigUnit::Radians);
        let Row::Certified { value, .. } = &a.monotonicity else {
            panic!("{src}: {:?}", a.monotonicity);
        };
        assert!(value.iter().all(|m| m.dir == Dir::Constant), "{src}");
        assert!(
            value
                .iter()
                .any(|m| matches!(m.on.hi, Bound::At { closed: false, .. })),
            "{src}: {value:?}"
        );
        assert!(a.range.is_certified(), "{src}");
    }
}
