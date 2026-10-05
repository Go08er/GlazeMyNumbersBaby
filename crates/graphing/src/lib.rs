//! # graphing — the graphing calculator engine of the gmnb port
//!
//! Windows Calculator's graphing mode is driven by a proprietary engine
//! that the open-source repository replaces with a mock. This crate is a new,
//! numeric (non-symbolic) implementation of that engine's feature set, written
//! against the public interfaces of the original as its specification:
//! `GraphingInterfaces/*.h` (`IGraph`, `IEquation`, `IGraphAnalyzer`,
//! `IGraphRenderer`, `IGraphingOptions`, `IMathSolver`, `GraphingEnums.h`),
//! `GraphControl` (`Grapher.cpp`, `Equation.cpp`, `KeyGraphFeaturesInfo`,
//! `RenderMain.cpp`), the view models (`EquationViewModel`,
//! `VariableViewModel`, `GraphingSettingsViewModel`) and the en-US resources.
//! Those interfaces are © Microsoft Corporation, MIT licensed.
//!
//! The crate is pure math and data; geometry is produced in world (graph)
//! coordinates for a GUI to stroke/fill.
//!
//! ## Overview
//!
//! * [`equation::Equation`] — parse what the user typed: `y = …`, bare
//!   expressions, `f(x) = …`, `x = g(y)`, implicit relations such as
//!   `x^2 + y^2 = 25`, inequalities (`<`, `>`, `≤`/`<=`, `≥`/`>=`,
//!   chained `1 < y < 3`). Errors carry a char span and the original
//!   message text ([`error`]).
//! * [`compile::Program`] — bytecode with constant folding, scalar and
//!   batched evaluation; trig unit (radians/degrees/grads) baked in.
//! * [`plot`] — adaptive sampling with discontinuity detection, marching
//!   squares for implicit curves, fill polygons for inequalities.
//! * [`analysis`] — the "Function analysis" panel (domain, range,
//!   intercepts, extrema, inflection points, asymptotes, parity, period,
//!   monotonicity) with the original's texts and outcomes.
//! * [`trace`] — nearest point on a curve to the pointer.
//! * [`grid`] / [`viewport`] — 1-2-5 grid lines and labels, zoom/pan/reset.
//! * [`graph::Graph`] — ties it together: equations, shared slider
//!   [`variable::Variable`]s, trig unit.
//!
//! ## Example
//!
//! ```
//! use graphing::graph::Graph;
//! use graphing::viewport::Viewport;
//! use graphing::functions::TrigUnit;
//!
//! let mut graph = Graph::new();
//! let id = graph.add_equation("y = a sin(x)");
//! assert!(graph.error(id).is_none());
//!
//! // Slider variables appear automatically (value 1, range −5..5, step 0.1).
//! assert_eq!(graph.variable("a").unwrap().value(), 1.0);
//! graph.set_variable("a", 2.0);
//! graph.set_trig_unit(TrigUnit::Radians);
//!
//! // Geometry for a 1280×720 graph area, in world coordinates.
//! let vp = Viewport::default_for_size(1280.0, 720.0);
//! let plots = graph.plot(&vp);
//! for p in &plots {
//!     for polyline in &p.plot.curves {
//!         // stroke polyline (Vec<Point>) with the equation's colour
//!         assert!(polyline.len() >= 2);
//!     }
//! }
//!
//! // Function analysis panel.
//! let features = graph.analyze(id);
//! assert_eq!(features.range, "y ∈ [−2, 2]");
//! assert_eq!(features.periodicity_expression, "2π");
//! for row in features.items() {
//!     let _ = (row.title, row.display_items);
//! }
//!
//! // Tracing: nearest point to the pointer at pixel (700, 300).
//! if let Some((eq, point)) = graph.trace(&vp, &plots, 700.0, 300.0, 50.0) {
//!     assert_eq!(eq, id);
//!     // "(x, y)": y to the digits its enclosure fixes, or "undefined".
//!     let _label = point.text();
//! }
//! ```

// Float-literal guards (`Some(v) if v == 1.0`) read better than float patterns.
#![allow(clippy::redundant_guards)]

pub mod analysis;
pub mod ast;
pub(crate) mod big;
pub mod certify;
pub mod compile;
pub(crate) mod dd;
pub mod diff;
pub mod equation;
pub mod error;
pub mod functions;
pub mod graph;
pub mod grid;
pub mod interval;
pub mod lexer;
pub mod parser;
pub mod plot;
pub mod simplify;
pub mod strings;
pub mod trace;
pub mod variable;
pub mod viewport;
pub(crate) mod wide;

pub use equation::{Equation, EquationKind};
pub use error::EquationError;
pub use functions::TrigUnit;
pub use graph::{EquationId, Graph};
pub use plot::{Plot, Point, Polyline};
pub use viewport::Viewport;
