//! Nonblocking OSC 11 reply filter over Crossterm events. Unmatched input is replayed.
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};
#[derive(Default)]
pub struct BackgroundReply {
    buffered: Vec<Event>,
    started: Option<Instant>,
}
impl BackgroundReply {
    pub fn event(&mut self, event: Event) -> (Vec<Event>, Option<[u8; 3]>) {
        if self.buffered.is_empty() {
            if matches!(&event,Event::Key(KeyEvent{code:KeyCode::Char(']'),modifiers,..}) if *modifiers==KeyModifiers::ALT)
            {
                self.started = Some(Instant::now());
                self.buffered.push(event);
                return (vec![], None);
            }
            return (vec![event], None);
        }
        let end = matches!(&event,Event::Key(KeyEvent{code:KeyCode::Char('\\'),modifiers,..}) if *modifiers==KeyModifiers::ALT)
            || matches!(&event,Event::Key(KeyEvent{code:KeyCode::Char('g'),modifiers,..}) if *modifiers==KeyModifiers::CONTROL);
        let character = if let Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers,
            ..
        }) = &event
        {
            if modifiers.is_empty() { Some(*c) } else { None }
        } else {
            None
        };
        self.buffered.push(event);
        let text = self
            .buffered
            .iter()
            .skip(1)
            .take(self.buffered.len() - 1 - usize::from(end))
            .filter_map(|e| {
                if let Event::Key(KeyEvent {
                    code: KeyCode::Char(c),
                    ..
                }) = e
                {
                    Some(*c)
                } else {
                    None
                }
            })
            .collect::<String>();
        if end {
            let color = parse(&text);
            let input = if color.is_some() {
                self.buffered.clear();
                vec![]
            } else {
                std::mem::take(&mut self.buffered)
            };
            self.started = None;
            return (input, color);
        }
        if character.is_none()
            || self.buffered.len() > 80
            || !("11;rgb:".starts_with(&text) || text.starts_with("11;rgb:"))
        {
            self.started = None;
            return (std::mem::take(&mut self.buffered), None);
        }
        (vec![], None)
    }
    pub fn expire(&mut self) -> Vec<Event> {
        if self
            .started
            .is_some_and(|at| at.elapsed() >= Duration::from_millis(150))
        {
            self.started = None;
            std::mem::take(&mut self.buffered)
        } else {
            vec![]
        }
    }
}
fn parse(value: &str) -> Option<[u8; 3]> {
    let values = value
        .strip_prefix("11;rgb:")?
        .split('/')
        .map(|s| {
            if s.is_empty() || s.len() > 4 {
                return None;
            }
            let n = u32::from_str_radix(s, 16).ok()?;
            Some((n * 255 / ((1u32 << (4 * s.len())) - 1)) as u8)
        })
        .collect::<Option<Vec<_>>>()?;
    values.try_into().ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn key(c: char, m: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), m))
    }
    #[test]
    fn complete_reply_is_consumed_and_returns_actual_rgb_without_input_keys() {
        let mut f = BackgroundReply::default();
        assert!(f.event(key(']', KeyModifiers::ALT)).0.is_empty());
        for c in "11;rgb:1010/2020/ffff".chars() {
            assert!(f.event(key(c, KeyModifiers::NONE)).0.is_empty());
        }
        let (input, color) = f.event(key('\\', KeyModifiers::ALT));
        assert!(input.is_empty());
        assert_eq!(color, Some([16, 32, 255]));
    }
    #[test]
    fn unmatched_keyboard_input_is_never_lost() {
        let mut f = BackgroundReply::default();
        let a = key(']', KeyModifiers::ALT);
        let b = key('x', KeyModifiers::NONE);
        f.event(a.clone());
        assert_eq!(f.event(b.clone()).0, vec![a, b]);
        assert_eq!(f.event(key('z', KeyModifiers::NONE)).0.len(), 1);
    }
    #[test]
    fn supports_bel_reply_and_invalid_color_replays_every_event() {
        let mut f = BackgroundReply::default();
        f.event(key(']', KeyModifiers::ALT));
        for c in "11;rgb:10/20/ff".chars() {
            f.event(key(c, KeyModifiers::NONE));
        }
        assert_eq!(
            f.event(key('g', KeyModifiers::CONTROL)).1,
            Some([16, 32, 255])
        );
        assert_eq!(parse("11;rgb:ffff/0000/nope"), None);
    }
}
