// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! `std::mt19937` + `std::uniform_real_distribution<double>(0, 1)` as used by
//! `CCalcEngine::GenerateRandomNumber` (libstdc++'s `generate_canonical`
//! algorithm: two 32-bit draws combined in double precision).

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

/// `std::mt19937`
#[derive(Clone)]
pub struct Mt19937 {
    mt: [u32; N],
    mti: usize,
}

impl Mt19937 {
    pub fn new(seed: u32) -> Self {
        let mut mt = [0u32; N];
        mt[0] = seed;
        for i in 1..N {
            mt[i] = 1_812_433_253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Mt19937 { mt, mti: N }
    }

    /// Seeds from OS randomness (stand-in for `std::random_device`).
    pub fn from_entropy() -> Self {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(0x5eed);
        Mt19937::new(h.finish() as u32)
    }

    pub fn next_u32(&mut self) -> u32 {
        if self.mti >= N {
            for kk in 0..N {
                let y = (self.mt[kk] & UPPER_MASK) | (self.mt[(kk + 1) % N] & LOWER_MASK);
                let mut v = self.mt[(kk + M) % N] ^ (y >> 1);
                if y & 1 != 0 {
                    v ^= MATRIX_A;
                }
                self.mt[kk] = v;
            }
            self.mti = 0;
        }

        let mut y = self.mt[self.mti];
        self.mti += 1;

        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// `std::uniform_real_distribution<>(0, 1)(*this)`
    pub fn uniform_01(&mut self) -> f64 {
        // generate_canonical<double, 53>: k = ceil(53 / 32) = 2 draws.
        let r = 4_294_967_296.0f64; // urng.max() - urng.min() + 1
        let mut sum = 0.0f64;
        let mut tmp = 1.0f64;
        for _ in 0..2 {
            sum += f64::from(self.next_u32()) * tmp;
            tmp *= r;
        }
        let mut ret = sum / tmp;
        if ret >= 1.0 {
            ret = 1.0f64 - f64::EPSILON / 2.0; // nextafter(1.0, 0.0)
        }
        0.0 + (1.0 - 0.0) * ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mt19937_reference_value() {
        // The 10000th invocation of a default-constructed mt19937 is 4123659995.
        let mut g = Mt19937::new(5489);
        let mut v = 0;
        for _ in 0..10000 {
            v = g.next_u32();
        }
        assert_eq!(v, 4_123_659_995);
    }

    #[test]
    fn uniform_in_range() {
        let mut g = Mt19937::from_entropy();
        for _ in 0..1000 {
            let x = g.uniform_01();
            assert!((0.0..1.0).contains(&x));
        }
    }
}
