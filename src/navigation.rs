//! Terminal-independent key gestures and axis-independent selection movement.
//!
//! Every key press is delivered immediately; nothing is deferred or dropped.
//! Wrapping past an edge is limited per gesture instead: only the first press of
//! a gesture may wrap, and only when it is at the edge in its direction. Every
//! later step of the same hold stops at the edge it reaches, and native repeat
//! events never wrap. Legacy terminals such as Termux, SSH links and HERDR report
//! a held arrow as a stream of presses without repeat or release events; such a
//! hold wraps at most once, on its first press, and never spins around.
//! A legacy press continues a hold only when its timing matches keyboard
//! auto-repeat: the first repeat after the OS repeat delay (at least
//! `REPEAT_DELAY_MIN`, at most `FIRST_REPEAT_GAP` after the press that began the
//! hold), later repeats (and SSH bursts) at most `REPEAT_INTERVAL_MAX` apart.
//! Deliberate taps fall in between, so two quick taps at an edge both move and
//! the second may wrap.
//! These windows apply only to legacy input. Once the terminal has reported a
//! release or repeat event, a hold arrives as repeats and every press follows a
//! release, so each press begins a fresh gesture.
use crossterm::event::{KeyEvent, KeyEventKind};
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

/// Steady auto-repeat (and SSH bursts of it): presses of one direction at most
/// this far apart continue one hold.
pub const REPEAT_INTERVAL_MAX: Duration = Duration::from_millis(110);
/// Lower bound of a keyboard's initial auto-repeat delay: a press this long or
/// longer after the press that began a hold may be its first repeat. A press
/// sooner than this (and slower than steady repeat) is a deliberate tap.
pub const REPEAT_DELAY_MIN: Duration = Duration::from_millis(250);
/// Upper bound of a keyboard's initial auto-repeat delay: the first repeat of a
/// legacy hold may lag the press that began it by up to this much.
pub const FIRST_REPEAT_GAP: Duration = Duration::from_millis(1200);

thread_local! {
    /// Set once the terminal reports release or repeat events (input thread).
    static EVENT_TYPES: Cell<bool> = const { Cell::new(false) };
}
/// Whether key event types are reported, so every press begins a gesture.
pub fn event_types_reported() -> bool {
    EVENT_TYPES.with(Cell::get)
}
pub fn set_event_types_reported(reported: bool) {
    EVENT_TYPES.with(|c| c.set(reported));
}

#[derive(Default)]
pub struct Navigation;
impl Navigation {
    /// Each event with its wrap permission (`true` for presses, `false` for
    /// native repeats and releases). No event is held back.
    pub fn events(&mut self, key: KeyEvent) -> Vec<(KeyEvent, bool)> {
        if key.kind != KeyEventKind::Press {
            set_event_types_reported(true);
        }
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

/// Wrap bookkeeping for one list: the current hold's direction and last step.
#[derive(Clone, Debug, Default)]
pub struct SelectionNavigation {
    direction: Option<bool>,
    last: Option<Instant>,
    /// The last step was a native repeat: the terminal reports holds as
    /// repeats, so a press after one follows a release and begins a gesture.
    repeated: bool,
    /// The last step began a gesture, so the next may be its delayed first repeat.
    started: bool,
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
            && !(press && (self.repeated || event_types_reported()))
            && self.last.is_some_and(|at| {
                let gap = now.saturating_duration_since(at);
                gap <= REPEAT_INTERVAL_MAX
                    || self.started && (REPEAT_DELAY_MIN..FIRST_REPEAT_GAP).contains(&gap)
            });
        self.started = !continuing;
        self.repeated = !press;
        self.direction = Some(previous);
        self.last = Some(now);
        // Only a gesture's first press may wrap; the rest of a hold clamps.
        let wrap = press && !continuing;
        let selected = selected.min(count - 1);
        let mut probe = selected;
        for _ in 0..count {
            let edge = previous && probe == 0 || !previous && probe + 1 == count;
            if edge && !wrap {
                return selected;
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
        // Nine rows, Down × 10 from the top: separate taps wrap on the ninth
        // press; a rapid Termux stream is one hold and stops at the bottom.
        for gap in [0, 3, 50, 100, 250, 400] {
            let held = gap <= REPEAT_INTERVAL_MAX.as_millis() as u64;
            let path = presses(9, 0, false, 10, gap, true);
            assert_eq!(path[..8], [1, 2, 3, 4, 5, 6, 7, 8], "gap {gap}");
            assert_eq!(path[8], if held { 8 } else { 0 }, "gap {gap}");
            let path = presses(9, 8, true, 10, gap, true);
            assert_eq!(path[8], if held { 0 } else { 8 }, "Up gap {gap}");
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
    /// Replays legacy presses at the given millisecond offsets.
    fn timed(rows: usize, start: usize, steps: &[(bool, u64)]) -> Vec<usize> {
        let mut nav = SelectionNavigation::default();
        let now = Instant::now();
        let mut selected = start;
        steps
            .iter()
            .map(|(previous, at)| {
                selected = nav.step_at(
                    selected,
                    rows,
                    *previous,
                    true,
                    now + Duration::from_millis(*at),
                    |_| true,
                );
                selected
            })
            .collect()
    }
    /// A legacy hold: the press, the first repeat after the keyboard's repeat
    /// delay, then repeats every `rate_ms`.
    fn hold(previous: bool, delay_ms: u64, rate_ms: u64, count: u64) -> Vec<(bool, u64)> {
        (0..count)
            .map(|i| match i {
                0 => (previous, 0),
                _ => (previous, delay_ms + (i - 1) * rate_ms),
            })
            .collect()
    }
    #[test]
    fn a_long_hold_wraps_once_then_stops_at_the_opposite_edge() {
        for delay in [250, 400, 600, 1000] {
            // Up held on the top row of ten: wrap, climb back to the top, stop.
            let path = timed(10, 0, &hold(true, delay, 33, 40));
            assert_eq!(path[..10], [9, 8, 7, 6, 5, 4, 3, 2, 1, 0], "delay {delay}");
            assert!(path[9..].iter().all(|r| *r == 0), "Up {delay}: {path:?}");
            // Down held on the bottom row: wrap, descend to the bottom, stop.
            let path = timed(10, 9, &hold(false, delay, 33, 40));
            assert_eq!(path[..10], [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], "delay {delay}");
            assert!(path[9..].iter().all(|r| *r == 9), "Down {delay}: {path:?}");
        }
    }
    #[test]
    fn quick_distinct_taps_at_an_edge_each_wrap() {
        // Up on the top row, Down on the bottom row, Up again: every tap wraps.
        let path = timed(5, 0, &[(true, 0), (false, 200), (true, 400), (false, 600)]);
        assert_eq!(path, [4, 0, 4, 0]);
        // A slow tap after a finished hold wraps again too.
        let mut steps = hold(true, 500, 30, 20);
        steps.push((true, 2000));
        let path = timed(4, 0, &steps);
        assert_eq!(path[19], 0, "{path:?}");
        assert_eq!(path[20], 3, "a fresh tap wraps: {path:?}");
    }
    #[test]
    fn ssh_bursts_of_a_hold_wrap_once() {
        // A held Up relayed over SSH: the press, then bursts of five
        // simultaneous presses every 100 ms.
        let mut steps = vec![(true, 0)];
        for burst in 0..6u64 {
            steps.extend((0..5).map(|_| (true, 450 + burst * 100)));
        }
        let path = timed(8, 0, &steps);
        assert_eq!(path[0], 7, "the first press wraps");
        assert_eq!(path[7], 0);
        assert!(path[7..].iter().all(|r| *r == 0), "{path:?}");
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
    #[test]
    fn a_hold_from_a_non_edge_row_stops_at_the_edge_it_reaches() {
        for delay in [250, 400, 600, 1000] {
            // Up held from the bottom row of ten climbs to the top and stops.
            let path = timed(10, 9, &hold(true, delay, 33, 40));
            assert_eq!(path[..9], [8, 7, 6, 5, 4, 3, 2, 1, 0], "delay {delay}");
            assert!(path[8..].iter().all(|r| *r == 0), "Up {delay}: {path:?}");
            // Down held from the top row descends to the bottom and stops.
            let path = timed(10, 0, &hold(false, delay, 33, 40));
            assert_eq!(path[..9], [1, 2, 3, 4, 5, 6, 7, 8, 9], "delay {delay}");
            assert!(path[8..].iter().all(|r| *r == 9), "Down {delay}: {path:?}");
            // From the middle, in both directions.
            let path = timed(10, 5, &hold(true, delay, 33, 40));
            assert_eq!(path[..5], [4, 3, 2, 1, 0], "mid Up {delay}");
            assert!(
                path[4..].iter().all(|r| *r == 0),
                "mid Up {delay}: {path:?}"
            );
            let path = timed(10, 5, &hold(false, delay, 33, 40));
            assert_eq!(path[..4], [6, 7, 8, 9], "mid Down {delay}");
            assert!(
                path[3..].iter().all(|r| *r == 9),
                "mid Down {delay}: {path:?}"
            );
        }
        // A short list whose edge is reached by the delayed first repeat.
        let path = timed(2, 1, &hold(true, 600, 33, 10));
        assert!(path.iter().all(|r| *r == 0), "{path:?}");
    }
    #[test]
    fn ssh_bursts_of_a_hold_from_a_non_edge_row_do_not_wrap() {
        let mut steps = vec![(false, 0)];
        for burst in 0..6u64 {
            steps.extend((0..5).map(|_| (false, 450 + burst * 100)));
        }
        let path = timed(8, 2, &steps);
        assert_eq!(path[..6], [3, 4, 5, 6, 7, 7]);
        assert!(path[4..].iter().all(|r| *r == 7), "{path:?}");
        let steps: Vec<_> = steps.iter().map(|(_, at)| (true, *at)).collect();
        let path = timed(8, 4, &steps);
        assert!(path[4..].iter().all(|r| *r == 0), "{path:?}");
    }
    #[test]
    fn a_fresh_gesture_after_a_clamped_hold_wraps() {
        // Hold Down from the top to the bottom, release, then tap Down.
        let mut steps = hold(false, 500, 30, 20);
        steps.push((false, 3000));
        let path = timed(5, 0, &steps);
        assert_eq!(path[19], 4, "{path:?}");
        assert_eq!(path[20], 0, "a fresh tap wraps: {path:?}");
    }
    #[test]
    fn two_quick_deliberate_taps_at_an_edge_both_wrap_once_releases_are_reported() {
        // A Down tap reaches the bottom row, a second one 500 ms later wraps:
        // with release events each tap is its own gesture.
        set_event_types_reported(true);
        let path = timed(4, 2, &[(false, 0), (false, 500)]);
        assert_eq!(path, [3, 0], "the second deliberate tap wraps");
        let path = timed(4, 1, &[(true, 0), (true, 500), (true, 1000)]);
        assert_eq!(path, [0, 3, 2]);
        let mut nav = SelectionNavigation::default();
        let now = Instant::now();
        let s = nav.step_at(3, 4, false, true, now, |_| true);
        assert_eq!(s, 0);
        let s = nav.step_at(3, 4, false, true, now + Duration::from_millis(500), |_| {
            true
        });
        assert_eq!(s, 0, "the second deliberate tap at the edge wraps too");
        // Holds still stop: repeats never wrap.
        let s = nav.step_at(3, 4, false, false, now + Duration::from_millis(530), |_| {
            true
        });
        assert_eq!(s, 3);
        // Legacy input keeps the long first-repeat window.
        set_event_types_reported(false);
        let path = timed(4, 2, &[(false, 0), (false, 500)]);
        assert_eq!(
            path,
            [3, 3],
            "a legacy press 500 ms later may be the first repeat"
        );
        let mut input = Navigation;
        let mut release = KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        );
        release.kind = KeyEventKind::Release;
        input.events(release);
        assert!(event_types_reported(), "a release event switches modes");
        set_event_types_reported(false);
    }
    #[test]
    fn two_quick_taps_from_next_to_an_edge_reach_it_then_wrap() {
        // Legacy input (no release events): taps 120 ms apart are deliberate.
        for gap in [120, 150, 200] {
            let path = timed(6, 1, &[(true, 0), (true, gap)]);
            assert_eq!(path, [0, 5], "Up from the second row, {gap} ms: {path:?}");
            let path = timed(6, 4, &[(false, 0), (false, gap)]);
            assert_eq!(path, [5, 0], "Down from the second-to-last row, {gap} ms");
        }
        // Three quick taps from the second row: top, wrap, then one more step.
        let path = timed(6, 1, &[(true, 0), (true, 120), (true, 240)]);
        assert_eq!(path, [0, 5, 4]);
    }
    #[test]
    fn a_realistic_held_key_from_mid_list_stops_at_the_edge() {
        // First repeat after a 400 ms OS delay, then steady 33 ms repeats.
        let path = timed(8, 4, &hold(true, 400, 33, 30));
        assert_eq!(path[..5], [3, 2, 1, 0, 0]);
        assert!(path[3..].iter().all(|r| *r == 0), "Up: {path:?}");
        let path = timed(8, 3, &hold(false, 400, 33, 30));
        assert_eq!(path[..5], [4, 5, 6, 7, 7]);
        assert!(path[4..].iter().all(|r| *r == 7), "Down: {path:?}");
    }
    #[test]
    fn a_hold_starting_at_an_edge_wraps_exactly_once() {
        let path = timed(8, 0, &hold(true, 400, 33, 40));
        assert_eq!(path[0], 7, "the first press wraps");
        assert_eq!(path.iter().filter(|r| **r == 7).count(), 1, "{path:?}");
        assert!(path[7..].iter().all(|r| *r == 0), "{path:?}");
        let path = timed(8, 7, &hold(false, 400, 33, 40));
        assert_eq!(path[0], 0);
        assert_eq!(path.iter().filter(|r| **r == 0).count(), 1, "{path:?}");
        assert!(path[7..].iter().all(|r| *r == 7), "{path:?}");
    }
    #[test]
    fn a_press_after_native_repeats_is_a_fresh_gesture() {
        // Enhanced terminals: press, repeats to the bottom, release, press.
        let mut nav = SelectionNavigation::default();
        let now = Instant::now();
        let mut s = nav.step_at(3, 5, false, true, now, |_| true);
        for i in 1..10 {
            s = nav.step_at(s, 5, false, false, now + Duration::from_millis(i), |_| true);
        }
        assert_eq!(s, 4);
        let s = nav.step_at(s, 5, false, true, now + Duration::from_millis(11), |_| true);
        assert_eq!(s, 0, "the released press wraps");
    }
}
