//! What the boxes leave out beside an excluded point (a hole, a pole) or
//! an end of the domain, read as if a claim decided it: the panel's rows
//! held to the true answer, worked out by hand.
//!
//! - A zero of f′ (f″) at the double beside a hole is a turn (an
//!   inflection) or not by f′'s (f″'s) sign on the reals between it and
//!   the hole, which no claim decides; the certifier passed over it as
//!   the box's bound, and the panel said "no maxima" (or "no inflection
//!   points") of a function with one there. The claim fuzzer's long runs
//!   found four (`tests/replay_fuzz.rs`, seeds 20261025 and 20261026),
//!   holding the genuine rows to an exact truth; the strict replay
//!   refused each, the zero's side across the hole not decided.
//! - A side condition's sign at the first double of a piece another
//!   condition cut was carried back to that cut: the domain was
//!   certified with reals where f is undefined in it, or without reals
//!   where f is defined (the audit of every place a row reads across a
//!   gap that followed).
//!
//! Each fails on `7618817`. A row may say less (partial, or unknown):
//! never something false.

use graphing::analysis::{KeyGraphFeatures, Monotonicity, analyze_str, flags};
use graphing::certify::{Analysis, Bound, DEFAULT_BUDGET, Row, certify_text};
use graphing::compile::CompileOptions;

/// The true answer: each feature as the doubles about it (one double
/// twice, where it is one), and the maximal monotone stretches.
struct Truth {
    src: &'static str,
    minima: &'static [(f64, f64)],
    maxima: &'static [(f64, f64)],
    inflections: &'static [(f64, f64)],
    /// (from, to, increasing): f strictly monotone from `from` to `to`
    /// where it is defined, and on no longer stretch.
    monotone: &'static [(f64, f64, bool)],
}

/// `v` lies at the feature `(lo, hi)`: within an ulp of the doubles about
/// it (the panel's number is the middle of a certified enclosure).
fn at(v: f64, (lo, hi): (f64, f64)) -> bool {
    lo.next_down() <= v && v <= hi.next_up()
}

/// One list row of the panel against the true features: unknown says
/// nothing; otherwise each item is a true one, no two the same, and a
/// complete row has them all.
fn list_row(
    k: &KeyGraphFeatures,
    name: &str,
    flag: u32,
    listed: &[f64],
    truth: &[(f64, f64)],
) -> Vec<String> {
    let mut bad = Vec::new();
    if k.too_complex_features & flag != 0 {
        if !listed.is_empty() {
            bad.push(format!("{name}: unknown, yet lists {listed:?}"));
        }
        return bad;
    }
    let mut used = vec![false; truth.len()];
    for &v in listed {
        match (0..truth.len()).find(|&i| !used[i] && at(v, truth[i])) {
            Some(i) => used[i] = true,
            None => bad.push(format!("{name}: {v:e} is none of {truth:?}")),
        }
    }
    if k.partial_features & flag == 0 && listed.len() != truth.len() {
        bad.push(format!(
            "{name}: complete with {listed:?}, but there are {truth:?}"
        ));
    }
    bad
}

/// The panel's rows of `t.src` against the truth.
fn panel(t: &Truth) -> Vec<String> {
    let k = analyze_str(t.src);
    let mut bad = Vec::new();
    let xs = |v: &[(graphing::analysis::Family, f64)]| -> Vec<f64> {
        v.iter().map(|(f, _)| f.x).collect()
    };
    bad.extend(list_row(
        &k,
        "minima",
        flags::MINIMA,
        &xs(&k.data.minima),
        t.minima,
    ));
    bad.extend(list_row(
        &k,
        "maxima",
        flags::MAXIMA,
        &xs(&k.data.maxima),
        t.maxima,
    ));
    bad.extend(list_row(
        &k,
        "inflection points",
        flags::INFLECTION_POINTS,
        &xs(&k.data.inflection_points),
        t.inflections,
    ));
    // Each piece listed lies in a true stretch of its direction; a
    // complete row lists them all.
    if k.too_complex_features & flags::MONOTONE_INTERVALS == 0 {
        for (iv, dir) in &k.data.monotonicity {
            let up = match dir {
                Monotonicity::Increasing => true,
                Monotonicity::Decreasing => false,
                d => {
                    bad.push(format!("monotonicity: {d:?} on {iv:?}"));
                    continue;
                }
            };
            let inside = t.monotone.iter().any(|&(a, b, u)| {
                u == up && a.next_down() <= iv.lo.value && iv.hi.value <= b.next_up()
            });
            if !inside {
                bad.push(format!(
                    "monotonicity: {dir:?} on {iv:?}, in none of {:?}",
                    t.monotone
                ));
            }
        }
        if k.partial_features & flags::MONOTONE_INTERVALS == 0
            && k.data.monotonicity.len() != t.monotone.len()
        {
            bad.push(format!(
                "monotonicity: complete with {:?}, but f is monotone on {:?}",
                k.data.monotonicity, t.monotone
            ));
        }
    }
    bad.into_iter().map(|b| format!("{}: {b}", t.src)).collect()
}

/// f′ (f″) is 0 at the double beside a hole, and of the other sign than
/// before it on the reals up to the hole: a turn (an inflection) there,
/// at a point of the domain. Each of these had that row certified
/// without it.
#[test]
fn a_zero_beside_a_hole_is_no_bound_of_the_box() {
    let truths = [
        // f′ = (x − ½)·(½(x − ½) + 2⁻⁵⁵): 0 at ½ (the hole) and at
        // ½ − 2⁻⁵⁴, the double below; f″ = (x − ½) + 2⁻⁵⁵ < 0 there, a
        // maximum. f″ is 0 at ½ − 2⁻⁵⁵, between the two.
        Truth {
            src: "1/6*(x-1/2)^3+2^(-56)*(x-1/2)^2+1/3+0/(x-(1/2))",
            minima: &[],
            maxima: &[(0.49999999999999994, 0.49999999999999994)],
            inflections: &[(0.49999999999999994, 0.5)],
            monotone: &[
                (f64::NEG_INFINITY, 0.49999999999999994, true),
                (0.49999999999999994, 0.5, false),
                (0.5, f64::INFINITY, true),
            ],
        },
        // f′ = (x + 2)·(x + 2 − 2⁻⁵²): 0 at −2 and at the hole, the double
        // above; negative between: a maximum at −2. f″ is 0 at −2 + 2⁻⁵³,
        // between the two.
        Truth {
            src: "1/3*(x+2)^3-2^(-53)*(x+2)^2+1/3+0/(x-(-9007199254740991*2^(-52)))",
            minima: &[],
            maxima: &[(-2.0, -2.0)],
            inflections: &[(-2.0, -1.9999999999999998)],
            monotone: &[
                (f64::NEG_INFINITY, -2.0, true),
                (-2.0, -1.9999999999999998, false),
                (-1.9999999999999998, f64::INFINITY, true),
            ],
        },
        // f′ = −3(x − ½)·(x − ½ + 2⁻⁵⁴): 0 at the hole ½ − 2⁻⁵⁴ and at ½,
        // the double above; positive between: a maximum at ½. f″ is 0 at
        // ½ − 2⁻⁵⁵, between the two.
        Truth {
            src: "-1*(x-1/2)^3-3*2^(-55)*(x-1/2)^2+1+0/(x-(9007199254740991*2^(-54)))",
            minima: &[],
            maxima: &[(0.5, 0.5)],
            inflections: &[(0.49999999999999994, 0.5)],
            monotone: &[
                (f64::NEG_INFINITY, 0.49999999999999994, false),
                (0.49999999999999994, 0.5, true),
                (0.5, f64::INFINITY, false),
            ],
        },
        // f″ = (x − 3)·(½(x − 3) − 2⁻⁵²): 0 at the hole 3 and at 3 + 2⁻⁵¹,
        // the double above; negative between, positive past it: an
        // inflection there. f′ = (x − 3)³/6 − 2⁻⁵³(x − 3)² + 2⁻⁵¹ has one
        // zero, below 3: a minimum.
        Truth {
            src: "1/24*(x-3)^4-2^(-53)/3*(x-3)^3+2^(-51)*(x-3)-1+0/(x-(3))",
            minima: &[(2.9999861364700866, 2.999986136470087)],
            maxima: &[],
            inflections: &[(3.0000000000000004, 3.0000000000000004)],
            monotone: &[
                (f64::NEG_INFINITY, 2.9999861364700866, false),
                (2.999986136470087, 3.0, true),
                (3.0, f64::INFINITY, true),
            ],
        },
    ];
    let bad: Vec<String> = truths.iter().flat_map(panel).collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

/// Two poles at adjacent doubles, f defined between them: f″ =
/// 2/(x − 1)³ + 2/(x − 1 − 2⁻⁵²)³ is 0 at 1 + 2⁻⁵³, where f inflects.
/// The domain left the reals between the poles out (the second divisor's
/// zero at its box's first double; below), and the inflection points
/// with them: "none", complete.
#[test]
fn the_reals_between_two_poles_keep_their_inflection() {
    let t = Truth {
        src: "1/(x-1)+1/(x-(1+2^-52))",
        minima: &[],
        maxima: &[],
        inflections: &[(1.0, 1.0000000000000002)],
        monotone: &[
            (f64::NEG_INFINITY, 1.0, false),
            (1.0, 1.0000000000000002, false),
            (1.0000000000000002, f64::INFINITY, false),
        ],
    };
    let bad = panel(&t);
    assert!(bad.is_empty(), "{bad:#?}");
}

fn certify(src: &str) -> Analysis {
    certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None)
        .unwrap_or_else(|e| panic!("{src}: {e}"))
}

/// A domain's end, as the doubles about it (one double twice where it is
/// one), and whether the end belongs to the domain. `None`: ±∞.
type End = Option<((f64, f64), bool)>;

/// The certified domain (the panel's row is written from it, and is
/// unknown with it) against the truth: each piece's end enclosed as
/// listed, open or closed as it is, and no piece more or less.
fn domain(src: &str, truth: &[(End, End)]) -> Vec<String> {
    let a = certify(src);
    let k = analyze_str(src);
    let mut bad = Vec::new();
    // (Certified, the panel may still say less: two ends it can't tell
    // apart.)
    let Row::Certified { value, .. } = &a.domain else {
        if k.too_complex_features & flags::DOMAIN == 0 {
            bad.push(format!(
                "{src}: the domain is unknown, yet shown: {}",
                k.domain
            ));
        }
        return bad;
    };
    let holds = |b: Bound, e: End| match (b, e) {
        (Bound::NegInf | Bound::PosInf, None) => true,
        (Bound::At { x, closed }, Some(((lo, hi), c))) => {
            closed == c && x.lo.0 <= lo && hi <= x.hi.0 && (x.lo.0 < x.hi.0 || lo == hi)
        }
        _ => false,
    };
    let ok = value.excluded.is_empty()
        && value.pieces.len() == truth.len()
        && value
            .pieces
            .iter()
            .zip(truth)
            .all(|(p, &(lo, hi))| holds(p.lo, lo) && holds(p.hi, hi));
    if !ok {
        bad.push(format!(
            "{src}: the domain is certified {:?}, but it is {truth:?}",
            value.pieces
        ));
    }
    bad
}

/// A later condition at the reals between a piece's cut and its box's
/// first double: ln's argument 2⁶⁰·(x − 1) − ½ is negative up to
/// 1 + 2⁻⁶¹, past the cut the divisor made at 1 (certified (1, ∞)); and
/// a second divisor's zero at the box's first double left out the piece
/// (1, 1 + 2⁻⁵²) where f is defined (certified (−∞, 1) ∪ (1 + 2⁻⁵², ∞)).
#[test]
fn a_cut_is_no_end_of_the_conditions_after_it() {
    let one = 1.0f64;
    let (up, down) = (one.next_up(), one.next_down());
    let open = |lo: f64, hi: f64| Some(((lo, hi), false));
    let bad: Vec<String> = [
        // 1 + 2⁻⁶¹ is between 1 and the double above it.
        ("1/(x-1)+ln(2^60*(x-1)-1/2)", vec![(open(one, up), None)]),
        (
            "1/(x-1)+sqrt(2^60*(x-1)-1/2)",
            vec![(Some(((one, up), true)), None)],
        ),
        // 1 − 2⁻⁶¹ is between the double below 1 and 1.
        ("1/(x-1)+ln(2^60*(1-x)-1/2)", vec![(None, open(down, one))]),
        (
            "1/(x-1)+1/(x-(1+2^-52))",
            vec![
                (None, open(one, one)),
                (open(one, one), open(up, up)),
                (open(up, up), None),
            ],
        ),
    ]
    .iter()
    .flat_map(|(src, truth)| domain(src, truth))
    .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}
