//! Reusable terminal controls. No filesystem, provider, host or application policy.
use crate::render::{Canvas, Cell};
use crossterm::event::KeyCode;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, Default)]
pub struct TextInput {
    pub value: String,
    /// UTF-8 byte offset at a grapheme boundary.
    pub cursor: usize,
}
impl TextInput {
    pub fn new(value: String) -> Self {
        let cursor = value.len();
        Self { value, cursor }
    }
    /// Bracketed paste and typing use the same path; control bytes never activate.
    pub fn insert(&mut self, text: &str) {
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        self.value.insert_str(self.cursor, &clean);
        self.cursor += clean.len();
        // Combining characters can merge across the insertion point.
        self.cursor = self
            .value
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(self.value.len()))
            .find(|i| *i >= self.cursor)
            .unwrap_or(self.value.len());
    }
    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }
    pub fn edit(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Left | KeyCode::Backspace => {
                let start = self.value[..self.cursor]
                    .grapheme_indices(true)
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                if key == KeyCode::Backspace {
                    self.value.replace_range(start..self.cursor, "");
                }
                self.cursor = start;
            }
            KeyCode::Right => {
                if let Some(g) = self.value[self.cursor..].graphemes(true).next() {
                    self.cursor += g.len();
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.value.len(),
            KeyCode::Delete => {
                if let Some(g) = self.value[self.cursor..].graphemes(true).next() {
                    let end = self.cursor + g.len();
                    self.value.replace_range(self.cursor..end, "");
                }
            }
            _ => return false,
        }
        true
    }
    pub fn paint(&self, canvas: &mut Canvas, x: usize, y: usize, width: usize, valid: bool) {
        if width < 3 {
            return;
        }
        let style = Cell {
            fg: if valid { [238; 3] } else { [215, 0, 0] },
            ..Cell::default()
        };
        let before = &self.value[..self.cursor];
        let skip = UnicodeWidthStr::width(before).saturating_sub(width - 3);
        let mut removed = 0;
        let visible: String = self
            .value
            .graphemes(true)
            .skip_while(|g| {
                if removed < skip {
                    removed += UnicodeWidthStr::width(*g);
                    true
                } else {
                    false
                }
            })
            .scan(0, |used, g| {
                *used += UnicodeWidthStr::width(g);
                (*used <= width - 2).then_some(g)
            })
            .collect();
        canvas.text(x, y, &format!("> {visible}"), style.clone(), true);
        let cursor = x + 2 + UnicodeWidthStr::width(before).saturating_sub(removed);
        if cursor < x + width {
            let glyph = self.value[self.cursor..]
                .graphemes(true)
                .next()
                .unwrap_or(" ");
            canvas.text(
                cursor,
                y,
                glyph,
                Cell {
                    fg: [0; 3],
                    bg: Some([238; 3]),
                    ..style
                },
                false,
            );
        }
    }
}

#[derive(Clone, Debug)]
pub struct Row {
    pub label: String,
    pub enabled: bool,
}
impl Row {
    pub fn yes(label: impl Into<String>) -> Self {
        Self::gate(label, true)
    }
    pub fn gate(label: impl Into<String>, enabled: bool) -> Self {
        Self {
            label: label.into(),
            enabled,
        }
    }
    pub fn paint(&self, canvas: &mut Canvas, x: usize, y: usize, selected: bool, accent: [u8; 3]) {
        canvas.text(
            x,
            y,
            if selected { "●" } else { "○" },
            Cell {
                fg: if self.enabled {
                    accent
                } else {
                    accent.map(|v| (v as u16 * 60 / 100) as u8)
                },
                bold: selected,
                strike: !self.enabled,
                ..Cell::default()
            },
            false,
        );
        canvas.text(
            x + 2,
            y,
            &self.label,
            Cell {
                fg: if !self.enabled {
                    [108; 3]
                } else if selected {
                    [238; 3]
                } else {
                    [188; 3]
                },
                bold: selected,
                strike: !self.enabled,
                ..Cell::default()
            },
            true,
        );
    }
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub selected: usize,
    pub back: bool,
    pub scroll: usize,
}
impl Options {
    /// Optional bottom action participates in focus without scrolling the list.
    pub fn step(&mut self, rows: &[Row], bottom_action: bool, previous: bool) {
        let count = rows.len() + usize::from(bottom_action);
        if count == 0 {
            return;
        }
        self.back = false;
        self.selected %= count;
        for _ in 0..count {
            self.selected = if previous {
                (self.selected + count - 1) % count
            } else {
                (self.selected + 1) % count
            };
            if self.selected == rows.len() || rows[self.selected].enabled {
                break;
            }
        }
    }
    pub fn paint(
        &mut self,
        canvas: &mut Canvas,
        rows: &[Row],
        top: usize,
        bottom: usize,
        accent: [u8; 3],
    ) -> (usize, usize) {
        let capacity = bottom.saturating_sub(top);
        if self.selected < rows.len() {
            if self.selected < self.scroll {
                self.scroll = self.selected;
            }
            if capacity > 0 && self.selected >= self.scroll + capacity {
                self.scroll = self.selected + 1 - capacity;
            }
        }
        self.scroll = self.scroll.min(rows.len().saturating_sub(capacity));
        let longest = rows
            .iter()
            .map(|r| UnicodeWidthStr::width(r.label.as_str()))
            .max()
            .unwrap_or(0)
            .min(canvas.width.saturating_sub(4));
        let left = canvas.width.saturating_sub(longest + 2) / 2;
        for (i, row) in rows.iter().enumerate().skip(self.scroll).take(capacity) {
            row.paint(
                canvas,
                left,
                top + i - self.scroll,
                i == self.selected && !self.back,
                accent,
            );
        }
        (left, capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editor_moves_and_erases_whole_graphemes_and_paste_cannot_submit() {
        let mut input = TextInput::new("a界e\u{301}".into());
        input.edit(KeyCode::Backspace);
        assert_eq!(input.value, "a界");
        input.edit(KeyCode::Left);
        input.insert("\nX\x1b");
        assert_eq!(input.value, "aX界");
        input.edit(KeyCode::Delete);
        assert_eq!(input.value, "aX");
    }
    #[test]
    fn disabled_rows_skip_and_bottom_action_does_not_scroll_background() {
        let rows = vec![Row::yes("one"), Row::gate("two", false), Row::yes("three")];
        let mut options = Options::default();
        options.step(&rows, true, false);
        assert_eq!(options.selected, 2);
        let mut canvas = Canvas::new(24, 10);
        canvas.cells[9 * 24].glyph = "*".into();
        options.paint(&mut canvas, &rows, 3, 4, [200; 3]);
        assert_eq!(options.scroll, 2);
        options.step(&rows, true, false);
        options.paint(&mut canvas, &rows, 3, 4, [200; 3]);
        assert_eq!(options.scroll, 2);
        assert_eq!(canvas.cells[9 * 24].glyph, "*");
    }
    #[test]
    fn input_viewport_preserves_wide_characters_and_cursor_cell() {
        let input = TextInput::new("界界界界界".into());
        let mut canvas = Canvas::new(10, 1);
        input.paint(&mut canvas, 0, 0, 9, true);
        assert_eq!(canvas.cells.iter().filter(|c| c.bg.is_some()).count(), 1);
        assert_eq!(canvas.cells[9], Cell::default());
    }
}
