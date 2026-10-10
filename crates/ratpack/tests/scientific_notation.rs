// Rust port: GMNB contributors.

//! Scientific notation keeps `precision` significant digits whatever the
//! exponent (a deviation from upstream, which cuts a number below 1 short:
//! see `number_to_string`). What it writes, read back, is the value rounded
//! to `precision` significant digits, half away from zero. That is checked
//! exactly: a value is the quotient of two whole numbers, and the check
//! multiplies and compares whole numbers (`exact`), nothing else.

mod exact;

use ratpack::{Number, NumberFormat, Rational, change_constants};

/// xorshift64: the same numbers on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A whole number of `limbs` digits of base 2³¹, the top one not zero.
    fn whole(&mut self, limbs: usize) -> Vec<u32> {
        let mut digits: Vec<u32> = (0..limbs).map(|_| (self.next() >> 33) as u32).collect();
        let top = digits.last_mut().unwrap();
        *top = (*top).max(1);
        digits
    }

    /// `count` decimal digits, the first not zero.
    fn digits(&mut self, count: usize) -> String {
        (0..count)
            .map(|i| {
                let low = u64::from(i == 0);
                char::from(b'0' + (low + self.below(10 - low)) as u8)
            })
            .collect()
    }
}

fn scientific(value: &Rational, precision: i32) -> String {
    value
        .to_string_radix(10, NumberFormat::Scientific, precision)
        .unwrap()
}

/// A decimal number as it is typed: `mantissa` × 10^`exponent`, exactly.
fn decimal(negative: bool, mantissa: &str, exponent: i32) -> Rational {
    // A precision no product here reaches, so that nothing is trimmed.
    ratpack::string_to_rat(
        negative,
        mantissa,
        exponent < 0,
        &exponent.unsigned_abs().to_string(),
        10,
        1_000_000,
    )
    .unwrap()
    .unwrap()
}

#[track_caller]
fn assert_rounded(value: &Rational, precision: i32) -> String {
    let text = scientific(value, precision);
    if let Err(why) = exact::check_rounded(value, &text, 10, precision as u32) {
        panic!("precision {precision}: {why}\n  {value:?}");
    }
    text
}

const PRECISIONS: [i32; 8] = [2, 5, 8, 16, 32, 33, 64, 128];

/// Random quotients of whole numbers of up to six digits of base 2³¹ (so
/// from 10^-56 to 10^56), of either sign, at every precision.
#[test]
fn a_quotient_is_written_rounded_to_the_precision() {
    change_constants(10, 32);
    let mut rng = Rng(0x243F_6A88_85A3_08D3);
    for _ in 0..1500 {
        let (p_limbs, q_limbs) = (1 + rng.below(6) as usize, 1 + rng.below(6) as usize);
        let (p, q) = (rng.whole(p_limbs), rng.whole(q_limbs));
        let sign = if rng.below(2) == 0 { 1 } else { -1 };
        let value = Rational::from_pq(Number::new(sign, 0, p), Number::new(1, 0, q));
        for precision in PRECISIONS {
            assert_rounded(&value, precision);
        }
    }
}

/// Random decimal numbers of 1 to 45 digits with exponents from -9999 to
/// 9999, the ends among them: shorter than the precision they are written
/// as they are, longer they are rounded.
///
/// One kind is left out: an exact half above 1 (one digit more than the
/// precision, a 5). To write a number out ratpack reads only the first
/// digits of a numerator or denominator longer than it needs (one more
/// than the precision, of base 2³¹: some 300 decimal digits at 32), which
/// makes a long numerator a hair smaller: a half then falls below, in
/// plain notation as in this one. Below 1 it is the denominator that is
/// long, and a half goes up as it should (`nines_and_a_five_roll_over`).
#[test]
fn a_decimal_number_is_written_rounded_to_the_precision_at_any_exponent() {
    change_constants(10, 32);
    let mut rng = Rng(0x1319_8A2E_0370_7344);
    for i in 0..400 {
        let count = 1 + rng.below(45) as usize;
        let digits = rng.digits(count);
        let mantissa = format!("{}.{}", &digits[..1], &digits[1..]);
        let exponent = match i % 8 {
            0 => 9999,
            1 => -9999,
            2 => -(rng.below(40) as i32),
            3 => rng.below(40) as i32,
            _ => rng.below(19_999) as i32 - 9999,
        };
        let value = decimal(rng.below(2) == 0, &mantissa, exponent);
        for precision in [16, 32, 33] {
            let half = digits.len() == precision as usize + 1 && digits.ends_with('5');
            if half && exponent > 0 {
                continue;
            }
            let text = assert_rounded(&value, precision);
            // Of a number no longer than the precision, its own digits.
            if digits.len() <= precision as usize {
                let written = exact::Scientific::parse(&text, 10).unwrap();
                assert_eq!(
                    written.digits,
                    digits.trim_end_matches('0').max("0"),
                    "{text}"
                );
                assert_eq!(written.exponent, i64::from(exponent), "{text}");
            }
        }
    }
}

/// At the rounding carry: nines up to the precision and a 5 after them roll
/// over to the next power of ten, of either sign and at any exponent down
/// to -9999; with a 4 they stay nines. (An exact half far above 1 is left
/// out: ratpack reads only the first digits of a numerator that long, so
/// there it can fall either way, in plain notation too.)
#[test]
fn nines_and_a_five_roll_over() {
    change_constants(10, 32);
    for precision in [8usize, 16, 32] {
        let nines = "9".repeat(precision - 1);
        for exponent in [-9999, -5000, -100, -33, -32, -31, -3, -2, -1, 0] {
            for negative in [false, true] {
                let sign = if negative { "-" } else { "" };
                let over = decimal(negative, &format!("9.{nines}5"), exponent);
                assert_eq!(
                    assert_rounded(&over, precision as i32),
                    format!("{sign}1.e{:+}", exponent + 1)
                );
                let under = decimal(negative, &format!("9.{nines}4"), exponent);
                assert_eq!(
                    assert_rounded(&under, precision as i32),
                    format!("{sign}9.{nines}e{exponent:+}")
                );
            }
        }
        // Above 1, past the half.
        for exponent in [1, 31, 32, 33, 100, 9998] {
            let over = decimal(true, &format!("9.{nines}6"), exponent);
            assert_eq!(
                assert_rounded(&over, precision as i32),
                format!("-1.e{:+}", exponent + 1)
            );
        }
    }
}

/// What upstream cut, a digit for every decade below 1 (the C++ oracle's
/// texts are beside ours): 1 ÷ 3 ÷ 10^k keeps its 32 digits.
#[test]
fn a_third_keeps_its_digits_below_one() {
    change_constants(10, 32);
    let third = Rational::from(1).div(&Rational::from(3)).unwrap();
    let threes = "3".repeat(31);
    for (k, upstream) in [
        (0, "3.3333333333333333333333333333333e-1"),
        (2, "3.33333333333333333333333333333e-3"),
        (10, "3.333333333333333333333e-11"),
        (20, "3.33333333333e-21"),
        (31, "3.e-32"),
        (32, "0.e-33"),
        (40, "0.e-41"),
        (99, "0.e-100"),
    ] {
        let value = third.div(&decimal(false, "1", k)).unwrap();
        let ours = assert_rounded(&value, 32);
        assert_eq!(ours, format!("3.{threes}e-{}", k + 1));
        // Upstream's is ours cut short (or ours, where there is no zero
        // after the point to cut for).
        let cut = exact::Scientific::parse(upstream, 10).unwrap().digits;
        assert!(cut == "0" || threes.starts_with(&cut[1..]), "{upstream}");
        assert_eq!(cut.len() == 32, k == 0, "{upstream}");
        // As plain notation writes it, where it turns to e-notation itself.
        let plain = value.to_string_radix(10, NumberFormat::Float, 32).unwrap();
        if k >= 3 {
            assert_eq!(plain, ours);
        }
    }
    // A number typed with all its digits (upstream: 0.e-50), and a short
    // one, which upstream left alone.
    let full = decimal(false, "1.2345678901234567890123456789012", -50);
    assert_eq!(
        scientific(&full, 32),
        "1.2345678901234567890123456789012e-50"
    );
    let short = decimal(false, "1.2345", -50);
    assert_eq!(scientific(&short, 32), "1.2345e-50");
    // And the two notations agree wherever plain notation uses an exponent.
    let mut rng = Rng(0x4528_21E6_38D0_1377);
    for _ in 0..300 {
        let count = 20 + rng.below(30) as usize;
        let digits = rng.digits(count);
        let value = decimal(
            rng.below(2) == 0,
            &format!("{}.{}", &digits[..1], &digits[1..]),
            -(3 + rng.below(9000) as i32),
        );
        for precision in [16, 32] {
            let plain = value
                .to_string_radix(10, NumberFormat::Float, precision)
                .unwrap();
            assert!(plain.contains('e'), "{plain}");
            assert_eq!(scientific(&value, precision), plain);
        }
    }
}
