# crmath

CORE-MATH's correctly rounded binary64 functions for the graphing engine,
used under the crate name `core_math`.

- **Upstream:** the CORE-MATH project, <https://core-math.gitlabpages.inria.fr/>,
  commit `85430b3e`, as vendored by the `core-math-sys` crate 1.4.0
  (<https://github.com/jdh8/core-math-sys>, commit
  `c27f24ada7845827dc5e79069b2d090d7c408ece`).
- **Licence:** MIT. CORE-MATH's permission notice is `vendor/LICENSE`;
  the copyrights are in each C file's header, collected in
  `vendor/COPYRIGHT` (which says how). Both ship with every package and
  appear in the apps' About/licence screens.
- **What's vendored:** `vendor/binary64/<f>/<f>.c` (and the headers it
  includes) for each function the graphing code calls, copied unchanged
  apart from the two patches below. To add one, copy its directory from the
  same upstream commit, add it to `src/lib.rs`, and add its copyright line
  to `vendor/COPYRIGHT`.

## Local patches

Both are marked `GMNB patch` in the source.

- **`pow/pow.c`:** after its last Ziv iteration upstream prints
  "Unexpected worst-case found" and calls `exit(1)`, a path CORE-MATH's
  worst-case search says is unreachable. A graphing app mustn't exit on one
  value, so the patch returns the rounding of that iteration's
  approximation (relative error below 2⁻²⁴⁰), as the branch before it does.
- **`lgamma/lgamma.c`:** upstream stores the sign of Γ(x) in libc's global
  `signgam`, which concurrent plot threads would race on. The patch
  redirects it to a thread-local in that file; nothing reads it (the crate
  only exposes `lgamma`'s value).

## Why not the `core-math` crate

`core-math-sys` compiles the C once, with `-march=$TARGET_CPU`, defaulting
to the build machine (`native`). CORE-MATH relies on `fma()`; a baseline
x86-64 build (DGMNB, which must run on any 64-bit PC) can only call it in
software, and trig-heavy plots ran two to three times slower than with an
FMA build.

Here, on x86-64, `build.rs` compiles the C twice, for `-march=x86-64` and
for `-march=x86-64-v3` (the v3 entry points renamed `crmath_v3_<f>`), and
each call takes the v3 build on a CPU that has every v3 feature (checked
once, cached in an atomic). Both builds are correctly rounded, so the
results are bit-identical; `GRAPHING_CRMATH=baseline` forces the baseline
build for testing. A crate compiled for x86-64-v3 (GMNB's packages) and
other architectures get one build and direct calls. The build machine's
CPU never matters.
