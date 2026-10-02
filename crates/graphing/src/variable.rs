//! Slider variables (parameters such as `a` in `y = a·sin(x)`).
//!
//! Defaults come from `GraphControl/Models/Variable.h` (value 1 when created
//! by `Grapher::UpdateVariables`, step 0.1, min −5, max 5) and the setter
//! rules from `Calculator.ViewModels/GraphingCalculator/VariableViewModel.cs`.

/// When min/max collide, the other bound is moved this far away
/// (`VariableViewModel.DefaultMinMaxRange`).
pub const DEFAULT_MIN_MAX_RANGE: f64 = 10.0;

/// A slider variable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variable {
    value: f64,
    step: f64,
    min: f64,
    max: f64,
}

impl Default for Variable {
    /// `Variable(1.0)`: the value a newly discovered variable gets.
    fn default() -> Self {
        Variable::new(1.0)
    }
}

impl Variable {
    /// A variable with the given value and the default slider settings
    /// (step 0.1, min −5, max 5).
    pub fn new(value: f64) -> Variable {
        Variable {
            value,
            step: 0.1,
            min: -5.0,
            max: 5.0,
        }
    }

    /// Current value.
    pub fn value(&self) -> f64 {
        self.value
    }

    /// Slider step.
    pub fn step(&self) -> f64 {
        self.step
    }

    /// Slider minimum.
    pub fn min(&self) -> f64 {
        self.min
    }

    /// Slider maximum.
    pub fn max(&self) -> f64 {
        self.max
    }

    /// Sets the value; a value outside [min, max] extends the range to
    /// include it. Returns true if the value changed (the original then
    /// raises `VariableUpdated` and re-renders).
    pub fn set_value(&mut self, value: f64) -> bool {
        if value < self.min {
            self.min = value;
        } else if value > self.max {
            self.max = value;
        }
        let changed = self.value != value;
        self.value = value;
        changed
    }

    /// Sets the minimum; if it is ≥ max, max becomes `min + 10`.
    pub fn set_min(&mut self, min: f64) {
        if self.min != min {
            if min >= self.max {
                self.max = min + DEFAULT_MIN_MAX_RANGE;
            }
            self.min = min;
        }
    }

    /// Sets the maximum; if it is ≤ min, min becomes `max − 10`.
    pub fn set_max(&mut self, max: f64) {
        if self.max != max {
            if max <= self.min {
                self.min = max - DEFAULT_MIN_MAX_RANGE;
            }
            self.max = max;
        }
    }

    /// Sets the step. Like `EquationInputArea`, a step ≤ 0 (or not finite)
    /// is rejected and the previous step kept; returns whether it was set.
    pub fn set_step(&mut self, step: f64) -> bool {
        if step <= 0.0 || !step.is_finite() {
            return false;
        }
        self.step = step;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let v = Variable::default();
        assert_eq!(
            (v.value(), v.step(), v.min(), v.max()),
            (1.0, 0.1, -5.0, 5.0)
        );
    }

    #[test]
    fn view_model_semantics() {
        // Mirrors Calculator.Tests/GraphingViewModelTests.cs.
        let mut v = Variable::new(2.0);
        v.set_min(-5.0);
        v.set_max(5.0);
        assert!(v.set_step(0.25));
        assert_eq!(
            (v.min(), v.max(), v.step(), v.value()),
            (-5.0, 5.0, 0.25, 2.0)
        );
        v.set_step(0.5);
        v.set_value(8.0);
        assert_eq!((v.step(), v.value(), v.max()), (0.5, 8.0, 8.0));
        v.set_value(-7.0);
        assert_eq!(v.min(), -7.0);
        v.set_min(20.0);
        assert_eq!((v.min(), v.max()), (20.0, 30.0));
        v.set_max(0.0);
        assert_eq!((v.min(), v.max()), (-10.0, 0.0));
        assert!(!v.set_step(0.0));
        assert_eq!(v.step(), 0.5);
    }
}
