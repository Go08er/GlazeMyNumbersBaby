//! Interval arithmetic: the foundation of certified analysis.
//!
//! An [`Interval`] encloses a set of reals; every operation here returns an
//! enclosure of the exact image of its inputs, so a property proven of an
//! enclosure (say, f > 0 on a box) holds of the function itself.
//! [`DecInterval`] adds IEEE 1788 decorations (whether the function is
//! defined and continuous on the box) and strict-sign facts that survive
//! underflow. [`taylor`] evaluates an expression's value and first
//! derivatives over a box; [`Literals`] reads its numbers back to the exact
//! decimals that were typed.
//!
//! Rounding is outward by one ulp from correctly rounded results:
//! CORE-MATH (MIT, correctly rounded in binary64) for the elementary
//! functions, IEEE arithmetic for the rest, kept exact where the operation
//! is (TwoSum, `mul_add` residuals). Nothing depends on the rounding mode,
//! on FMA hardware or on AVX: the results are the same bits on every CPU.
//!
//! The `mpfr-oracle` feature's tests (`tests/interval_oracle.rs`) check
//! every operation against MPFR at 256 bits.

mod arith;
mod dec;
pub mod elem;
mod literal;
mod taylor;

pub use arith::Interval;
pub use dec::{Dec, DecInterval};
pub use literal::Literals;
pub use taylor::{Ctx, Series, derivs_valid, enclose, taylor};
