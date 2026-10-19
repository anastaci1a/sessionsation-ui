//! Terminal-independent gesture tracking and axis-independent selection movement.
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use std::time::{Duration, Instant};

/// Native repeat/release events are exact. Legacy arrow bursts have no release
/// bytes, so a quiet gap separates gestures conservatively.
#[derive(Default)]
pub struct Navigation {
    last: Option<(KeyCode, Instant)>,
}
impl Navigation {
    pub fn key(&mut self, key: KeyEvent) -> bool {
        self.key_at(key, Instant::now())
    }
    pub(crate) fn key_at(&mut self, key: KeyEvent, now: Instant) -> bool {
        if key.kind == KeyEventKind::Release {
            if self.last.is_some_and(|(code, _)| code == key.code) {
                self.last = None;
            }
            return false;
        }
        if !matches!(
            key.code,
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right
        ) {
            self.last = None;
            return true;
        }
        let fresh = key.kind == KeyEventKind::Press
            && self.last.is_none_or(|(last, at)| {
                last != key.code || now.saturating_duration_since(at) >= Duration::from_millis(150)
            });
        self.last = Some((key.code, now));
        fresh
    }
}

/// A held direction may wrap once, only when its gesture began at that edge.
#[derive(Clone, Debug, Default)]
pub struct SelectionNavigation {
    direction: Option<bool>,
    edge: bool,
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
        fresh: bool,
        focusable: impl Fn(usize) -> bool,
    ) -> usize {
        if count == 0 {
            return selected;
        }
        let selected = selected.min(count - 1);
        let first = (0..count).find(|i| focusable(*i));
        let last = (0..count).rfind(|i| focusable(*i));
        if fresh {
            self.direction = Some(previous);
            self.edge = Some(selected) == if previous { first } else { last };
        }
        let wrap = self.direction == Some(previous) && self.edge;
        let mut probe = selected;
        for _ in 0..count {
            if !wrap && (previous && probe == 0 || !previous && probe + 1 == count) {
                return selected;
            }
            if previous && probe == 0 || !previous && probe + 1 == count {
                // Crossing consumes this gesture's permission, for either axis.
                self.edge = false;
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
    use crossterm::event::KeyModifiers;
    #[test]
    fn every_axis_moves_immediately_and_consumes_its_single_wrap() {
        for code in [KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right] {
            let previous = matches!(code, KeyCode::Up | KeyCode::Left);
            for origin in 0..4 {
                let mut input = Navigation::default();
                let mut selection = SelectionNavigation::default();
                let mut selected = origin;
                let now = Instant::now();
                let press = KeyEvent::new(code, KeyModifiers::NONE);
                let mut repeat = press;
                repeat.kind = KeyEventKind::Repeat;
                for i in 0..20 {
                    let fresh = input.key_at(
                        if i == 0 { press } else { repeat },
                        now + Duration::from_millis(i * 30),
                    );
                    selected = selection.step(selected, 4, previous, fresh, |_| true);
                    if i == 0 {
                        assert_ne!(selected, origin);
                    }
                }
                assert_eq!(selected, if previous { 0 } else { 3 });
                let mut release = press;
                release.kind = KeyEventKind::Release;
                input.key_at(release, now + Duration::from_millis(601));
                selected = selection.step(
                    selected,
                    4,
                    previous,
                    input.key_at(press, now + Duration::from_millis(602)),
                    |_| true,
                );
                assert_eq!(selected, if previous { 3 } else { 0 });
            }
        }
    }
    #[test]
    fn legacy_quiet_tap_rearms_without_delaying_movement() {
        let mut input = Navigation::default();
        let now = Instant::now();
        let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert!(input.key_at(key, now));
        assert!(!input.key_at(key, now + Duration::from_millis(30)));
        assert!(input.key_at(key, now + Duration::from_millis(200)));
    }
}
