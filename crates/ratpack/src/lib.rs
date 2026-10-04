// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Rust port of Windows Calculator's `Ratpack` arbitrary-precision rational
//! library together with the thin C++ wrappers that sit on top of it
//! (`CalcEngine::Number`, `CalcEngine::Rational`, `CalcEngine::RationalMath`).
//!
//! # API CONTRACT
//! The public surface below is what `calcmanager` (the CEngine port) codes
//! against. Implementations are free to change; signatures should not without
//! coordinating with the `calcmanager` crate.
//!
//! * Everything that can `throw` a `CALC_E_*` code in C++ returns
//!   [`CalcResult`] here, carrying the identical numeric code.
//! * Ratpack's process globals (radix, precision, `g_ratio`, `rat_*`/`num_*`
//!   constants, decimal separator, `g_ftrueinfinite`) live in ONE
//!   thread-local context that [`change_constants`] / [`set_decimal_separator`]
//!   mutate — mirroring the C++ semantics exactly while keeping each thread
//!   (e.g. each `cargo test`) isolated.
//! * A fresh thread starts in the state the calculator engine establishes
//!   first (`InitialOneTimeOnlySetup` → `ChangeConstants(10, 32)`).
//!
//! # Porting notes
//! The algorithms are a line-by-line port of the C++ (see `src/ratpak/`),
//! including its quirks; the crate is differentially tested against the
//! original C++ build (`tests/golden.rs`, `tools/oracle/ratpack/`).

mod cengine;
mod ratpak;

pub use cengine::rational_math;
pub use ratpak::work_done;

// ---------------------------------------------------------------------------
// Errors (CalcErr.h)
// ---------------------------------------------------------------------------

/// A `CALC_E_*` error code, bit-identical to the C++ values.
pub type CalcErr = u32;
pub type CalcResult<T> = Result<T, CalcErr>;

/// The current operation would require a divide by zero to complete
pub const CALC_E_DIVIDEBYZERO: CalcErr = 0x8000_0000;
/// The given input is not within the domain of this function
pub const CALC_E_DOMAIN: CalcErr = 0x8000_0001;
/// The result of this function is undefined
pub const CALC_E_INDEFINITE: CalcErr = 0x8000_0002;
/// The result of this function is Positive Infinity.
pub const CALC_E_POSINFINITY: CalcErr = 0x8000_0003;
/// The result of this function is Negative Infinity
pub const CALC_E_NEGINFINITY: CalcErr = 0x8000_0004;
/// The given input is within the domain of the function but is beyond
/// the range for which calc can successfully compute the answer
pub const CALC_E_INVALIDRANGE: CalcErr = 0x8000_0006;
/// There is not enough free memory to complete the requested function
pub const CALC_E_OUTOFMEMORY: CalcErr = 0x8000_0007;
/// The result of this operation is an overflow
pub const CALC_E_OVERFLOW: CalcErr = 0x8000_0008;
/// The result of this operation is undefined
pub const CALC_E_NORESULT: CalcErr = 0x8000_0009;

// ---------------------------------------------------------------------------
// Enums (ratpak.h)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumberFormat {
    /// Floating point, or exponential if number is too big.
    Float,
    /// Always scientific notation.
    Scientific,
    /// Engineering notation: exponent is a multiple of 3.
    Engineering,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AngleType {
    /// Calculate trig using 360 degrees per revolution
    Degrees,
    /// Calculate trig using 2 pi radians per revolution
    Radians,
    /// Calculate trig using 400 gradians per revolution
    Gradians,
}

/// `CalcEngine::RATIONAL_BASE`: default Base/Radix to use for Rational
/// calculations. RatPack calculations currently support up to Base64.
pub const RATIONAL_BASE: u32 = 10;
/// `CalcEngine::RATIONAL_PRECISION`: default Precision to use for Rational
/// calculations.
pub const RATIONAL_PRECISION: i32 = 128;

// ---------------------------------------------------------------------------
// Number (CalcEngine::Number)
// ---------------------------------------------------------------------------

/// Value type mirroring `CalcEngine::Number` (and ratpak's `NUMBER`): sign,
/// exponent and mantissa digits in the internal radix (`BASEX` = 2^31),
/// least significant digit first.
///
/// Equality is structural.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Number {
    /// The sign of the mantissa, +1, or -1
    pub(crate) sign: i32,
    /// The offset of digits from the radix point
    pub(crate) exp: i32,
    /// The digits; `cdigit` is `mant.len()`.
    pub(crate) mant: Vec<u32>,
}

impl Default for Number {
    /// `Number()` — zero: sign 1, exp 0, mantissa `{0}`.
    fn default() -> Self {
        Number {
            sign: 1,
            exp: 0,
            mant: vec![0],
        }
    }
}

impl Number {
    pub fn new(sign: i32, exp: i32, mantissa: Vec<u32>) -> Self {
        Number {
            sign,
            exp,
            mant: mantissa,
        }
    }
    pub fn sign(&self) -> i32 {
        self.sign
    }
    pub fn exp(&self) -> i32 {
        self.exp
    }
    pub fn mantissa(&self) -> &[u32] {
        &self.mant
    }
    pub fn is_zero(&self) -> bool {
        self.mant.iter().all(|&d| d == 0)
    }
}

pub use cengine::rational::Rational;

// ---------------------------------------------------------------------------
// Ratpack globals used directly by CEngine
// ---------------------------------------------------------------------------

/// `ChangeConstants(radix, precision)` — re-derives every radix/precision
/// dependent constant in the thread-local ratpack context.
///
/// Like the C++, whether constants are recomputed or re-read from the
/// precomputed tables depends on the highest `g_ratio * radix * precision`
/// seen so far on this thread.
pub fn change_constants(radix: u32, precision: i32) {
    ratpak::with_ctx_mut(|ctx| {
        // Only allocation failure can make this fail (the C++ would throw out
        // of ChangeConstants too); treat it like Rust allocation failure.
        ratpak::support::change_constants(ctx, radix, precision)
            .unwrap_or_else(|e| panic!("ChangeConstants({radix}, {precision}) failed: {e:#x}"))
    })
}

/// `SetDecimalSeparator(ch)`
pub fn set_decimal_separator(sep: char) {
    ratpak::with_ctx_mut(|ctx| ctx.decimal_separator = sep)
}

/// The current ratpak decimal separator (`g_decimalSeparator`).
pub fn decimal_separator() -> char {
    ratpak::with_ctx(|ctx| ctx.decimal_separator)
}

/// `StringToRat(...)`; `None` where C++ returns `nullptr`.
pub fn string_to_rat(
    mantissa_is_negative: bool,
    mantissa: &str,
    exponent_is_negative: bool,
    exponent: &str,
    radix: u32,
    precision: i32,
) -> CalcResult<Option<Rational>> {
    ratpak::with_ctx(|ctx| {
        ratpak::conv::string_to_rat(
            ctx,
            mantissa_is_negative,
            mantissa,
            exponent_is_negative,
            exponent,
            radix,
            precision,
        )
    })
    .map(|r| r.map(Rational::from_rat))
}

/// Snapshots of the context constants CEngine reads directly
/// (`rat_qword`, `rat_dword`, `rat_word`, `rat_byte`, `rat_exp`).
pub fn rat_qword() -> Rational {
    ratpak::with_ctx(|ctx| Rational::from_rat(ctx.rat_qword.clone()))
}
pub fn rat_dword() -> Rational {
    ratpak::with_ctx(|ctx| Rational::from_rat(ctx.rat_dword.clone()))
}
pub fn rat_word() -> Rational {
    ratpak::with_ctx(|ctx| Rational::from_rat(ctx.rat_word.clone()))
}
pub fn rat_byte() -> Rational {
    ratpak::with_ctx(|ctx| Rational::from_rat(ctx.rat_byte.clone()))
}
pub fn rat_exp() -> Rational {
    ratpak::with_ctx(|ctx| Rational::from_rat(ctx.rat_exp.clone()))
}
/// `ln_ten` (used by `RationalMath::Log10`).
pub fn ln_ten() -> Rational {
    ratpak::with_ctx(|ctx| Rational::from_rat(ctx.ln_ten.clone()))
}
/// `pi` at the current precision.
pub fn pi() -> Rational {
    ratpak::with_ctx(|ctx| Rational::from_rat(ctx.pi.clone()))
}
