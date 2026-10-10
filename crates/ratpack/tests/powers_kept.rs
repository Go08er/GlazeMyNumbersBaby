// Rust port: GMNB contributors.

//! Extension: numbers read together (`with_powers_kept`) are the numbers
//! read alone, digit for digit of numerator and denominator.

use ratpack::{Rational, change_constants, string_to_rat, with_powers_kept, work_done};

/// xorshift64: the same numbers on every run.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

struct Typed {
    negative: bool,
    mantissa: String,
    exponent_negative: bool,
    exponent: String,
    radix: u32,
    precision: i32,
}

impl Typed {
    fn read(&self) -> Rational {
        string_to_rat(
            self.negative,
            &self.mantissa,
            self.exponent_negative,
            &self.exponent,
            self.radix,
            self.precision,
        )
        .unwrap()
        .unwrap()
    }
}

fn numbers() -> Vec<Typed> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let digits = b"0123456789ABCDEF";
    (0..400)
        .map(|i| {
            let radix = [10, 10, 10, 16, 8, 2][i % 6];
            let count = 1 + rng.below(33) as usize;
            let mut mantissa: String = (0..count)
                .map(|_| char::from(digits[rng.below(u64::from(radix)) as usize]))
                .collect();
            mantissa.insert(1.min(mantissa.len()), '.');
            // Exponents that repeat, that differ by little and that are far
            // apart, the ends of what is typed and of what is written
            // among them. An exponent is written in its number's radix.
            let exponent = match i % 5 {
                0 => 9999,
                1 => 9999 - rng.below(100) as u32,
                2 => rng.below(40) as u32,
                3 => 19_999 - rng.below(3) as u32,
                _ => rng.below(10_000) as u32,
            };
            let exponent = match radix {
                10 => exponent.to_string(),
                16 => format!("{:X}", exponent % 4096),
                8 => format!("{:o}", exponent % 512),
                _ => format!("{:b}", exponent % 64),
            };
            Typed {
                negative: rng.below(2) == 0,
                mantissa,
                exponent_negative: rng.below(2) == 0,
                exponent,
                radix,
                precision: [32, 16, 64][i % 3],
            }
        })
        .collect()
}

fn read_all<'a>(numbers: impl Iterator<Item = &'a Typed>) -> Vec<String> {
    numbers.map(|n| format!("{:?}", n.read())).collect()
}

#[test]
fn numbers_read_together_are_the_numbers_read_alone() {
    change_constants(10, 32);
    let numbers = numbers();
    let work = work_done();
    let alone = read_all(numbers.iter());
    let alone_work = work_done() - work;

    let work = work_done();
    let together = with_powers_kept(|| read_all(numbers.iter()));
    let together_work = work_done() - work;
    assert_eq!(together, alone);
    // Sharing is what it is for.
    assert!(
        together_work < alone_work / 3,
        "{together_work} {alone_work}"
    );

    // In another order.
    let backwards = with_powers_kept(|| read_all(numbers.iter().rev()));
    assert!(backwards.iter().rev().eq(alone.iter()));

    // And afterwards alone again, at the cost of reading alone.
    let work = work_done();
    assert_eq!(read_all(numbers.iter()), alone);
    assert_eq!(work_done() - work, alone_work);
}
