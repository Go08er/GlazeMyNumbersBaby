//! Certified analysis: function-analysis rows that are *proven*, each with
//! a machine-readable certificate a separate checker can replay.
//!
//! Every row is either certified (complete and correct), partial (every
//! item proven, but there may be more beyond the region the proof covers)
//! or unknown. Proofs come from the interval core (`crate::interval`): an
//! enclosure that excludes 0 proves a sign, a strictly monotone box with
//! opposite signs at its ends proves exactly one crossing, and the side
//! conditions of the original tree (divisors, logarithm arguments, roots,
//! the trigonometric poles) prove where f is defined.
//!
//! Not wired into the panel yet: `crate::analysis` still produces what the
//! app shows.

pub mod cert;
pub mod cover;
pub mod fun;

pub use cert::*;
pub use fun::{Fun, Stop, canonical};
