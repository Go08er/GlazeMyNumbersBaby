//! The allowance for a picture that changes, or a field that is coloured
//! again (docs/theming-media.md, "Pixel pictures"): up to 2 tokens, 30
//! gained a second, one spent each time. (A plain "not within 33 ms"
//! would refuse every other frame of a steady 30 a second, since frames
//! arrive a millisecond early as often as late.)

/// Tokens gained a second.
pub const RATE: f64 = 30.0;
/// The most held at once.
pub const BURST: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bucket {
    tokens: f64,
    /// When the tokens were last counted, in the caller's seconds.
    at: Option<f64>,
}

impl Default for Bucket {
    fn default() -> Self {
        Bucket {
            tokens: BURST,
            at: None,
        }
    }
}

impl Bucket {
    pub fn new() -> Bucket {
        Bucket::default()
    }

    /// Spends a token at `now` (seconds on any clock that only goes
    /// forward), if there is one.
    pub fn take(&mut self, now: f64) -> bool {
        if now.is_finite() {
            match self.at {
                Some(then) if now > then => {
                    self.tokens = (self.tokens + (now - then) * RATE).min(BURST);
                    self.at = Some(now);
                }
                // A clock that stands still or steps back gains nothing.
                Some(_) => {}
                None => self.at = Some(now),
            }
        }
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thirty_a_second_with_two_in_hand() {
        let mut b = Bucket::new();
        // Two at once, then no more at that moment.
        assert!(b.take(10.0));
        assert!(b.take(10.0));
        assert!(!b.take(10.0));
        // A thirtieth of a second later, one more.
        assert!(!b.take(10.0 + 0.5 / 30.0));
        assert!(b.take(10.0 + 1.01 / 30.0));
        assert!(!b.take(10.0 + 1.02 / 30.0));
        // A long rest fills it to two, not beyond.
        assert!(b.take(20.0));
        assert!(b.take(20.0));
        assert!(!b.take(20.0));
        // A clock that steps back gains nothing.
        assert!(!b.take(19.0));
        assert!(!b.take(f64::NAN));
    }

    #[test]
    fn a_steady_thirty_a_second_is_never_refused() {
        // Frames a millisecond early as often as late.
        let mut b = Bucket::new();
        let mut refused = 0;
        for i in 0..600 {
            let jitter = if i % 2 == 0 { -0.001 } else { 0.001 };
            if !b.take(100.0 + f64::from(i) / 30.0 + jitter) {
                refused += 1;
            }
        }
        assert_eq!(refused, 0);
        // Sixty a second gets half.
        let mut b = Bucket::new();
        let taken = (0..600)
            .filter(|i| b.take(100.0 + f64::from(*i) / 60.0))
            .count();
        assert!((300..=303).contains(&taken), "{taken}");
    }
}
