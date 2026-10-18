//! Reusable internal modals. Closing a viewer never cancels backend work.
use crate::{
    render::{Canvas, Cell},
    widgets::{Row, TextInput, clip},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
#[derive(Clone, Debug)]
pub struct Palette {
    pub accent: [u8; 3],
    pub panel: [u8; 3],
    pub text: [u8; 3],
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            accent: [255, 193, 140],
            panel: [16; 3],
            text: [255; 3],
        }
    }
}
#[derive(Clone, Debug)]
pub struct Field {
    pub label: String,
    pub input: TextInput,
}
impl Field {
    pub fn new(label: &str, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            input: TextInput::new(value.into()),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Dialog {
    pub title: String,
    pub body: Vec<String>,
    pub fields: Vec<Field>,
    pub buttons: Vec<Row>,
    pub selected: usize,
    pub offset: usize,
    pub visible: usize,
    pub wrapped: usize,
    pub busy: bool,
}
pub enum Event {
    None,
    Dismiss,
    Choose(usize, Vec<String>),
}
impl Dialog {
    pub fn new(title: impl Into<String>, body: Vec<String>, buttons: &[&str]) -> Self {
        Self {
            title: title.into(),
            body,
            fields: vec![],
            buttons: buttons.iter().map(|s| Row::yes(*s)).collect(),
            selected: 0,
            offset: 0,
            visible: 0,
            wrapped: 0,
            busy: false,
        }
    }
    pub fn progress(title: &str) -> Self {
        let mut d = Self::new(
            title,
            vec!["Submitting once…".into()],
            &["Continue in background"],
        );
        d.busy = true;
        d
    }
    pub fn values(&self) -> Vec<String> {
        self.fields.iter().map(|f| f.input.value.clone()).collect()
    }
    pub fn paste(&mut self, text: &str) {
        if let Some(field) = self.fields.get_mut(self.selected) {
            field.input.insert(text);
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Event {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return Event::None;
        }
        let count = self.fields.len() + self.buttons.len();
        match key.code {
            KeyCode::Esc => return Event::Dismiss,
            KeyCode::PageDown => {
                self.offset = (self.offset + self.visible.max(1))
                    .min(self.wrapped.saturating_sub(self.visible))
            }
            KeyCode::PageUp => self.offset = self.offset.saturating_sub(self.visible.max(1)),
            KeyCode::Up if count > 0 => self.selected = (self.selected + count - 1) % count,
            KeyCode::Down | KeyCode::Tab if count > 0 => {
                self.selected = (self.selected + 1) % count
            }
            KeyCode::Enter if self.selected >= self.fields.len() => {
                let button = self.selected - self.fields.len();
                if self.buttons.get(button).is_some_and(|b| b.enabled)
                    && (self.busy || self.offset + self.visible >= self.wrapped)
                {
                    return Event::Choose(button, self.values());
                }
            }
            KeyCode::Enter => self.selected = (self.selected + 1).min(count.saturating_sub(1)),
            KeyCode::Char(c) => self.paste(&c.to_string()),
            code => {
                if let Some(f) = self.fields.get_mut(self.selected) {
                    f.input.edit(code);
                }
            }
        }
        Event::None
    }
    pub fn paint(&mut self, canvas: &mut Canvas, palette: &Palette) {
        let width = canvas.width.saturating_sub(4).min(76);
        let max_height = canvas.height.saturating_sub(4);
        let reserved = self.fields.len() * 2 + self.buttons.len() + 6;
        if width < 20 || max_height < reserved + usize::from(!self.body.is_empty()) {
            canvas.text(
                0,
                1,
                "Resize to view dialog; Esc dismisses",
                Cell::default(),
                false,
            );
            return;
        }
        let x = (canvas.width - width) / 2;
        let lines: Vec<_> = self
            .body
            .iter()
            .flat_map(|l| {
                if l.is_empty() {
                    vec![String::new()]
                } else {
                    wrap(l, width - 4)
                }
            })
            .collect();
        let height = max_height.min(lines.len() + reserved);
        let y = (canvas.height - height) / 2;
        let body_height = height - reserved;
        self.visible = body_height;
        self.wrapped = lines.len();
        self.offset = self.offset.min(lines.len().saturating_sub(body_height));
        let style = Cell {
            fg: palette.text,
            bg: Some(palette.panel),
            ..Cell::default()
        };
        for row in y..y + height {
            canvas.text(x, row, &" ".repeat(width), style.clone(), false);
        }
        let border = Cell {
            fg: palette.accent,
            ..style.clone()
        };
        for row in y + 1..y + height - 1 {
            canvas.text(x, row, "│", border.clone(), false);
            canvas.text(x + width - 1, row, "│", border.clone(), false);
        }
        canvas.text(
            x,
            y,
            &format!("┌{}┐", "─".repeat(width - 2)),
            border.clone(),
            false,
        );
        canvas.text(
            x,
            y + height - 1,
            &format!("└{}┘", "─".repeat(width - 2)),
            border.clone(),
            false,
        );
        canvas.text(x + 2, y, &clip(&self.title, width - 4), border, false);
        for (i, line) in lines.iter().skip(self.offset).take(body_height).enumerate() {
            canvas.text(x + 2, y + 1 + i, line, style.clone(), false);
        }
        let mut row = y + 1 + body_height;
        for (index, field) in self.fields.iter().enumerate() {
            canvas.text(x + 2, row, &field.label, style.clone(), false);
            field
                .input
                .paint(canvas, x + 2, row + 1, width - 4, index == self.selected);
            row += 2;
        }
        row += 1;
        for (index, button) in self.buttons.iter().enumerate() {
            button.paint(
                canvas,
                x + 2,
                row + index,
                self.selected == self.fields.len() + index,
                false,
            );
        }
        let hint = if self.offset + self.visible < self.wrapped {
            "PgDn: Read more before confirming"
        } else {
            "Esc: Back   PgUp/PgDn: Scroll"
        };
        canvas.text(x + 2, y + height - 2, &clip(hint, width - 4), style, false);
        for row in y..y + height {
            for col in x..x + width {
                let cell = &mut canvas.cells[row * canvas.width + col];
                if cell.bg.is_none() {
                    cell.bg = Some(palette.panel);
                }
                if matches!(cell.glyph.as_str(), "○" | "●") {
                    cell.fg = palette.accent;
                } else if cell.fg == [238; 3] || cell.fg == [188; 3] {
                    cell.fg = palette.text;
                }
            }
        }
    }
}
// Preserve entire paths, IDs and consent text, including unbroken tokens.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut line = String::new();
    let mut used = 0;
    for g in text.graphemes(true) {
        let cells = UnicodeWidthStr::width(g);
        if g == "\n" || used + cells > width {
            lines.push(std::mem::take(&mut line));
            used = 0;
            if g == "\n" {
                continue;
            }
        }
        line.push_str(g);
        used += cells;
    }
    lines.push(line);
    lines
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consent_wrap_preserves_long_paths_and_graphemes() {
        let text = "/very/long/unbroken/path/αβγ/with-combining-é-and-👩‍💻";
        let lines = wrap(text, 12);
        assert_eq!(lines.concat(), text);
        assert!(
            lines
                .iter()
                .all(|l| UnicodeWidthStr::width(l.as_str()) <= 12)
        );
    }
    #[test]
    fn confirmation_requires_reading_all_pages_and_resize_keeps_input() {
        let mut d = Dialog::new(
            "Confirm",
            (0..50).map(|n| format!("Affected session {n}")).collect(),
            &["Confirm"],
        );
        d.fields.push(Field::new("Account ID", "codex:test"));
        d.selected = 1;
        d.paint(&mut Canvas::new(60, 24), &Palette::default());
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert!(matches!(d.key(enter), Event::None));
        for _ in 0..50 {
            d.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        }
        assert!(matches!(d.key(enter), Event::Choose(0, _)));
        d.paint(&mut Canvas::new(40, 16), &Palette::default());
        assert_eq!(d.values(), vec!["codex:test"]);
    }
}
