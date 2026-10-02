// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of the CEngine value wrappers around ratpak: `CalcEngine::Rational`
//! (`Rational.cpp`) and `CalcEngine::RationalMath` (`RationalMath.cpp`).
//! `CalcEngine::Number` (`Number.cpp`) is [`crate::Number`] itself, which
//! doubles as ratpak's `NUMBER`.

pub(crate) mod rational;
pub mod rational_math;
