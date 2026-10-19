//! Closing a viewer with its configured host prefix never sends input to an agent.
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
#[derive(Debug)]
pub struct Prefixes(Vec<(KeyCode, KeyModifiers)>);
impl Prefixes {
    pub fn load() -> Self {
        Self::parse(&std::fs::read_to_string(crate::host_config::path()).unwrap_or_default())
    }
    pub fn parse(text: &str) -> Self {
        let config = text.parse::<toml::Value>().ok();
        let value = config
            .as_ref()
            .and_then(|v| v.get("keys"))
            .and_then(|v| v.get("prefix"));
        let values: Vec<_> = match value {
            Some(toml::Value::String(s)) => vec![s.as_str()],
            Some(toml::Value::Array(a)) => a.iter().filter_map(toml::Value::as_str).collect(),
            _ => vec![],
        };
        let mut keys = Vec::new();
        for value in values {
            if let Some(key) = combo(value)
                && !keys.contains(&key)
            {
                keys.push(key);
            }
        }
        if keys.is_empty() {
            keys.push((KeyCode::Char('b'), KeyModifiers::CONTROL));
        }
        Self(keys)
    }
    pub fn matches(&self, event: KeyEvent) -> bool {
        event.kind != KeyEventKind::Release
            && self.0.contains(&normalize(event.code, event.modifiers))
    }
}
fn normalize(mut code: KeyCode, mut modifiers: KeyModifiers) -> (KeyCode, KeyModifiers) {
    if let KeyCode::Char(c) = code
        && c.is_ascii_uppercase()
    {
        code = KeyCode::Char(c.to_ascii_lowercase());
        modifiers.insert(KeyModifiers::SHIFT);
    }
    if code == KeyCode::Tab && modifiers.contains(KeyModifiers::SHIFT) {
        code = KeyCode::BackTab;
    }
    if code == KeyCode::BackTab {
        modifiers.remove(KeyModifiers::SHIFT);
    }
    (code, modifiers)
}
fn combo(raw: &str) -> Option<(KeyCode, KeyModifiers)> {
    let raw = raw.trim();
    let expanded = if raw == "+" {
        "plus".into()
    } else if let Some(prefix) = raw.strip_suffix("++") {
        format!("{prefix}+plus")
    } else {
        raw.to_owned()
    };
    let mut modifiers = KeyModifiers::empty();
    let mut key = None;
    for token in expanded.split('+').map(str::trim) {
        let lower = token.to_ascii_lowercase();
        let modifier = match lower.as_str() {
            "ctrl" | "control" => Some(KeyModifiers::CONTROL),
            "shift" => Some(KeyModifiers::SHIFT),
            "alt" | "option" => Some(KeyModifiers::ALT),
            "super" | "cmd" | "command" | "win" => Some(KeyModifiers::SUPER),
            "hyper" => Some(KeyModifiers::HYPER),
            "meta" => Some(KeyModifiers::META),
            _ => None,
        };
        if let Some(m) = modifier {
            modifiers.insert(m);
            continue;
        }
        if key.is_some() {
            return None;
        }
        let code = match lower.as_str() {
            "space" => KeyCode::Char(' '),
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backspace" | "bs" => KeyCode::Backspace,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "plus" => KeyCode::Char('+'),
            "minus" => KeyCode::Char('-'),
            "comma" => KeyCode::Char(','),
            "period" => KeyCode::Char('.'),
            "slash" => KeyCode::Char('/'),
            "backslash" => KeyCode::Char('\\'),
            "quote" => KeyCode::Char('\''),
            "double_quote" | "double-quote" => KeyCode::Char('"'),
            "semicolon" => KeyCode::Char(';'),
            "colon" => KeyCode::Char(':'),
            "percent" => KeyCode::Char('%'),
            "ampersand" => KeyCode::Char('&'),
            "backtick" => KeyCode::Char('`'),
            _ => {
                let mut chars = token.chars();
                let first = chars.next()?;
                if chars.next().is_none() {
                    KeyCode::Char(first)
                } else if let Some(f) = lower.strip_prefix('f') {
                    let n = f.parse().ok()?;
                    if !(1..=24).contains(&n) {
                        return None;
                    }
                    KeyCode::F(n)
                } else {
                    return None;
                }
            }
        };
        key = Some(code);
    }
    Some(normalize(key?, modifiers))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_configured_prefixes_accept_press_but_never_release() {
        let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
        for config in ["", "[keys]\nprefix=[]", "[keys]\nprefix='invalid key'"] {
            assert!(Prefixes::parse(config).matches(ctrl_b));
        }
        let keys = Prefixes::parse("[keys]\nprefix=['ctrl+space','ctrl+a','alt+x']");
        assert!(!keys.matches(ctrl_b));
        for (code, modifiers) in [
            (KeyCode::Char(' '), KeyModifiers::CONTROL),
            (KeyCode::Char('a'), KeyModifiers::CONTROL),
            (KeyCode::Char('x'), KeyModifiers::ALT),
        ] {
            let key = KeyEvent::new(code, modifiers);
            assert!(keys.matches(key));
            let mut released = key;
            released.kind = KeyEventKind::Release;
            assert!(!keys.matches(released));
        }
    }
    #[test]
    fn enhanced_and_legacy_key_forms_share_normalization() {
        assert!(
            Prefixes::parse("[keys]\nprefix='ctrl+shift+b'")
                .matches(KeyEvent::new(KeyCode::Char('B'), KeyModifiers::CONTROL))
        );
        assert!(
            Prefixes::parse("[keys]\nprefix='shift+tab'")
                .matches(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
        );
        assert!(
            Prefixes::parse("[keys]\nprefix='ctrl++'")
                .matches(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::CONTROL))
        );
        assert!(
            Prefixes::parse("[keys]\nprefix='f12'")
                .matches(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE))
        );
        assert!(
            !Prefixes::parse("[keys]\nprefix='alt+x'")
                .matches(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
        );
    }
}
