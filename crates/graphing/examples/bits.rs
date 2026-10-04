//! The evaluator's results, bit for bit, as one hash per expression and
//! angle unit: `cargo run --release -p graphing --example bits [-- --full]`.
//!
//! The built-in functions are CORE-MATH's, correctly rounded, so the bits
//! must not depend on the CPU the code was built for. CI runs this from a
//! baseline x86-64 build and an x86-64-v3 build (as GMNB ships) and
//! compares the output. `--full` prints every value instead of a hash, for
//! finding a difference.

use graphing::TrigUnit;
use graphing::compile::{Input, Program, compile_str};

/// One argument per function call, so each function's own bits are seen.
const EXPRS: &[&str] = &[
    "sin(x)",
    "cos(x)",
    "tan(x)",
    "sec(x)",
    "csc(x)",
    "cot(x)",
    "asin(x)",
    "acos(x)",
    "atan(x)",
    "asec(x)",
    "acsc(x)",
    "acot(x)",
    "sinh(x)",
    "cosh(x)",
    "tanh(x)",
    "sech(x)",
    "csch(x)",
    "coth(x)",
    "asinh(x)",
    "acosh(x)",
    "atanh(x)",
    "asech(x)",
    "acsch(x)",
    "acoth(x)",
    "sqrt(x)",
    "cbrt(x)",
    "root(x,5)",
    "log(x)",
    "ln(x)",
    "log(2,x)",
    "log(7,x)",
    "exp(x)",
    "x^3",
    "x^7",
    "x^-5",
    "x^(1/3)",
    "x^(2/5)",
    "x^0.37",
    "x^x",
    "2^x",
    "x!",
    "nCr(x,3)",
    "nPr(x,2.5)",
    "mod(x,7)",
    "floor(x)",
    "round(x)",
];

/// Arguments: special values, subnormals, the neighbourhoods of ±1, powers
/// of ten across the range, multiples of π/4 and of quarter turns in
/// degrees and grads, and a fixed pseudo-random spread.
fn arguments() -> Vec<f64> {
    let mut xs = vec![
        0.0,
        -0.0,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::INFINITY,
        f64::NAN,
        1.0,
        1.0 + f64::EPSILON,
        1.0 - f64::EPSILON / 2.0,
        0.5,
        2.0,
        1e15 + 1.0,
    ];
    for k in -320..=308 {
        xs.push(10f64.powi(k));
        xs.push(3.0 * 10f64.powi(k));
    }
    for k in -16..=16 {
        xs.push(f64::from(k) * std::f64::consts::FRAC_PI_4);
        xs.push(f64::from(k) * 45.0);
        xs.push(f64::from(k) * 50.0);
    }
    let mut s: u64 = 0x9e37_79b9_7f4a_7c15;
    for _ in 0..3000 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        // Mantissa and a moderate exponent, so most values are ordinary.
        let e = (s >> 52) as i64 % 64 - 32;
        let m = 1.0 + (s & ((1 << 52) - 1)) as f64 / (1u64 << 52) as f64;
        xs.push(m * 2f64.powi(e as i32));
    }
    let neg: Vec<f64> = xs.iter().map(|x| -x).collect();
    xs.extend(neg);
    xs
}

/// FNV-1a over the values' bit patterns (NaNs collapsed to one pattern).
fn hash(values: &[f64]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for v in values {
        let bits = if v.is_nan() {
            0x7ff8_0000_0000_0000
        } else {
            v.to_bits()
        };
        for b in bits.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

fn main() {
    let full = std::env::args().any(|a| a == "--full");
    let xs = arguments();
    for src in EXPRS {
        for unit in [TrigUnit::Radians, TrigUnit::Degrees, TrigUnit::Grads] {
            let p: Program = compile_str(src, unit).expect(src);
            // The scalar and batch paths must agree too.
            let scalar: Vec<f64> = xs.iter().map(|&x| p.eval(x, 0.0)).collect();
            let mut batch = vec![0.0; xs.len()];
            p.eval_batch(Input::Slice(&xs), Input::Scalar(0.0), &mut batch);
            let same = scalar
                .iter()
                .zip(&batch)
                .all(|(a, b)| a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()));
            assert!(same, "{src} [{unit:?}]: scalar and batch evaluation differ");
            if full {
                for (x, v) in xs.iter().zip(&scalar) {
                    println!(
                        "{src}\t{unit:?}\t{:016x}\t{:016x}",
                        x.to_bits(),
                        v.to_bits()
                    );
                }
            } else {
                println!("{src:<12} {unit:<8?} {:016x}", hash(&scalar));
            }
        }
    }
}
