//! Touch contacts: one finger acts as the pointer, two pinch-zoom the graph.
//!
//! Policy: a pinch only zooms if its midpoint started on the graph canvas,
//! and it stays with the canvas wherever the fingers wander. A third finger
//! suspends it; when the count drops back to two, the spread is measured
//! afresh so the zoom carries on from the remaining pair without a jump.
//! Once fingers have pinched, none of them acts as the pointer again until
//! all have lifted.

use std::collections::HashMap;

/// What a touch event means for the app.
#[derive(Debug, PartialEq)]
pub enum Gesture {
    /// The first finger went down, moved, or lifted: act like the pointer.
    Press(f32, f32),
    Move(f32, f32),
    Release,
    /// The first finger's touch was cancelled: drop the press, don't click.
    Cancel,
    /// A second finger landed: end any one-finger press or drag.
    PinchStart,
    /// Zoom about (x, y) by `factor` (below 1 zooms in).
    Zoom {
        x: f32,
        y: f32,
        factor: f64,
    },
    None,
}

#[derive(Default)]
pub struct Touches {
    points: HashMap<u64, (f32, f32)>,
    /// Spread of the pinching pair when last measured, and whether the
    /// pinch began on the canvas. Kept until every finger has lifted.
    pinch: Option<(f32, bool)>,
}

impl Touches {
    /// A finger landed. `on_canvas` says whether a point is on the graph.
    pub fn down(
        &mut self,
        id: u64,
        x: f32,
        y: f32,
        on_canvas: impl FnOnce(f32, f32) -> bool,
    ) -> Gesture {
        self.points.insert(id, (x, y));
        match self.points.len() {
            1 => Gesture::Press(x, y),
            2 => {
                let (d, (mx, my)) = self.spread();
                self.pinch = Some((d, on_canvas(mx, my)));
                Gesture::PinchStart
            }
            _ => Gesture::None,
        }
    }

    pub fn moved(&mut self, id: u64, x: f32, y: f32) -> Gesture {
        let Some(p) = self.points.get_mut(&id) else {
            return Gesture::None; // a contact we never saw land
        };
        *p = (x, y);
        match self.pinch {
            None => Gesture::Move(x, y),
            Some((prev, latched)) if self.points.len() == 2 => {
                let (d, (mx, my)) = self.spread();
                self.pinch = Some((d, latched));
                if latched && prev > 1.0 && d > 1.0 {
                    Gesture::Zoom {
                        x: mx,
                        y: my,
                        factor: f64::from(prev / d),
                    }
                } else {
                    Gesture::None
                }
            }
            Some(_) => Gesture::None,
        }
    }

    /// A finger lifted (or its touch was cancelled).
    pub fn up(&mut self, id: u64, cancelled: bool) -> Gesture {
        if self.points.remove(&id).is_none() {
            return Gesture::None;
        }
        let Some((_, latched)) = self.pinch else {
            return if cancelled {
                Gesture::Cancel
            } else {
                Gesture::Release
            };
        };
        match self.points.len() {
            0 => self.pinch = None,
            2 => self.pinch = Some((self.spread().0, latched)),
            _ => {}
        }
        Gesture::None
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Distance between the two contacts, and their midpoint.
    fn spread(&self) -> (f32, (f32, f32)) {
        let mut it = self.points.values();
        match (it.next(), it.next()) {
            (Some(&(ax, ay)), Some(&(bx, by))) => (
                ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt(),
                ((ax + bx) / 2.0, (ay + by) / 2.0),
            ),
            _ => (0.0, (0.0, 0.0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoom(g: Gesture) -> f64 {
        match g {
            Gesture::Zoom { factor, .. } => factor,
            other => panic!("expected a zoom, got {other:?}"),
        }
    }

    #[test]
    fn one_finger_is_the_pointer() {
        let mut t = Touches::default();
        assert_eq!(t.down(1, 5.0, 5.0, |_, _| true), Gesture::Press(5.0, 5.0));
        assert_eq!(t.moved(1, 6.0, 5.0), Gesture::Move(6.0, 5.0));
        assert_eq!(t.up(1, false), Gesture::Release);
        t.down(2, 5.0, 5.0, |_, _| true);
        assert_eq!(t.up(2, true), Gesture::Cancel);
        assert!(t.is_empty());
        // Contacts we never saw are ignored.
        assert_eq!(t.moved(9, 1.0, 1.0), Gesture::None);
        assert_eq!(t.up(9, false), Gesture::None);
    }

    #[test]
    fn pinch_zooms_by_the_change_in_spread() {
        let mut t = Touches::default();
        t.down(1, 0.0, 0.0, |_, _| true);
        assert_eq!(t.down(2, 100.0, 0.0, |_, _| true), Gesture::PinchStart);
        // Spreading to 200 zooms in by half about the midpoint.
        let Gesture::Zoom { x, y, factor } = t.moved(2, 200.0, 0.0) else {
            panic!()
        };
        assert_eq!((x, y, factor), (100.0, 0.0, 0.5));
        // The finger left behind is not a pointer: no drag, no click.
        assert_eq!(t.up(1, false), Gesture::None);
        assert_eq!(t.moved(2, 150.0, 0.0), Gesture::None);
        assert_eq!(t.up(2, false), Gesture::None);
        // All lifted: the next touch is the pointer again.
        assert_eq!(t.down(3, 1.0, 1.0, |_, _| true), Gesture::Press(1.0, 1.0));
    }

    #[test]
    fn pinch_must_start_on_the_canvas_and_stays_latched() {
        let mut t = Touches::default();
        t.down(1, 0.0, 0.0, |_, _| false);
        t.down(2, 100.0, 0.0, |_, _| false);
        assert_eq!(t.moved(2, 200.0, 0.0), Gesture::None);
        t.up(1, false);
        t.up(2, false);
        // Started on the canvas: keeps zooming wherever the midpoint goes.
        t.down(1, 0.0, 0.0, |_, _| true);
        t.down(2, 100.0, 0.0, |mx, _| mx < 60.0);
        assert_eq!(zoom(t.moved(2, 400.0, 0.0)), 0.25);
    }

    #[test]
    fn a_third_finger_suspends_and_the_pair_is_remeasured() {
        let mut t = Touches::default();
        t.down(1, 0.0, 0.0, |_, _| true);
        t.down(2, 100.0, 0.0, |_, _| true);
        assert_eq!(t.down(3, 50.0, 300.0, |_, _| true), Gesture::None);
        assert_eq!(t.moved(2, 500.0, 0.0), Gesture::None);
        // Lift finger 1: the pair is now 2 and 3, measured as they stand,
        // so doubling their distance is exactly a 2× zoom (not a jump from
        // the old pair's spread).
        assert_eq!(t.up(1, false), Gesture::None);
        let (bx, by) = (500.0 + 2.0 * (50.0 - 500.0), 2.0 * 300.0);
        assert!((zoom(t.moved(3, bx, by)) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn degenerate_spreads_do_nothing() {
        let mut t = Touches::default();
        t.down(1, 10.0, 10.0, |_, _| true);
        t.down(2, 10.0, 10.0, |_, _| true);
        assert_eq!(t.moved(2, 50.0, 10.0), Gesture::None);
        assert_eq!(t.moved(2, 10.0, 10.0), Gesture::None);
    }
}
