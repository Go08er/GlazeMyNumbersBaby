//! Decorated intervals (IEEE 1788 decorations) with strict-sign facts.

use super::arith::Interval;

/// What is known about a function on its input box (IEEE 1788 §8), from
/// least to most: [`Dec::Ill`] < [`Dec::Trv`] < [`Dec::Def`] < [`Dec::Dac`]
/// < [`Dec::Com`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dec {
    /// Not an interval (an invalid construction).
    Ill,
    /// Nothing known: the function may be undefined somewhere on the box.
    Trv,
    /// Defined everywhere on the box (maybe discontinuous).
    Def,
    /// Defined and continuous on the box.
    Dac,
    /// Defined, continuous and bounded on the box, from bounded inputs.
    Com,
}

/// An interval with a decoration and strict-sign facts: `pos` means every
/// value the function takes is > 0 even where the enclosure reaches 0
/// (e^−1000 encloses as [0, 5·10⁻³²⁴]); `neg` likewise for < 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecInterval {
    pub iv: Interval,
    pub dec: Dec,
    pub pos: bool,
    pub neg: bool,
}

impl DecInterval {
    /// A decorated interval: [`Dec::Com`] if bounded, [`Dec::Dac`] if not,
    /// [`Dec::Trv`] if empty.
    pub fn new(iv: Interval) -> DecInterval {
        let dec = if iv.is_empty() {
            Dec::Trv
        } else if iv.is_bounded() {
            Dec::Com
        } else {
            Dec::Dac
        };
        DecInterval {
            iv,
            dec,
            pos: false,
            neg: false,
        }
        .normalized()
    }

    pub fn with_dec(iv: Interval, dec: Dec) -> DecInterval {
        let mut d = DecInterval::new(iv);
        d.dec = d.dec.min(dec);
        d
    }

    pub fn point(x: f64) -> DecInterval {
        DecInterval::new(Interval::point(x))
    }

    /// Nothing known: ℝ, possibly undefined.
    pub fn unknown() -> DecInterval {
        DecInterval::with_dec(Interval::ENTIRE, Dec::Trv)
    }

    /// Ill-formed.
    pub fn ill() -> DecInterval {
        DecInterval {
            iv: Interval::ENTIRE,
            dec: Dec::Ill,
            pos: false,
            neg: false,
        }
    }

    /// Caps the decoration at `d`.
    pub fn cap(mut self, d: Dec) -> DecInterval {
        self.dec = self.dec.min(d);
        self
    }

    pub fn lo(&self) -> f64 {
        self.iv.lo()
    }

    pub fn hi(&self) -> f64 {
        self.iv.hi()
    }

    pub fn is_empty(&self) -> bool {
        self.iv.is_empty()
    }

    /// Every value is > 0.
    pub fn gt0(&self) -> bool {
        !self.iv.is_empty() && (self.pos || self.iv.lo() > 0.0)
    }

    /// Every value is < 0.
    pub fn lt0(&self) -> bool {
        !self.iv.is_empty() && (self.neg || self.iv.hi() < 0.0)
    }

    /// Every value is ≠ 0.
    pub fn ne0(&self) -> bool {
        self.gt0() || self.lt0()
    }

    /// Re-derives the facts the interval itself shows, drops facts it
    /// contradicts, and caps the decoration by boundedness.
    pub fn normalized(mut self) -> DecInterval {
        if self.iv.is_empty() {
            self.pos = false;
            self.neg = false;
            if self.dec > Dec::Trv {
                self.dec = Dec::Trv;
            }
            return self;
        }
        // A strict fact is kept only while the enclosure allows it.
        if self.iv.hi() <= 0.0 && self.iv.hi() < 0.0 {
            self.pos = false;
        }
        if self.iv.lo() >= 0.0 && self.iv.lo() > 0.0 {
            self.neg = false;
        }
        if self.pos && self.iv.hi() < 0.0 {
            self.pos = false;
        }
        if self.neg && self.iv.lo() > 0.0 {
            self.neg = false;
        }
        if self.pos {
            // Values > 0: the enclosure can lose its negative part.
            self.iv = self.iv.intersect(Interval::new(0.0, f64::INFINITY));
        }
        if self.neg {
            self.iv = self.iv.intersect(Interval::new(f64::NEG_INFINITY, 0.0));
        }
        if self.dec == Dec::Com && !self.iv.is_bounded() {
            self.dec = Dec::Dac;
        }
        self
    }

    /// The result of an operation: `iv` with local decoration `local`,
    /// capped by the inputs' decorations.
    pub(crate) fn result(iv: Interval, local: Dec, inputs: &[&DecInterval]) -> DecInterval {
        let mut dec = local;
        for i in inputs {
            dec = dec.min(i.dec);
        }
        // Com needs bounded inputs too.
        if dec == Dec::Com && inputs.iter().any(|i| !i.iv.is_bounded()) {
            dec = Dec::Dac;
        }
        if inputs.iter().any(|i| i.dec == Dec::Ill) {
            return DecInterval::ill();
        }
        DecInterval {
            iv,
            dec,
            pos: false,
            neg: false,
        }
        .normalized()
    }

    /// Sets strict-sign facts known about the result.
    pub(crate) fn signs(mut self, pos: bool, neg: bool) -> DecInterval {
        self.pos |= pos;
        self.neg |= neg;
        self.normalized()
    }

    pub fn hull(&self, o: &DecInterval) -> DecInterval {
        let mut r = DecInterval::new(self.iv.hull(o.iv));
        r.dec = r.dec.min(self.dec).min(o.dec);
        r.pos = self.gt0() && o.gt0();
        r.neg = self.lt0() && o.lt0();
        r.normalized()
    }

    /// Intersection of two enclosures of the same quantity (keeps the
    /// better facts of both).
    pub fn refine(&self, o: &DecInterval) -> DecInterval {
        let iv = self.iv.intersect(o.iv);
        let mut r = *self;
        r.iv = if iv.is_empty() { self.iv } else { iv };
        r.dec = self.dec.max(o.dec).min(if r.iv.is_bounded() {
            Dec::Com
        } else {
            Dec::Dac
        });
        r.pos |= o.pos;
        r.neg |= o.neg;
        r.normalized()
    }
}
