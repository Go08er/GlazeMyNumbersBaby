//! CORE-MATH's correctly rounded binary64 functions
//! (<https://core-math.gitlabpages.inria.fr/>, MIT; the C is vendored in
//! `vendor/`, see `README.md`).
//!
//! The API is the subset of the `core-math` crate's that the graphing code
//! uses, under the same names. Every function returns the correctly rounded
//! result (round to nearest), so the answer never depends on the CPU or the
//! build.
//!
//! On x86-64, unless the crate is itself compiled for x86-64-v3, the C is
//! built twice, for baseline x86-64 and for x86-64-v3, and each call takes
//! the v3 build on a CPU that has it: CORE-MATH leans on `fma`, which the
//! baseline build can only call in software. The choice is made once
//! (cached in an atomic) and costs a predictable branch per call. Setting
//! `GRAPHING_CRMATH=baseline` keeps the baseline build (for tests); nothing
//! selects the v3 build on a CPU without it.

#[cfg(crmath_dispatch)]
use std::sync::atomic::{AtomicU8, Ordering};

/// Which build of the C a call goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    /// Baseline x86-64 (software `fma`).
    Baseline,
    /// x86-64-v3 (AVX2, FMA, BMI1/2, LZCNT, MOVBE, F16C).
    V3,
    /// The single build of a binary compiled for one target: x86-64-v3
    /// code, or another architecture.
    Native,
}

#[cfg(crmath_dispatch)]
mod dispatch {
    use super::*;

    /// 0: not yet decided; 1: baseline; 2: v3.
    static STATE: AtomicU8 = AtomicU8::new(0);

    #[inline(always)]
    pub(crate) fn v3() -> bool {
        match STATE.load(Ordering::Relaxed) {
            2 => true,
            1 => false,
            _ => decide(),
        }
    }

    /// Whether this CPU has everything the v3 build may use.
    pub(crate) fn cpu_has_v3() -> bool {
        std::arch::is_x86_feature_detected!("avx")
            && std::arch::is_x86_feature_detected!("avx2")
            && std::arch::is_x86_feature_detected!("fma")
            && std::arch::is_x86_feature_detected!("bmi1")
            && std::arch::is_x86_feature_detected!("bmi2")
            && std::arch::is_x86_feature_detected!("lzcnt")
            && std::arch::is_x86_feature_detected!("movbe")
            && std::arch::is_x86_feature_detected!("f16c")
    }

    #[cold]
    fn decide() -> bool {
        let forced_baseline = std::env::var_os("GRAPHING_CRMATH").is_some_and(|v| v == "baseline");
        let v3 = !forced_baseline && cpu_has_v3();
        STATE.store(if v3 { 2 } else { 1 }, Ordering::Relaxed);
        v3
    }

    pub(crate) fn set(v3: bool) {
        STATE.store(if v3 { 2 } else { 1 }, Ordering::Relaxed);
    }
}

/// The build calls currently go to.
pub fn build() -> Build {
    #[cfg(crmath_dispatch)]
    {
        if dispatch::v3() {
            Build::V3
        } else {
            Build::Baseline
        }
    }
    #[cfg(not(crmath_dispatch))]
    {
        Build::Native
    }
}

/// Send calls to `want`, as far as this CPU and binary allow, and return
/// the build now in force. Only for tests and checks: both builds give the
/// same results. `Build::V3` on a CPU without it, or any choice in a binary
/// with one build, changes nothing.
pub fn select(want: Build) -> Build {
    #[cfg(crmath_dispatch)]
    {
        match want {
            Build::Baseline => dispatch::set(false),
            Build::V3 | Build::Native => dispatch::set(dispatch::cpu_has_v3()),
        }
    }
    #[cfg(not(crmath_dispatch))]
    let _ = want;
    build()
}

mod ffi {
    macro_rules! decls {
        ($($f:ident),* $(,)?) => {
            unsafe extern "C" {
                $(pub fn $f(x: f64) -> f64;)*
                pub fn cr_pow(x: f64, y: f64) -> f64;
                pub fn cr_sincos(x: f64, s: *mut f64, c: *mut f64);
            }
        };
    }
    decls!(
        cr_acos, cr_acosh, cr_acospi, cr_asin, cr_asinh, cr_asinpi, cr_atan, cr_atanh, cr_atanpi,
        cr_cbrt, cr_cos, cr_cosh, cr_cospi, cr_exp, cr_lgamma, cr_log, cr_log10, cr_log1p, cr_log2,
        cr_sin, cr_sinh, cr_sinpi, cr_tan, cr_tanh, cr_tgamma,
    );

    #[cfg(crmath_dispatch)]
    pub mod v3 {
        macro_rules! decls {
            ($($f:ident),* $(,)?) => {
                unsafe extern "C" {
                    $(pub fn $f(x: f64) -> f64;)*
                    pub fn crmath_v3_pow(x: f64, y: f64) -> f64;
                    pub fn crmath_v3_sincos(x: f64, s: *mut f64, c: *mut f64);
                }
            };
        }
        decls!(
            crmath_v3_acos,
            crmath_v3_acosh,
            crmath_v3_acospi,
            crmath_v3_asin,
            crmath_v3_asinh,
            crmath_v3_asinpi,
            crmath_v3_atan,
            crmath_v3_atanh,
            crmath_v3_atanpi,
            crmath_v3_cbrt,
            crmath_v3_cos,
            crmath_v3_cosh,
            crmath_v3_cospi,
            crmath_v3_exp,
            crmath_v3_lgamma,
            crmath_v3_log,
            crmath_v3_log10,
            crmath_v3_log1p,
            crmath_v3_log2,
            crmath_v3_sin,
            crmath_v3_sinh,
            crmath_v3_sinpi,
            crmath_v3_tan,
            crmath_v3_tanh,
            crmath_v3_tgamma,
        );
    }
}

/// One-argument functions: `name = baseline symbol / v3 symbol`.
macro_rules! unary {
    ($($(#[$doc:meta])* $name:ident = $base:ident / $v3:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[must_use]
        #[inline]
        pub fn $name(x: f64) -> f64 {
            #[cfg(crmath_dispatch)]
            if dispatch::v3() {
                // SAFETY: a pure C function of one double.
                return unsafe { ffi::v3::$v3(x) };
            }
            // SAFETY: a pure C function of one double.
            unsafe { ffi::$base(x) }
        }
    )*};
}

unary!(
    acos = cr_acos / crmath_v3_acos,
    acosh = cr_acosh / crmath_v3_acosh,
    /// acos(x)/π.
    acospi = cr_acospi / crmath_v3_acospi,
    asin = cr_asin / crmath_v3_asin,
    asinh = cr_asinh / crmath_v3_asinh,
    /// asin(x)/π.
    asinpi = cr_asinpi / crmath_v3_asinpi,
    atan = cr_atan / crmath_v3_atan,
    atanh = cr_atanh / crmath_v3_atanh,
    /// atan(x)/π.
    atanpi = cr_atanpi / crmath_v3_atanpi,
    cbrt = cr_cbrt / crmath_v3_cbrt,
    cos = cr_cos / crmath_v3_cos,
    cosh = cr_cosh / crmath_v3_cosh,
    /// cos(πx).
    cospi = cr_cospi / crmath_v3_cospi,
    exp = cr_exp / crmath_v3_exp,
    /// ln |Γ(x)|.
    lgamma = cr_lgamma / crmath_v3_lgamma,
    /// Natural logarithm.
    log = cr_log / crmath_v3_log,
    log10 = cr_log10 / crmath_v3_log10,
    /// ln(1 + x).
    log1p = cr_log1p / crmath_v3_log1p,
    log2 = cr_log2 / crmath_v3_log2,
    sin = cr_sin / crmath_v3_sin,
    sinh = cr_sinh / crmath_v3_sinh,
    /// sin(πx).
    sinpi = cr_sinpi / crmath_v3_sinpi,
    tan = cr_tan / crmath_v3_tan,
    tanh = cr_tanh / crmath_v3_tanh,
    /// Γ(x).
    tgamma = cr_tgamma / crmath_v3_tgamma,
);

/// x^y.
#[must_use]
#[inline]
pub fn pow(x: f64, y: f64) -> f64 {
    #[cfg(crmath_dispatch)]
    if dispatch::v3() {
        // SAFETY: a pure C function of two doubles.
        return unsafe { ffi::v3::crmath_v3_pow(x, y) };
    }
    // SAFETY: a pure C function of two doubles.
    unsafe { ffi::cr_pow(x, y) }
}

/// (sin x, cos x).
#[must_use]
#[inline]
pub fn sincos(x: f64) -> (f64, f64) {
    let (mut s, mut c) = (0.0, 0.0);
    #[cfg(crmath_dispatch)]
    if dispatch::v3() {
        // SAFETY: writes one double through each of two valid pointers.
        unsafe { ffi::v3::crmath_v3_sincos(x, &mut s, &mut c) };
        return (s, c);
    }
    // SAFETY: writes one double through each of two valid pointers.
    unsafe { ffi::cr_sincos(x, &mut s, &mut c) };
    (s, c)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every function, on both builds where there are two: the same bits.
    #[test]
    fn builds_agree() {
        let xs = [
            -1e300,
            -710.5,
            -3.5,
            -1.0,
            -0.75,
            -0.5,
            -1e-300,
            -5e-324,
            0.0,
            5e-324,
            1e-10,
            0.5,
            0.999,
            1.0,
            1.5,
            2.0,
            std::f64::consts::PI,
            10.0,
            100.0,
            709.7,
            1e15,
            1e300,
        ];
        type Named = (&'static str, fn(f64) -> f64);
        let fs: [Named; 25] = [
            ("acos", acos),
            ("acosh", acosh),
            ("acospi", acospi),
            ("asin", asin),
            ("asinh", asinh),
            ("asinpi", asinpi),
            ("atan", atan),
            ("atanh", atanh),
            ("atanpi", atanpi),
            ("cbrt", cbrt),
            ("cos", cos),
            ("cosh", cosh),
            ("cospi", cospi),
            ("exp", exp),
            ("lgamma", lgamma),
            ("log", log),
            ("log10", log10),
            ("log1p", log1p),
            ("log2", log2),
            ("sin", sin),
            ("sinh", sinh),
            ("sinpi", sinpi),
            ("tan", tan),
            ("tanh", tanh),
            ("tgamma", tgamma),
        ];
        let run = || {
            let mut out = Vec::new();
            for (_, f) in fs {
                for &x in &xs {
                    out.push(f(x).to_bits());
                }
            }
            for &x in &xs {
                for &y in &[-2.5, -1.0, 0.0, 0.5, 3.0, 1e10] {
                    out.push(pow(x, y).to_bits());
                }
                let (s, c) = sincos(x);
                out.push(s.to_bits());
                out.push(c.to_bits());
            }
            out
        };
        let first = select(Build::Baseline);
        let a = run();
        let second = select(Build::V3);
        let b = run();
        select(Build::Native);
        assert_eq!(a, b, "{first:?} vs {second:?}");
        // Spot values, correctly rounded.
        assert_eq!(exp(1.0), std::f64::consts::E);
        assert_eq!(sin(0.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(cbrt(-8.0), -2.0);
    }
}
