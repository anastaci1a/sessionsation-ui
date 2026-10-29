//! Terminal-independent key gestures and axis-independent selection movement.
//!
//! Every key press is delivered immediately; nothing is deferred or dropped.
//! Wrapping past an edge is limited per gesture instead: a press may wrap unless
//! it continues a hold (the same direction within `HOLD_GAP`) that has already
//! crossed an edge, and native repeat events never wrap. Legacy terminals such as
//! Termux report a held arrow as a stream of presses without repeat or release
//! events; such a hold crosses an edge at most once and never spins around.
use crossterm::event::{KeyEvent, KeyEventKind};
use std::time::{Duration, Instant};

/// Presses of one direction closer together than this continue one hold.
pub const HOLD_GAP: Duration = Duration::from_millis(150);

#[derive(Default)]
pub struct Navigation;
impl Navigation {
    /// Each event with its wrap permission (`true` for presses, `false` for
    /// native repeats and releases). No event is held back.
    pub fn events(&mut self, key: KeyEvent) -> Vec<(KeyEvent, bool)> {
        vec![(key, self.key_at(key, Instant::now()))]
    }
    /// Retained for the event loop's frame tick; nothing is ever pending.
    pub fn flush(&mut self) -> Vec<(KeyEvent, bool)> {
        vec![]
    }
    pub(crate) fn key_at(&mut self, key: KeyEvent, _now: Instant) -> bool {
        key.kind == KeyEventKind::Press
    }
}

/// Wrap bookkeeping for one list: the current hold's direction, its last step
/// and whether it has already crossed an edge.
#[derive(Clone, Debug, Default)]
pub struct SelectionNavigation {
    direction: Option<bool>,
    last: Option<Instant>,
    crossed: bool,
}
impl SelectionNavigation {
    pub fn boundary(
        &mut self,
        selected: usize,
        count: usize,
        last: bool,
        focusable: impl Fn(usize) -> bool,
    ) -> usize {
        *self = Self::default();
        if last {
            (0..count).rfind(|i| focusable(*i))
        } else {
            (0..count).find(|i| focusable(*i))
        }
        .unwrap_or(selected)
    }
    pub fn step(
        &mut self,
        selected: usize,
        count: usize,
        previous: bool,
        press: bool,
        focusable: impl Fn(usize) -> bool,
    ) -> usize {
        self.step_at(selected, count, previous, press, Instant::now(), focusable)
    }
    pub fn step_at(
        &mut self,
        selected: usize,
        count: usize,
        previous: bool,
        press: bool,
        now: Instant,
        focusable: impl Fn(usize) -> bool,
    ) -> usize {
        if count == 0 {
            return selected;
        }
        let continuing = self.direction == Some(previous)
            && self
                .last
                .is_some_and(|at| now.saturating_duration_since(at) < HOLD_GAP);
        if !continuing {
            self.crossed = false;
        }
        self.direction = Some(previous);
        self.last = Some(now);
        let wrap = press && !self.crossed;
        let selected = selected.min(count - 1);
        let mut probe = selected;
        for _ in 0..count {
            let edge = previous && probe == 0 || !previous && probe + 1 == count;
            if edge {
                if !wrap {
                    return selected;
                }
                // A hold crosses an edge at most once.
                self.crossed = true;
            }
            probe = if previous {
                (probe + count - 1) % count
            } else {
                (probe + 1) % count
            };
            if focusable(probe) {
                return probe;
            }
        }
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn presses(
        rows: usize,
        start: usize,
        previous: bool,
        count: usize,
        gap_ms: u64,
        press: bool,
    ) -> Vec<usize> {
        let mut nav = SelectionNavigation::default();
        let now = Instant::now();
        let mut selected = start;
        (0..count)
            .map(|i| {
                selected = nav.step_at(
                    selected,
                    rows,
                    previous,
                    press || i == 0,
                    now + Duration::from_millis(i as u64 * gap_ms),
                    |_| true,
                );
                selected
            })
            .collect()
    }
    #[test]
    fn every_separate_or_rapid_press_moves_and_one_wrap_is_allowed() {
        // Nine rows, Down × 10 from the top: the tenth press wraps to row 0,
        // whatever the human or Termux gap between presses.
        for gap in [0, 3, 50, 100, 250, 400] {
            let path = presses(9, 0, false, 10, gap, true);
            assert_eq!(path[..8], [1, 2, 3, 4, 5, 6, 7, 8], "gap {gap}");
            assert_eq!(path[8], 0, "gap {gap} wraps");
            let path = presses(9, 8, true, 10, gap, true);
            assert_eq!(path[8], 8, "Up wraps from the top too (gap {gap})");
        }
    }
    #[test]
    fn a_held_arrow_crosses_an_edge_at_most_once() {
        // Termux hold: legacy presses 20 ms apart starting on the last row.
        let path = presses(4, 3, false, 20, 20, true);
        assert_eq!(path[0], 0, "the first press at the edge wraps");
        assert_eq!(path[3], 3);
        assert!(path[3..].iter().all(|r| *r == 3), "never spins: {path:?}");
        let path = presses(4, 0, true, 20, 20, true);
        assert_eq!(path[0], 3);
        assert!(path[3..].iter().all(|r| *r == 0), "{path:?}");
        // After a pause, a new press wraps again.
        let mut nav = SelectionNavigation::default();
        let now = Instant::now();
        let s = nav.step_at(3, 4, false, true, now, |_| true);
        let s = nav.step_at(s, 4, false, true, now + Duration::from_millis(20), |_| true);
        assert_eq!(s, 1);
        let s = nav.step_at(3, 4, false, true, now + Duration::from_millis(500), |_| {
            true
        });
        assert_eq!(s, 0);
    }
    #[test]
    fn native_repeats_never_wrap_and_mid_list_presses_are_unaffected() {
        let path = presses(4, 1, false, 10, 30, false);
        assert_eq!(path, [2, 3, 3, 3, 3, 3, 3, 3, 3, 3]);
        let path = presses(6, 2, true, 2, 10, true);
        assert_eq!(path, [1, 0]);
        let mut input = Navigation;
        let key = KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        );
        let mut repeat = key;
        repeat.kind = KeyEventKind::Repeat;
        assert_eq!(input.events(key), vec![(key, true)]);
        assert_eq!(input.events(repeat), vec![(repeat, false)]);
        assert!(input.flush().is_empty(), "nothing is deferred");
    }
    #[test]
    fn unselectable_rows_are_skipped_including_across_the_wrap() {
        let mut nav = SelectionNavigation::default();
        let now = Instant::now();
        let s = nav.step_at(2, 4, false, true, now, |i| i != 3);
        assert_eq!(s, 0);
        let s = nav.step_at(1, 4, true, true, now + Duration::from_secs(1), |i| i != 0);
        assert_eq!(s, 3);
    }
}
