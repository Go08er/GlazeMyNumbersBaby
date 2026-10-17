//! Round 19 (R19-M-01): what a certificate says across a kink.
//!
//! A kink box the certifier cannot read says nothing about f′ or f″ (a
//! valley can lie between two doubles). What it does say, it proves on
//! f's two branches there (`Claim::KinkSides`): these are the functions
//! that keep their rows by that proof, and the ones that must not get a
//! row from anything weaker.

use graphing::certify::{
    Analysis, Bound, Claim, DEFAULT_BUDGET, Dir, ExtKind, KinkSign, Row, certify_text,
};
use graphing::compile::CompileOptions;

fn certify(src: &str) -> Analysis {
    certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None)
        .unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn claims<T>(row: &Row<T>) -> &[Claim] {
    match row {
        Row::Certified { cert, .. } | Row::Partial { cert, .. } => &cert.claims,
        Row::Unknown { .. } => &[],
    }
}

fn certified<'a, T>(src: &str, name: &str, row: &'a Row<T>) -> &'a T {
    match row {
        Row::Certified { value, .. } => value,
        Row::Partial { .. } => panic!("{src}: {name} is partial"),
        Row::Unknown { reason } => panic!("{src}: {name} is unknown ({reason})"),
    }
}

/// |x − c| with c at no double: the kink is the one zero of x − c between
/// two doubles, f is c − x before it and x − c after it, and f′ = ∓1 and
/// f″ = 0 on those branches over the whole box. (After 1e7e006 all four
/// rows were unknown.)
#[test]
fn a_kink_between_two_doubles_keeps_its_rows() {
    for (src, c) in [
        ("abs(x-0.1)", 0.1),
        ("abs(x-1/3)", 1.0 / 3.0),
        ("abs(x-sin(1))", 1f64.sin()),
        ("2*abs(x-0.1)+3", 0.1),
        ("min(x,2/3-x)", 1.0 / 3.0),
    ] {
        let a = certify(src);
        let peak = src.starts_with("min");
        let ext = certified(src, "extrema", &a.extrema);
        assert_eq!(ext.len(), 1, "{src}: {ext:?}");
        let e = &ext[0];
        assert_eq!(
            e.kind,
            if peak { ExtKind::Max } else { ExtKind::Min },
            "{src}"
        );
        assert!(
            e.x.lo.0 < c.next_up() && c.next_down() < e.x.hi.0 && !e.x.is_point(),
            "{src}: the kink is enclosed, at no double: {:?}",
            e.x
        );
        assert!(
            e.x.hi.0 - e.x.lo.0 <= 8.0 * f64::EPSILON,
            "{src}: {:?}",
            e.x
        );
        assert!(
            claims(&a.extrema).iter().any(|k| matches!(
                k,
                Claim::KinkSides {
                    at: None,
                    order: 1,
                    left,
                    right,
                    ..
                } if (*left, *right) == if peak {
                    (KinkSign::Pos, KinkSign::Neg)
                } else {
                    (KinkSign::Neg, KinkSign::Pos)
                }
            )),
            "{src}: the turn is proven on the two branches"
        );
        let inf = certified(src, "inflections", &a.inflections);
        assert!(inf.is_empty(), "{src}: {inf:?}");
        assert!(
            claims(&a.inflections).iter().any(|k| matches!(
                k,
                Claim::KinkSides {
                    order: 2,
                    left: KinkSign::Zero,
                    right: KinkSign::Zero,
                    ..
                }
            )),
            "{src}: f″ = 0 on each branch"
        );
        let mono = certified(src, "monotonicity", &a.monotonicity);
        let dirs: Vec<Dir> = mono.iter().map(|m| m.dir).collect();
        assert_eq!(
            dirs,
            if peak {
                [Dir::Increasing, Dir::Decreasing]
            } else {
                [Dir::Decreasing, Dir::Increasing]
            },
            "{src}"
        );
        assert_eq!(mono[0].on.lo, Bound::NegInf, "{src}");
        assert_eq!(mono[1].on.hi, Bound::PosInf, "{src}");
        let range = certified(src, "range", &a.range);
        assert_eq!(range.len(), 1, "{src}: {range:?}");
    }
}

/// |x| and its like at a double: the minimum at the point, no inflection
/// (f″ = 0 on each side), the two monotone pieces, the range from 0.
#[test]
fn abs_at_a_double_keeps_its_rows() {
    for (src, at, low) in [
        ("abs(x)", 0.0, 0.0),
        ("abs(x-2)", 2.0, 0.0),
        ("3*abs(x+1)-4", -1.0, -4.0),
        ("abs(x/1000)", 0.0, 0.0),
    ] {
        let a = certify(src);
        let ext = certified(src, "extrema", &a.extrema);
        assert_eq!(ext.len(), 1, "{src}: {ext:?}");
        assert!(
            ext[0].kind == ExtKind::Min
                && ext[0].x.is_point()
                && ext[0].x.lo.0 == at
                && ext[0].y.contains(low),
            "{src}: {ext:?}"
        );
        let inf = certified(src, "inflections", &a.inflections);
        assert!(inf.is_empty(), "{src}: {inf:?}");
        let mono = certified(src, "monotonicity", &a.monotonicity);
        assert_eq!(
            mono.iter().map(|m| m.dir).collect::<Vec<_>>(),
            [Dir::Decreasing, Dir::Increasing],
            "{src}"
        );
        let range = certified(src, "range", &a.range);
        assert!(
            matches!(&range[..], [p] if p.hi == Bound::PosInf
                && matches!(&p.lo, Bound::At { x, closed: true } if x.contains(low))),
            "{src}: {range:?}"
        );
    }
}

/// x·|x| is −x² before 0 and x² after: f″ = ∓2 changes sign at 0, where
/// the two slopes are both 0, so f′ is continuous across the kink and
/// (0, 0) is an inflection. f′ = 2|x| is 0 at the kink alone: no extremum.
#[test]
fn a_kink_whose_slopes_join_is_an_inflection() {
    for src in ["x*abs(x)", "x+x*abs(x)", "abs(x)*x/4"] {
        let a = certify(src);
        let inf = certified(src, "inflections", &a.inflections);
        assert_eq!(inf.len(), 1, "{src}: {inf:?}");
        assert!(
            inf[0].x.is_point() && inf[0].x.lo.0 == 0.0 && inf[0].y.contains(0.0),
            "{src}: {inf:?}"
        );
        assert!(
            claims(&a.inflections).iter().any(|k| matches!(
                k,
                Claim::KinkSides {
                    at: Some(_),
                    order: 2,
                    left: KinkSign::Neg,
                    right: KinkSign::Pos,
                    joined: true,
                    ..
                }
            )),
            "{src}"
        );
        let ext = certified(src, "extrema", &a.extrema);
        assert!(ext.is_empty(), "{src}: {ext:?}");
        let mono = certified(src, "monotonicity", &a.monotonicity);
        assert!(
            matches!(&mono[..], [m] if m.dir == Dir::Increasing
                && m.on.lo == Bound::NegInf
                && m.on.hi == Bound::PosInf),
            "{src}: {mono:?}"
        );
    }
}

/// A corner whose slopes differ is no inflection the certificate names,
/// whatever f″ does on its two sides: |x² − 1| has f″ = 2 outside (−1, 1)
/// and −2 inside, and a corner at each of ±1.
#[test]
fn a_corner_is_not_an_inflection() {
    for src in ["abs(x^2-1)", "max(x^2,2-x^2)"] {
        let a = certify(src);
        assert!(
            !matches!(&a.inflections, Row::Certified { value, .. } | Row::Partial { value, .. }
                if !value.is_empty()),
            "{src}: {:?}",
            a.inflections.value()
        );
        let ext = certified(src, "extrema", &a.extrema);
        assert_eq!(ext.len(), 3, "{src}: {ext:?}");
        assert!(
            ext.iter().all(|e| e.x.is_point()),
            "{src}: each at a double: {ext:?}"
        );
    }
}

/// A kink argument with a zero of higher order: |x³| is −x³ before 0 and
/// x³ after (x³'s sign on each side by its third derivative, the first
/// two being 0 at 0), and each branch's slope ∓3x² is 0 at 0 alone.
#[test]
fn a_kink_at_a_zero_of_higher_order() {
    for src in ["abs(x^3)", "abs(x)^3", "abs(x^2)"] {
        let a = certify(src);
        let ext = certified(src, "extrema", &a.extrema);
        assert!(
            matches!(&ext[..], [e] if e.kind == ExtKind::Min
                && e.x.is_point()
                && e.x.lo.0 == 0.0
                && e.y.is_point()
                && e.y.lo.0 == 0.0),
            "{src}: {ext:?}"
        );
        let inf = certified(src, "inflections", &a.inflections);
        assert!(inf.is_empty(), "{src}: {inf:?}");
    }
    // |x² − 1|³ is twice differentiable at ±1 (its f″ is 0 there on both
    // branches and positive beside them): minima at ±1, and its only
    // inflections where 5x² = 1.
    let src = "abs(x^2-1)^3";
    let a = certify(src);
    let ext = certified(src, "extrema", &a.extrema);
    assert_eq!(
        ext.iter().map(|e| (e.x.lo.0, e.kind)).collect::<Vec<_>>(),
        [
            (-1.0, ExtKind::Min),
            (0.0, ExtKind::Max),
            (1.0, ExtKind::Min)
        ],
        "{src}"
    );
    let inf = certified(src, "inflections", &a.inflections);
    assert_eq!(inf.len(), 2, "{src}: {inf:?}");
    let r = 0.2f64.sqrt();
    assert!(
        inf[0].x.contains(-r) && inf[1].x.contains(r),
        "{src}: {inf:?}"
    );
}

/// A min or max of three or more arguments, at a point where two (or all)
/// of them cross: the branch on each side is the argument every pair puts
/// first there. min(x, 1, 2 − x) is x before 1 and 2 − x after.
#[test]
fn a_kink_of_three_or_more_arguments() {
    let src = "min(x,1,2-x)";
    let a = certify(src);
    let ext = certified(src, "extrema", &a.extrema);
    assert!(
        matches!(&ext[..], [e] if e.kind == ExtKind::Max
            && e.x.is_point()
            && e.x.lo.0 == 1.0
            && e.y.is_point()
            && e.y.lo.0 == 1.0),
        "{src}: {ext:?}"
    );
    let inf = certified(src, "inflections", &a.inflections);
    assert!(inf.is_empty(), "{src}: {inf:?}");
    let mono = certified(src, "monotonicity", &a.monotonicity);
    assert_eq!(
        mono.iter().map(|m| m.dir).collect::<Vec<_>>(),
        [Dir::Increasing, Dir::Decreasing],
        "{src}"
    );
    // Two of the three cross at ±√7 below the third: no kink of f there,
    // and the kinks at ±1 are placed.
    let src = "max(x^2,2-x^2,-5)";
    let a = certify(src);
    let ext = certified(src, "extrema", &a.extrema);
    assert_eq!(
        ext.iter()
            .map(|e| (e.x.lo.0, e.x.hi.0, e.kind))
            .collect::<Vec<_>>(),
        [
            (-1.0, -1.0, ExtKind::Min),
            (0.0, 0.0, ExtKind::Max),
            (1.0, 1.0, ExtKind::Min)
        ],
        "{src}"
    );
    let mono = certified(src, "monotonicity", &a.monotonicity);
    assert_eq!(mono.len(), 4, "{src}: {mono:?}");
    let range = certified(src, "range", &a.range);
    assert!(
        matches!(&range[..], [p] if p.hi == Bound::PosInf
            && matches!(&p.lo, Bound::At { x, closed: true } if x.is_point() && x.lo.0 == 1.0)),
        "{src}: {range:?}"
    );
}

/// A cusp or a vertical tangent of a power at a double: f″ exists on
/// each side of it and is negative (x^(2/3)) or positive (x^(4/3)) on
/// both, so there is no inflection there.
#[test]
fn a_power_singular_at_a_double_has_no_inflection_there() {
    for src in ["x^(2/3)", "x^(4/3)", "(x/1000)^(2/3)"] {
        let a = certify(src);
        let inf = certified(src, "inflections", &a.inflections);
        assert!(inf.is_empty(), "{src}: {inf:?}");
    }
}

/// A cusp at 0 of a base scaled down: its slope before 0 is a power of
/// [−5·10⁻³²⁴, 0]·½, a product that underflows. Rounded one double out on
/// both sides it held 0 and numbers past it, the power of it had no sign,
/// and the minimum at 0 went unplaced. The product stays on its side of 0.
#[test]
fn a_cusp_of_a_base_scaled_down_is_placed() {
    use graphing::interval::Interval;
    let tiny = 5e-324;
    let half = Interval::point(0.5);
    let before = Interval::new(-tiny, 0.0) * half;
    assert_eq!((before.lo(), before.hi()), (-tiny, 0.0));
    let after = Interval::new(0.0, tiny) * half;
    assert_eq!((after.lo(), after.hi()), (0.0, tiny));
    let both = Interval::new(-tiny, tiny) * Interval::point(-0.25);
    assert_eq!((both.lo(), both.hi()), (-tiny, tiny));
    for src in ["(x/2)^(2/3)", "(x/1000)^(2/3)", "(x*0.5)^(2/3)"] {
        let a = certify(src);
        let ext = certified(src, "extrema", &a.extrema);
        assert!(
            matches!(&ext[..], [e] if e.kind == ExtKind::Min && e.x.is_point() && e.x.lo.0 == 0.0),
            "{src}: {ext:?}"
        );
        let mono = certified(src, "monotonicity", &a.monotonicity);
        assert_eq!(
            mono.iter().map(|m| m.dir).collect::<Vec<_>>(),
            [Dir::Decreasing, Dir::Increasing],
            "{src}"
        );
    }
}

/// A vertical tangent no double holds: ∛(x − 0.1) is increasing through
/// it, f′ > 0 wherever it exists across the box that encloses it.
#[test]
fn a_vertical_tangent_between_two_doubles_is_no_turn() {
    for src in ["(x-0.1)^(1/3)", "((x/0.001)-1)^(1/3)", "cbrt(x-1/3)+x"] {
        let a = certify(src);
        let ext = certified(src, "extrema", &a.extrema);
        assert!(ext.is_empty(), "{src}: {ext:?}");
        let mono = certified(src, "monotonicity", &a.monotonicity);
        assert!(
            matches!(&mono[..], [m] if m.dir == Dir::Increasing
                && m.on.lo == Bound::NegInf
                && m.on.hi == Bound::PosInf),
            "{src}: {mono:?}"
        );
        assert!(
            claims(&a.monotonicity).iter().any(|k| matches!(
                k,
                Claim::KinkSides {
                    at: None,
                    order: 1,
                    left: KinkSign::Pos,
                    right: KinkSign::Pos,
                    joined: false,
                    ..
                }
            )),
            "{src}"
        );
    }
    // A cusp no double holds turns there, and nothing proves on which
    // side of what: not decided, and not taken for monotone.
    let src = "(x-0.1)^(2/3)";
    let a = certify(src);
    if let Some(mono) = a.monotonicity.value() {
        assert_eq!(mono.len(), 2, "{src}: {mono:?}");
    }
    if let Some(ext) = a.extrema.value() {
        assert_eq!(ext.len(), 1, "{src}: {ext:?}");
    }
}

/// What the branches do not prove stays unknown; nothing weaker decides
/// it. Each of these rows is true if certified at all.
#[test]
fn what_the_branches_do_not_prove_is_not_certified_otherwise() {
    // (x − 0.1)·|x − 0.1|: each branch's slope ∓2(x − 0.1) is 0 at the
    // kink itself, which no double holds. f is increasing with no
    // extremum; a certificate may say so or say nothing.
    let src = "(x-0.1)*abs(x-0.1)";
    let a = certify(src);
    if let Some(ext) = a.extrema.value() {
        assert!(ext.is_empty(), "{src}: {ext:?}");
    }
    if let Some(mono) = a.monotonicity.value() {
        assert!(
            mono.iter().all(|m| m.dir == Dir::Increasing),
            "{src}: {mono:?}"
        );
    }
    // |sin x|: f″ = −|sin x| is 0 at each kink and negative beside it,
    // so there is no inflection.
    let src = "abs(sin(x))";
    let a = certify(src);
    if let Some(inf) = a.inflections.value() {
        assert!(inf.is_empty(), "{src}: {inf:?}");
    }
    // x^(5/3) has an inflection at 0, where f′ = 0 and f″ is singular.
    let src = "x^(5/3)";
    let a = certify(src);
    if let Row::Certified { value, .. } = &a.inflections {
        assert!(
            matches!(&value[..], [i] if i.x.contains(0.0)),
            "{src}: {value:?}"
        );
    }
    // The valleys of R19-M-01: 0 at c, 0.01 outside |x − c| < 10⁻¹⁰⁰⁰⁰⁰³,
    // so with a strict minimum at c that no sample reaches.
    for centre in ["sin(1)", "1/3", "0.1"] {
        for front in ["", "x+", "3-"] {
            let src = format!("{front}min((x-({centre}))^(2/1000003),0.01)");
            let a = certify(&src);
            if let Some(ext) = a.extrema.value() {
                assert!(
                    !ext.is_empty(),
                    "{src}: its extremum at the centre is not absent"
                );
            }
            if let Some(mono) = a.monotonicity.value() {
                assert!(mono.len() >= 2, "{src}: {mono:?}");
            }
            if front.is_empty()
                && let Some(range) = a.range.value()
            {
                assert!(
                    !range.iter().all(|p| p.lo == p.hi),
                    "{src}: it takes the values 0 and 0.01: {range:?}"
                );
            }
        }
    }
}

/// The claim is true as stated for the functions above: a few of its
/// instances, checked against what the functions are.
#[test]
fn kink_sides_say_what_the_branches_do() {
    let sides = |src: &str, order: u8| -> Vec<(Option<f64>, KinkSign, KinkSign, bool)> {
        let a = certify(src);
        let row: &[Claim] = if order == 1 {
            claims(&a.extrema)
        } else {
            claims(&a.inflections)
        };
        row.iter()
            .filter_map(|c| match c {
                Claim::KinkSides {
                    at,
                    order: o,
                    left,
                    right,
                    joined,
                    ..
                } if *o == order => Some((at.map(|r| r.0), *left, *right, *joined)),
                _ => None,
            })
            .collect()
    };
    use KinkSign::{Neg, Pos, Zero};
    // |x|: f″ = 0 on both sides; its slopes −1, 1 do not join.
    assert_eq!(sides("abs(x)", 2), [(Some(0.0), Zero, Zero, false)]);
    // x·|x|: f″ = −2, 2; slopes 0, 0.
    assert_eq!(sides("x*abs(x)", 2), [(Some(0.0), Neg, Pos, true)]);
    // |x³|: f″ = −6x before 0 (positive there), 6x after; slopes 0, 0.
    assert_eq!(sides("abs(x^3)", 2), [(Some(0.0), Pos, Pos, true)]);
    // |x − 0.1|, the kink between two doubles.
    assert_eq!(sides("abs(x-0.1)", 1), [(None, Neg, Pos, false)]);
    assert_eq!(sides("abs(x-0.1)", 2), [(None, Zero, Zero, false)]);
    // x^(2/3): f″ = −(2/9)·x^(−4/3) on each side of the cusp.
    assert_eq!(sides("x^(2/3)", 2), [(Some(0.0), Neg, Neg, false)]);
    // min(x, 1, 2 − x): slopes 1 and −1 at the double 1 (f′ at a kink
    // placed with strict slopes is a `KinkAt` where two arguments meet;
    // three meet here).
    assert_eq!(sides("min(x,1,2-x)", 1), [(Some(1.0), Pos, Neg, false)]);
}
