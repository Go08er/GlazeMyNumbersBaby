//! Double-double arithmetic: a number as the unevaluated sum `hi + lo` of
//! two doubles (about 106 bits), for the few extended-range results that
//! must still come out right to the last bit of a double.
//!
//! A power or exponential beyond a double's range is e^t with t = y·ln|b|.
//! In double arithmetic t carries an error of about |t|·2⁻⁵³, and e^t then
//! a relative error of the same size: hundreds of ulps once |t| is in the
//! hundreds, which is exactly where results leave the doubles. Here t is
//! kept to about 2⁻¹⁰⁶·|t|, so e^t is as good as the libm exponential of a
//! small reduced argument.

use std::f64::consts::{LN_2, LN_10};

/// `hi + lo`, with |lo| at most half an ulp of `hi`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Dd {
    pub hi: f64,
    pub lo: f64,
}

/// ln 2 to about 106 bits.
pub(crate) const LN2: Dd = Dd {
    hi: LN_2,
    lo: 2.319_046_813_846_299_6e-17,
};

/// ln 10 to about 106 bits.
pub(crate) const LN10: Dd = Dd {
    hi: LN_10,
    lo: -2.170_756_223_382_249_4e-16,
};

/// a + b exactly, as a rounded sum and its error.
fn two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    let bb = s - a;
    Dd {
        hi: s,
        lo: (a - (s - bb)) + (b - bb),
    }
}

/// a · b exactly, as a rounded product and its error.
fn two_prod(a: f64, b: f64) -> Dd {
    let p = a * b;
    Dd {
        hi: p,
        lo: a.mul_add(b, -p),
    }
}

impl Dd {
    pub(crate) fn of(v: f64) -> Dd {
        Dd { hi: v, lo: 0.0 }
    }

    /// Renormalised: |lo| at most half an ulp of hi.
    fn norm(hi: f64, lo: f64) -> Dd {
        let s = hi + lo;
        Dd {
            hi: s,
            lo: lo - (s - hi),
        }
    }

    pub(crate) fn add(self, o: Dd) -> Dd {
        let s = two_sum(self.hi, o.hi);
        let t = two_sum(self.lo, o.lo);
        let s = Dd::norm(s.hi, s.lo + t.hi);
        Dd::norm(s.hi, s.lo + t.lo)
    }

    pub(crate) fn neg(self) -> Dd {
        Dd {
            hi: -self.hi,
            lo: -self.lo,
        }
    }

    pub(crate) fn mul(self, o: Dd) -> Dd {
        let p = two_prod(self.hi, o.hi);
        Dd::norm(p.hi, p.lo + (self.hi * o.lo + self.lo * o.hi))
    }

    pub(crate) fn mul_f(self, f: f64) -> Dd {
        let p = two_prod(self.hi, f);
        Dd::norm(p.hi, p.lo + self.lo * f)
    }

    pub(crate) fn div(self, o: Dd) -> Dd {
        let q1 = self.hi / o.hi;
        let r = self.add(o.mul_f(q1).neg());
        let q2 = r.hi / o.hi;
        let r = r.add(o.mul_f(q2).neg());
        let q3 = r.hi / o.hi;
        Dd::norm(q1, q2).add(Dd::of(q3))
    }

    pub(crate) fn div_f(self, f: f64) -> Dd {
        self.div(Dd::of(f))
    }
}

/// ln m for a finite m > 0, to about 2⁻¹⁰⁰ relative: m = 2^k·r with
/// √½ ≤ r < √2, c = j/64 the nearest from a table of ln c, and
/// ln(r/c) = 2·atanh(s), s = (r − c)/(r + c), |s| < 1/180, by its series.
/// (Near m = 1, r is m itself and c is 1: nothing cancels.)
pub(crate) fn ln(m: f64) -> Dd {
    debug_assert!(m > 0.0 && m.is_finite());
    // A power's base is usually the same constant point after point (e^x,
    // 2^x): the last logarithm is kept.
    thread_local! {
        static LAST: std::cell::Cell<(f64, Dd)> = const { std::cell::Cell::new((1.0, Dd { hi: 0.0, lo: 0.0 })) };
    }
    let (lm, ll) = LAST.get();
    if lm == m {
        return ll;
    }
    let l = ln_uncached(m);
    LAST.set((m, l));
    l
}

fn ln_uncached(m: f64) -> Dd {
    let (mut r, mut k) = frexp(m);
    if r < std::f64::consts::FRAC_1_SQRT_2 {
        r *= 2.0;
        k -= 1;
    }
    let j = (r * 64.0).round() as usize;
    let c = j as f64 / 64.0;
    // r − c is exact (c/2 ≤ r ≤ 2c); r + c needn't be.
    let s = Dd::of(r - c).div(two_sum(r, c));
    let z = s.mul(s);
    // 2s·(1 + z/3 + z²/5 + z³/7 + …) with z < 4·10⁻⁵: the terms to z³ in
    // double-double, the rest (below 2·10⁻¹⁸) in double.
    let z2 = z.mul(z);
    let z3 = z2.mul(z);
    let rest = z3.hi * z.hi * (1.0 / 9.0 + z.hi * (1.0 / 11.0 + z.hi / 13.0));
    let series = Dd::of(1.0)
        .add(z.mul(THIRD))
        .add(z2.mul(FIFTH))
        .add(z3.mul(SEVENTH))
        .add(Dd::of(rest));
    let (lc_hi, lc_lo) = LN_TABLE[j - 45];
    s.mul(series)
        .mul_f(2.0)
        .add(Dd {
            hi: lc_hi,
            lo: lc_lo,
        })
        .add(LN2.mul_f(f64::from(k)))
}

/// 1/3, 1/5 and 1/7 to about 106 bits.
const THIRD: Dd = Dd {
    hi: 0.3333333333333333,
    lo: 1.850371707708594e-17,
};
const FIFTH: Dd = Dd {
    hi: 0.2,
    lo: -1.1102230246251566e-17,
};
const SEVENTH: Dd = Dd {
    hi: 0.14285714285714285,
    lo: 7.93016446160826e-18,
};

/// ln(j/64) for j = 45..=91, as hi + lo (from Python's decimal).
#[rustfmt::skip]
const LN_TABLE: [(f64, f64); 47] = [
    (-0.3522205935893521, -5.7233316949182485e-18),
    (-0.33024168687057687, 1.0828321637483858e-17),
    (-0.3087354816496133, 1.6199186085148102e-17),
    (-0.2876820724517809, -2.607160616442564e-17),
    (-0.26706278524904525, 7.32891532732017e-18),
    (-0.24686007793152578, -1.361743371748368e-17),
    (-0.22705745063534608, -9.551415762738488e-18),
    (-0.2076393647782445, -1.2053243216686129e-17),
    (-0.18859116980755003, 7.432164219196925e-18),
    (-0.16989903679539747, 4.868008764439071e-19),
    (-0.15154989812720093, -5.1669593684615594e-18),
    (-0.13353139262452263, 3.664457663660085e-18),
    (-0.1158318155251217, -4.338484369808096e-18),
    (-0.09844007281325252, 4.439009633675136e-18),
    (-0.0813456394539524, -5.07707635593117e-18),
    (-0.06453852113757118, 6.470486661692933e-18),
    (-0.048009219186360606, -1.4390903347292205e-18),
    (-0.0317486983145803, -3.0382263084680858e-18),
    (-0.015748356968139168, -1.0021578630528974e-18),
    (0.0, 0.0),
    (0.015504186535965254, -3.278321022892429e-19),
    (0.030771658666753687, 1.0431732029005968e-18),
    (0.0458095360312942, 1.902959866474257e-18),
    (0.06062462181643484, 2.6424025938726934e-18),
    (0.07522342123758753, -5.930604196293241e-18),
    (0.08961215868968714, -5.4268129336647135e-18),
    (0.10379679368164356, 5.47772415726659e-18),
    (0.11778303565638346, -1.1971685747593677e-18),
    (0.13157635778871926, 1.1123000879729588e-17),
    (0.1451820098444979, 8.242418783022475e-18),
    (0.15860503017663857, 1.1257003872182592e-17),
    (0.17185025692665923, -6.0224538210113705e-18),
    (0.184922338494012, 3.0236614153574064e-18),
    (0.19782574332991987, 1.2821194372980142e-17),
    (0.21056476910734964, -4.249405314729895e-18),
    (0.22314355131420976, -9.091270597324799e-18),
    (0.2355660713127669, -2.3943371495187355e-18),
    (0.24783616390458127, -1.2432209578702523e-17),
    (0.25995752443692605, 2.069806938978935e-17),
    (0.27193371548364176, 7.83319637697442e-19),
    (0.2837681731306446, -2.032665581126656e-17),
    (0.2954642128938359, -2.16461086040599e-17),
    (0.3070250352949119, -1.2319916200101964e-17),
    (0.3184537311185346, 2.7114779367326236e-17),
    (0.329753286372468, 2.122020616196946e-18),
    (0.3409265869705932, 1.7467136443544747e-17),
    (0.3519764231571782, -1.2953893030191963e-17),
];

/// e^t as `(r, n)` with e^t = r·2ⁿ, r within an ulp or so of the true
/// mantissa (1 ≤ r < 2 or nearby) and n an integer valued double. For
/// |t| below about 2⁵² · ln 2 (beyond that, n itself isn't exact).
pub(crate) fn exp(t: Dd) -> (f64, f64) {
    let n = (t.hi / LN_2).round();
    // t − n·ln 2, exactly enough: n·ln 2's hi part is an exact product.
    let r = t.add(LN2.mul_f(n).neg());
    (r.hi.exp() * (1.0 + r.lo), n)
}

/// m = r·2^k with ½ ≤ r < 1, for a finite m ≠ 0.
fn frexp(m: f64) -> (f64, i32) {
    let (m, adj) = if m.abs() < f64::MIN_POSITIVE {
        (m * 18446744073709551616.0, -64)
    } else {
        (m, 0)
    };
    let bits = m.to_bits();
    let e = ((bits >> 52) & 0x7ff) as i32 - 1022;
    let r = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52));
    (r, e + adj)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Within `ulps` ulps of `want` (as hi + lo rounded).
    fn close(got: f64, want: f64, ulps: f64) -> bool {
        let u = f64::from_bits(want.abs().to_bits() + 1) - want.abs();
        (got - want).abs() <= ulps * u
    }

    #[test]
    fn logarithms_to_the_last_bit() {
        // ln of the exact doubles, split into hi + lo, from Python's decimal
        // at 80 digits.
        for (m, hi, lo) in [
            (1.024, 0.023716526617316065, -1.5774243488668216e-18),
            (std::f64::consts::E, 1.0, -5.318237706605891e-17),
            (1e-300, -690.7755278982137, -2.3670096176709832e-14),
            (
                1.7976931348623157e308,
                709.782712893384,
                2.3636017071323592e-14,
            ),
            (0.75, -0.2876820724517809, -2.607160616442564e-17),
            (3.0, 1.0986122886681098, -9.07129723500153e-17),
            // Table edges, both ends of the reduced range, and extremes.
            (1.0, 0.0, 0.0),
            (
                1.0156249999999998,
                0.015504186535965036,
                -3.8120820924239937e-19,
            ),
            (1.015625, 0.015504186535965254, -3.278321022892429e-19),
            (
                1.9999999999999998,
                0.6931471805599452,
                2.319046813846299e-17,
            ),
            (0.5, -LN_2, -2.3190468138462996e-17),
            (
                std::f64::consts::SQRT_2,
                0.3465735902799727,
                2.4442169414592898e-17,
            ),
            (
                1.0000000000000002,
                2.2204460492503128e-16,
                3.649214750845877e-48,
            ),
            (
                0.9999999999999999,
                -1.1102230246251565e-16,
                -6.162975822039155e-33,
            ),
            (5e-324, -744.4400719213812, -4.422444340918698e-14),
            (123456.789, 11.723646487185881, -4.1025541885795297e-16),
        ] {
            let l = ln(m);
            assert_eq!(l.hi, hi, "ln {m}: {l:?}");
            assert!(
                (l.lo - lo).abs() <= hi.abs() * 1e-31,
                "ln {m}: {l:?} vs {lo}"
            );
        }
    }

    #[test]
    fn exponentials_of_large_arguments() {
        // e^t = r·2ⁿ with n = round(t / ln 2), from Python's decimal.
        for (t, r_true, n_true) in [
            (Dd::of(-725.0), 1.0324667745294627, -1046.0),
            (Dd::of(1000.0), 0.809465158140234, 1443.0),
            (ln(10.0).mul_f(-400.0), 1.1718289888396994, -1329.0),
        ] {
            let (r, n) = exp(t);
            assert_eq!(n, n_true, "{t:?}");
            assert!(close(r, r_true, 1.0), "{t:?}: {r} vs {r_true}");
        }
    }
}
