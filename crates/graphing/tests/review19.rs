//! Ordinary mathematical counterexamples found in round 19.

use graphing::certify::{Bound, DEFAULT_BUDGET, Dir, Row, certify_text};
use graphing::compile::CompileOptions;

/// The valley has a unique zero and strict minimum at c, and is 0.01
/// outside |x-c| < 10^(-1000003). No sampled double reaches its bottom
/// when c is not representable. Continuity of the unresolved kink box
/// cannot certify that the function is constant or has no extrema.
#[test]
fn a_valley_between_doubles_is_not_a_constant() {
    for centre in ["sin(1)", "1/3"] {
        for affine in [false, true] {
            let valley = format!("min((x-({centre}))^(2/1000003),0.01)");
            let source = if affine {
                format!("x+{valley}")
            } else {
                valley
            };
            let a = certify_text(&source, CompileOptions::default(), DEFAULT_BUDGET, None)
                .expect("ordinary expression");

            // Adding x leaves a strict minimum at c: |t|^r dominates
            // |t| near 0 for 0<r<1, on both sides of c.
            assert!(
                !matches!(&a.extrema, Row::Certified { value, .. } if value.is_empty()),
                "{source}: its strict minimum must not be certified absent"
            );
            if let Row::Certified { value, .. } | Row::Partial { value, .. } = &a.monotonicity {
                assert!(
                    !value.iter().any(|m| m.on.lo == Bound::NegInf
                        && m.on.hi == Bound::PosInf
                        && (m.dir == Dir::Constant || (affine && m.dir == Dir::Increasing))),
                    "{source}: a whole-line monotonicity contradicts its valley"
                );
            }
            if !affine && let Row::Certified { value, .. } = &a.range {
                assert!(
                    !value.iter().all(|p| p.lo == p.hi),
                    "{source}: the attained values 0 and 0.01 are different"
                );
            }
        }
    }
}
