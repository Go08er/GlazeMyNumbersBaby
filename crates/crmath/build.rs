//! Builds the vendored CORE-MATH C (`vendor/binary64/<f>/<f>.c`).
//!
//! On x86-64 it is compiled twice: once for baseline x86-64 and once for
//! x86-64-v3 (AVX2, FMA, BMI1/2, …), whose entry points are renamed
//! `cr_<f>` → `crmath_v3_<f>` so both link into one binary; `src/lib.rs`
//! picks one at run time (`cfg(crmath_dispatch)`). CORE-MATH leans on
//! `fma()`, which only the v3 build gets as an instruction: the baseline
//! build calls a software fma, two to three times slower on trig-heavy
//! work. Both are correctly rounded, so they give the same bits.
//!
//! When the crate itself is compiled for x86-64-v3 (GMNB's packages), and
//! on other architectures (aarch64 has FMA natively), there is one build
//! and no dispatch. The environment's `TARGET_CPU`/`-march=native` is never
//! used: what a build runs on must not depend on the machine it was made
//! on.

use std::path::PathBuf;

/// The CPU features of x86-64-v3 beyond baseline that compiled code may
/// use (and `src/lib.rs` checks for).
const V3_FEATURES: &[&str] = &[
    "avx", "avx2", "fma", "bmi1", "bmi2", "lzcnt", "movbe", "f16c",
];

fn main() {
    println!("cargo::rustc-check-cfg=cfg(crmath_dispatch)");
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=vendor");

    let mut functions: Vec<String> = std::fs::read_dir("vendor/binary64")
        .expect("vendor/binary64")
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    functions.sort();
    let sources: Vec<PathBuf> = functions
        .iter()
        .map(|f| PathBuf::from(format!("vendor/binary64/{f}/{f}.c")))
        .collect();

    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let features = std::env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
    let has = |f: &str| features.split(',').any(|g| g == f);

    let build = |march: Option<&str>| {
        let mut b = cc::Build::new();
        b.files(&sources).warnings(false).cargo_warnings(false);
        if let Some(m) = march {
            b.flag(format!("-march={m}")).flag("-mtune=generic");
        }
        b
    };

    if arch != "x86_64" {
        build(None).compile("crmath");
    } else if V3_FEATURES.iter().all(|f| has(f)) {
        // The whole crate is built for v3: no baseline copy needed.
        build(Some("x86-64-v3")).compile("crmath");
    } else {
        build(Some("x86-64")).compile("crmath_base");
        let mut v3 = build(Some("x86-64-v3"));
        for f in &functions {
            v3.define(&format!("cr_{f}"), Some(format!("crmath_v3_{f}").as_str()));
        }
        v3.compile("crmath_v3");
        println!("cargo::rustc-cfg=crmath_dispatch");
    }
}
