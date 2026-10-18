//! Reusable terminal controls. No filesystem, provider, host or application policy.
use crate::render::{Accent, Canvas, Cell};
use crossterm::event::KeyCode;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, Default)]
pub struct TextInput {
    pub value: String,
    /// UTF-8 byte offset at a grapheme boundary.
    pub cursor: usize,
    pub max_chars: Option<usize>,
}
impl TextInput {
    pub fn new(value: String) -> Self {
        let cursor = value.len();
        Self {
            value,
            cursor,
            max_chars: None,
        }
    }
    pub fn limited(mut self, max_chars: usize) -> Self {
        self.max_chars = Some(max_chars);
        self
    }
    /// Bracketed paste and typing use the same path; control bytes never activate.
    pub fn insert(&mut self, text: &str) {
        let available = self
            .max_chars
            .map(|n| n.saturating_sub(self.value.chars().count()))
            .unwrap_or(usize::MAX);
        let clean: String = text
            .chars()
            .filter(|c| !c.is_control())
            .take(available)
            .collect();
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
        self.paint_focused(canvas, x, y, width, valid, true);
    }
    pub fn paint_focused(
        &self,
        canvas: &mut Canvas,
        x: usize,
        y: usize,
        width: usize,
        valid: bool,
        focused: bool,
    ) {
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
        if focused && cursor < x + width {
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
                    bg: Some(style.fg),
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
    pub preview: Option<String>,
    pub right_preview: bool,
    pub preview_selected_only: bool,
    pub preview_column: Option<usize>,
    pub preview_accent: bool,
    pub separator: bool,
}
impl Row {
    pub fn yes(label: impl Into<String>) -> Self {
        Self::gate(label, true)
    }
    pub fn gate(label: impl Into<String>, enabled: bool) -> Self {
        Self {
            label: label.into(),
            enabled,
            preview: None,
            right_preview: false,
            preview_selected_only: false,
            preview_column: None,
            preview_accent: false,
            separator: false,
        }
    }
    pub fn with_preview(mut self, preview: impl Into<String>) -> Self {
        self.preview = Some(preview.into());
        self
    }
    pub fn with_right_preview(mut self, preview: impl Into<String>) -> Self {
        self.preview = Some(preview.into());
        self.right_preview = true;
        self
    }
    pub fn with_column_preview(
        mut self,
        preview: impl Into<String>,
        column: usize,
        accent: bool,
    ) -> Self {
        self.preview = Some(preview.into());
        self.preview_column = Some(column);
        self.preview_accent = accent;
        self
    }
    pub fn with_selected_preview(mut self, preview: impl Into<String>) -> Self {
        self.preview = Some(preview.into());
        self.preview_selected_only = true;
        self
    }
    pub fn preview_position(&self, width: usize, x: usize) -> Option<usize> {
        let preview = self.preview.as_deref()?;
        let minimum = x + 2 + UnicodeWidthStr::width(self.label.as_str()) + 2;
        let start = if let Some(column) = self.preview_column {
            x + column
        } else if self.right_preview {
            width.saturating_sub(2 + UnicodeWidthStr::width(preview))
        } else {
            minimum
        };
        (start >= minimum && start + UnicodeWidthStr::width(preview) <= width.saturating_sub(2))
            .then_some(start)
    }
    pub fn separator() -> Self {
        Self {
            label: String::new(),
            enabled: false,
            preview: None,
            right_preview: false,
            preview_selected_only: false,
            preview_column: None,
            preview_accent: false,
            separator: true,
        }
    }
    pub fn paint(&self, canvas: &mut Canvas, x: usize, y: usize, selected: bool, back: bool) {
        if self.separator {
            canvas.accent_text(
                x,
                y,
                "•",
                Accent {
                    brightness: 600,
                    flash: false,
                    ..Accent::default()
                },
            );
            return;
        }
        canvas.accent_text(
            x,
            y,
            if selected { "●" } else { "○" },
            Accent {
                brightness: if self.enabled && !back { 1000 } else { 600 },
                bold: selected,
                strike: !self.enabled,
                ..Accent::default()
            },
        );
        canvas.text(x + 1, y, " ", Cell::default(), true);
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
            self.enabled,
        );
        if let Some(preview) = &self.preview
            && (!self.preview_selected_only || (selected && !back))
            && let Some(start) = self.preview_position(canvas.width, x)
        {
            if self.preview_accent {
                canvas.accent_text(
                    start,
                    y,
                    preview,
                    Accent {
                        brightness: if selected && !back { 1000 } else { 600 },
                        bold: selected && !back,
                        ..Accent::default()
                    },
                );
            } else {
                canvas.text(
                    start,
                    y,
                    preview,
                    Cell {
                        fg: if self.enabled { [188; 3] } else { [108; 3] },
                        ..Cell::default()
                    },
                    false,
                );
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub selected: usize,
    pub back: bool,
    pub scroll: usize,
}

/// Shared bottom hints: token wrapping never changes the star-field geometry.
pub fn paint_hint(canvas: &mut Canvas, x: usize, y: usize, text: &str) {
    canvas.accent_text(
        x,
        y,
        text,
        Accent {
            soft: true,
            brightness: 550,
            flash: false,
            ..Accent::default()
        },
    );
}
pub fn paint_hints(canvas: &mut Canvas, hints: &[String]) {
    let width = hints
        .iter()
        .map(|s| UnicodeWidthStr::width(s.as_str()))
        .max()
        .unwrap_or(0);
    let left = canvas.width.saturating_sub(width) / 2;
    let start = canvas.height.saturating_sub(hints.len());
    for (i, hint) in hints.iter().enumerate() {
        paint_hint(canvas, left, start + i, hint);
    }
}
pub fn hint_lines(tokens: &[&str], width: usize) -> Vec<String> {
    let available = width.saturating_sub(4).max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for token in tokens {
        let mut clipped = String::new();
        let mut used = 0;
        for glyph in token.graphemes(true) {
            let w = UnicodeWidthStr::width(glyph);
            if used + w > available {
                break;
            }
            clipped.push_str(glyph);
            used += w;
        }
        if !line.is_empty() && UnicodeWidthStr::width(line.as_str()) + 3 + used > available {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push_str("   ");
        }
        line.push_str(&clipped);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
pub fn clip(text: &str, width: usize) -> String {
    let mut used = 0;
    text.graphemes(true)
        .take_while(|g| {
            used += UnicodeWidthStr::width(*g);
            used <= width
        })
        .collect()
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
    pub fn left(width: usize, rows: &[Row]) -> usize {
        let longest = rows
            .iter()
            .map(|r| UnicodeWidthStr::width(r.label.as_str()))
            .max()
            .unwrap_or(0)
            .min(width.saturating_sub(8));
        let mut left = width.saturating_sub(longest + 2) / 2;
        if let Some(right) = rows
            .iter()
            .filter(|r| r.right_preview)
            .filter_map(|r| r.preview.as_deref())
            .map(UnicodeWidthStr::width)
            .max()
        {
            left = left.min(width.saturating_sub(longest + right + 6).max(4));
        }
        left
    }
    pub fn paint(
        &mut self,
        canvas: &mut Canvas,
        rows: &[Row],
        top: usize,
        bottom: usize,
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
        let left = Self::left(canvas.width, rows);
        for (i, row) in rows.iter().enumerate().skip(self.scroll).take(capacity) {
            row.paint(
                canvas,
                left,
                top + i - self.scroll,
                i == self.selected && !self.back,
                self.back,
            );
        }
        (left, capacity)
    }
    pub fn paint_range(&self, canvas: &mut Canvas, count: usize, capacity: usize, y: usize) {
        if capacity == 0 || count <= capacity {
            return;
        }
        let label = format!(
            "{}–{} of {}",
            self.scroll + 1,
            (self.scroll + capacity).min(count),
            count
        );
        let width = UnicodeWidthStr::width(label.as_str());
        if width + 2 <= canvas.width {
            paint_hint(canvas, (canvas.width - width) / 2, y, &label);
        }
    }
    /// Reference Back/Exit control belongs beside the first visible option.
    pub fn paint_back(&self, canvas: &mut Canvas, left: usize, top: usize, label: &str) {
        let text = format!("◀— {label}");
        let expanded_width = UnicodeWidthStr::width(text.as_str()) + 1;
        let expanded = self.back && left > expanded_width;
        let (x, text) = if expanded {
            (left - expanded_width, text.as_str())
        } else {
            (left.saturating_sub(3), "◀—")
        };
        canvas.text(
            x,
            top,
            text,
            Cell {
                fg: if self.back { [238; 3] } else { [188; 3] },
                bold: self.back,
                ..Cell::default()
            },
            false,
        );
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
        options.paint(&mut canvas, &rows, 3, 4);
        assert_eq!(options.scroll, 2);
        options.step(&rows, true, false);
        options.paint(&mut canvas, &rows, 3, 4);
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
    #[test]
    fn optional_preview_never_recenters_or_partially_displays_a_path() {
        let rows = [Row::yes("Global root").with_preview("/projects/long/directory")];
        let mut options = Options::default();
        let mut small = Canvas::new(40, 12);
        let (small_left, _) = options.paint(&mut small, &rows, 3, 8);
        assert_eq!(small_left, (40 - 13) / 2);
        assert!(!small.cells.iter().any(|c| c.glyph == "/"));
        let mut wide = Canvas::new(120, 30);
        let (wide_left, _) = options.paint(&mut wide, &rows, 3, 8);
        assert_eq!(wide_left, (120 - 13) / 2);
        assert!(wide.cells.iter().any(|c| c.glyph == "/"));
    }
    #[test]
    fn selected_preview_hides_without_moving_options_and_remains_gray() {
        let rows = [Row::yes("Operation").with_selected_preview("request-id")];
        let mut options = Options::default();
        let mut selected = Canvas::new(80, 12);
        let (left, _) = options.paint(&mut selected, &rows, 3, 8);
        let start = 3 * 80 + left + 2 + "Operation".len() + 2;
        assert_eq!(selected.cells[start].glyph, "r");
        assert_eq!(selected.cells[start].fg, [188; 3]);
        assert!(!selected.cells[start].bold);
        options.back = true;
        let mut back = Canvas::new(80, 12);
        assert_eq!(options.paint(&mut back, &rows, 3, 8).0, left);
        assert!(
            !back
                .cells
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
                .contains("request-id")
        );
        let mut unselected = Canvas::new(80, 1);
        rows[0].paint(&mut unselected, left, 0, false, false);
        assert!(
            !unselected
                .cells
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
                .contains("request-id")
        );
    }
    #[test]
    fn footer_hints_use_reference_soft_palette_fade_and_centering() {
        let mut field = crate::render::Field::new(80, 24, 1967);
        for time in [0, 1467, 4060, 8000] {
            let mut canvas = field.paint(time);
            let hints = vec!["Esc: Back".into()];
            paint_hints(&mut canvas, &hints);
            let mut expected = field.paint(time);
            expected.accent_text(
                35,
                23,
                "E",
                Accent {
                    soft: true,
                    brightness: 550,
                    flash: false,
                    ..Accent::default()
                },
            );
            let cell = &canvas.cells[23 * 80 + 35];
            assert_eq!(cell.glyph, "E");
            assert_eq!(cell.fg, expected.cells[23 * 80 + 35].fg);
            assert!(!cell.bold);
        }
    }
    #[test]
    fn expanded_back_keeps_a_left_padding_cell() {
        let options = Options {
            back: true,
            ..Options::default()
        };
        for (left, expands) in [(8, false), (9, true)] {
            let mut canvas = Canvas::new(30, 8);
            options.paint_back(&mut canvas, left, 3, "Back");
            let row: String = canvas.cells[90..120]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect();
            assert_eq!(row.contains("Back"), expands);
            assert_eq!(canvas.cells[90].glyph, " ");
        }
    }
    #[test]
    fn back_is_compact_until_focused_and_expands_left_of_the_top_option() {
        let mut options = Options::default();
        let mut canvas = Canvas::new(80, 24);
        options.paint_back(&mut canvas, 30, 3, "Close");
        assert_eq!(canvas.cells[3 * 80 + 27].glyph, "◀");
        assert!(!canvas.cells.iter().any(|c| c.glyph == "C"));
        options.back = true;
        let mut focused = Canvas::new(80, 24);
        options.paint_back(&mut focused, 30, 3, "Close");
        let text: String = focused.cells[3 * 80..4 * 80]
            .iter()
            .map(|c| c.glyph.as_str())
            .collect();
        assert!(text.contains("◀— Close"));
        assert!(focused.cells[3 * 80 + 21].bold);
        assert_eq!(focused.cells[3 * 80 + 30].glyph, " ");
    }
}
