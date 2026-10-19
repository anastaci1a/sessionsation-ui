//! Reusable terminal controls. No filesystem, provider, host or application policy.
use crate::render::{Accent, Canvas, Cell};
use crossterm::event::KeyCode;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
pub const SECONDARY: [u8; 3] = [188; 3];
pub const MUTED: [u8; 3] = [108; 3];
pub const SELECTED: [u8; 3] = [238; 3];
pub use crate::navigation::{Navigation, SelectionNavigation};

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
    pub preview_fg: [u8; 3],
    pub account: bool,
    pub preview_gap: usize,
    pub separator: bool,
    field_padding: Option<std::ops::Range<usize>>,
}
impl Row {
    pub fn field(label: &str, value: impl std::fmt::Display, label_width: usize) -> Self {
        let mut row = Self::yes(format!(
            "{:<width$} {value}",
            format!("{label}:"),
            width = label_width + 1
        ));
        row.field_padding = Some(label.len() + 1..label_width + 2);
        row
    }
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
            preview_fg: SECONDARY,
            account: false,
            preview_gap: 2,
            separator: false,
            field_padding: None,
        }
    }
    pub fn with_preview(mut self, preview: impl Into<String>) -> Self {
        self.preview = Some(preview.into());
        self.preview_selected_only = true;
        self
    }
    pub fn account(mut self) -> Self {
        self.account = true;
        self
    }
    pub fn preview_color(mut self, color: [u8; 3]) -> Self {
        self.preview_fg = color;
        self
    }
    pub fn preview_gap(mut self, gap: usize) -> Self {
        self.preview_gap = gap;
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
        if !accent {
            self.preview_fg = MUTED;
        }
        self
    }
    pub fn with_selected_preview(mut self, preview: impl Into<String>) -> Self {
        self.preview = Some(preview.into());
        self.preview_selected_only = true;
        self
    }
    pub fn preview_position(&self, width: usize, x: usize) -> Option<usize> {
        let preview = self.preview.as_deref()?;
        let minimum = x + 2 + UnicodeWidthStr::width(self.label.as_str()) + self.preview_gap;
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
            preview_fg: SECONDARY,
            account: false,
            preview_gap: 2,
            separator: true,
            field_padding: None,
        }
    }
    pub fn paint(&self, canvas: &mut Canvas, x: usize, y: usize, selected: bool, back: bool) {
        self.paint_with_preview(canvas, x, y, selected, back, true);
    }
    fn paint_with_preview(
        &self,
        canvas: &mut Canvas,
        x: usize,
        y: usize,
        selected: bool,
        back: bool,
        show_preview: bool,
    ) {
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
            if self.account {
                if selected && !back { "▶" } else { " " }
            } else if selected {
                "●"
            } else {
                "○"
            },
            Accent {
                brightness: if self.enabled && !back { 1000 } else { 600 },
                bold: selected,
                strike: false,
                ..Accent::default()
            },
        );
        canvas.text(x + 1, y, " ", Cell::default(), true);
        if self.account {
            canvas.accent_text(
                x + 2,
                y,
                &self.label,
                Accent {
                    bold: selected && !back && self.enabled,
                    strike: !self.enabled,
                    ..Accent::default()
                },
            );
        } else {
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
                    bold: selected && self.enabled,
                    strike: !self.enabled,
                    ..Cell::default()
                },
                self.enabled,
            );
        }
        if !self.enabled && y < canvas.height {
            // Layout gaps are not part of either struck-through text segment.
            // A single space inside a multi-word label remains styled with it.
            let chars: Vec<_> = self.label.chars().collect();
            let mut col = 0;
            for (i, ch) in chars.iter().enumerate() {
                let padding = *ch == ' '
                    && (self.field_padding.as_ref().is_some_and(|r| r.contains(&i))
                        || i > 0 && chars[i - 1] == ' '
                        || chars.get(i + 1) == Some(&' '));
                if padding && x + 2 + col < canvas.width {
                    canvas.cells[y * canvas.width + x + 2 + col].strike = false;
                }
                col += unicode_width::UnicodeWidthChar::width(*ch).unwrap_or(0);
            }
        }
        if let Some(preview) = &self.preview
            && show_preview
            && (!self.preview_selected_only || (selected && !back))
            && let Some(start) = self.preview_position(canvas.width, x)
        {
            self.paint_preview(canvas, start, y, preview, selected && !back);
        }
    }
    fn paint_preview(
        &self,
        canvas: &mut Canvas,
        x: usize,
        y: usize,
        preview: &str,
        selected: bool,
    ) {
        if self.preview_accent {
            canvas.accent_text(
                x,
                y,
                preview,
                Accent {
                    brightness: if selected { 1000 } else { 600 },
                    bold: false,
                    ..Accent::default()
                },
            );
        } else {
            canvas.text(
                x,
                y,
                preview,
                Cell {
                    fg: self.preview_fg,
                    ..Cell::default()
                },
                false,
            );
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub selected: usize,
    pub back: bool,
    pub scroll: usize,
    pub navigation: SelectionNavigation,
    /// Whether the highlighted row's preview was rendered by the last menu paint.
    pub preview_visible: bool,
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
            bold: false,
            ..Accent::default()
        },
    );
}
/// Shared one-line status lane with one blank row before the actual controls.
pub fn paint_status(canvas: &mut Canvas, controls_start: usize, text: &str) {
    let text = clip(text, canvas.width.saturating_sub(2));
    let width = UnicodeWidthStr::width(text.as_str());
    canvas.text(
        canvas.width.saturating_sub(width) / 2,
        controls_start.saturating_sub(2),
        &text,
        Cell::default(),
        false,
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
/// Preserve a visible truncation marker even when a layout narrows an already
/// shortened title. Width is terminal cells, never UTF-8 bytes.
pub fn ellipsize(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    format!("{}…", clip(text, width - 1))
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
    /// Bias odd free space right; six free columns need four on the arrow side.
    pub fn centered_left(width: usize, content: usize) -> usize {
        let free = width.saturating_sub(content);
        if free == 6 { 4 } else { free.div_ceil(2) }
    }
    /// Optional bottom action participates in focus without scrolling the list.
    pub fn step(&mut self, rows: &[Row], bottom_action: bool, previous: bool) {
        self.step_with_wrap(rows, bottom_action, previous, true);
    }
    pub fn step_with_wrap(
        &mut self,
        rows: &[Row],
        bottom_action: bool,
        previous: bool,
        wrap: bool,
    ) {
        let count = rows.len() + usize::from(bottom_action);
        if count == 0 {
            return;
        }
        self.back = false;
        self.selected = self
            .navigation
            .step(self.selected, count, previous, wrap, |index| {
                index == rows.len() || !rows[index].separator
            });
    }
    pub fn boundary(&mut self, rows: &[Row], bottom_action: bool, last: bool) {
        self.back = false;
        self.selected = self.navigation.boundary(
            self.selected,
            rows.len() + usize::from(bottom_action),
            last,
            |i| i == rows.len() || !rows[i].separator,
        );
    }
    pub fn left(width: usize, rows: &[Row]) -> usize {
        let longest = rows
            .iter()
            .map(|r| UnicodeWidthStr::width(r.label.as_str()))
            .max()
            .unwrap_or(0)
            .min(width.saturating_sub(8));
        let mut left = Self::centered_left(width, longest + 2);
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
    pub fn previews_detached(width: usize, rows: &[Row]) -> bool {
        let left = Self::left(width, rows);
        rows.iter().any(|r| {
            r.preview_selected_only
                && r.preview.is_some()
                && r.preview_position(width, left).is_none()
        })
    }
    /// One fit decision for the whole list, independent of selection and scrolling.
    pub fn paint_menu(
        &mut self,
        canvas: &mut Canvas,
        rows: &[Row],
        top: usize,
        bottom: usize,
        controls_start: usize,
    ) -> (usize, usize) {
        self.preview_visible = false;
        let detached = Self::previews_detached(canvas.width, rows);
        let bottom = bottom.min(controls_start.saturating_sub(if detached { 3 } else { 1 }));
        // Reserve a range counter only after the options stop fitting.
        let bottom = if rows.len() > bottom.saturating_sub(top) {
            bottom.saturating_sub(1)
        } else {
            bottom
        };
        let result = self.paint(canvas, rows, top, bottom);
        self.preview_visible = !detached
            && !self.back
            && self.selected >= self.scroll
            && self.selected < self.scroll + result.1
            && rows
                .get(self.selected)
                .is_some_and(|r| r.preview_position(canvas.width, result.0).is_some());
        self.paint_range(canvas, rows.len(), result.1, bottom);
        if detached
            && !self.back
            && result.1 > 0
            && let Some(row) = rows.get(self.selected)
            && row.preview_selected_only
            && let Some(preview) = row.preview.as_deref()
        {
            let y = if rows.len() > result.1 {
                bottom + 2
            } else {
                top + rows.len().saturating_sub(self.scroll).min(result.1) + 1
            };
            if y + 1 < controls_start && canvas.width > 3 {
                let max = canvas.width - 2;
                let text = if UnicodeWidthStr::width(preview) > max {
                    ellipsize(preview, max)
                } else {
                    preview.to_owned()
                };
                self.preview_visible = true;
                row.paint_preview(
                    canvas,
                    canvas
                        .width
                        .saturating_sub(UnicodeWidthStr::width(text.as_str()))
                        / 2,
                    y,
                    &text,
                    true,
                );
            }
        }
        result
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
        let inline = !Self::previews_detached(canvas.width, rows);
        for (i, row) in rows.iter().enumerate().skip(self.scroll).take(capacity) {
            row.paint_with_preview(
                canvas,
                left,
                top + i - self.scroll,
                i == self.selected && !self.back,
                self.back,
                inline || !row.preview_selected_only,
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
    use crossterm::event::{KeyEvent, KeyEventKind};
    use std::time::{Duration, Instant};
    #[test]
    fn held_arrows_stop_at_edges_and_release_rearms_wrap() {
        let mut nav = Navigation::default();
        let start = Instant::now();
        let down = KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE);
        let mut repeated = down;
        repeated.kind = KeyEventKind::Repeat;
        let mut released = down;
        released.kind = KeyEventKind::Release;
        let rows = vec![
            Row::yes("First"),
            Row::yes("Last"),
            Row::gate("Unavailable", false),
        ];
        let mut options = Options::default();
        options.step_with_wrap(&rows, false, false, nav.key_at(down, start));
        assert_eq!(options.selected, 1);
        for i in 1..20 {
            options.step_with_wrap(
                &rows,
                false,
                false,
                nav.key_at(repeated, start + Duration::from_millis(i * 100)),
            );
            assert_eq!(options.selected, 2);
        }
        nav.key_at(released, start + Duration::from_millis(2000));
        options.step_with_wrap(
            &rows,
            false,
            false,
            nav.key_at(down, start + Duration::from_millis(2001)),
        );
        assert_eq!(options.selected, 0);
        let up = KeyEvent::new(KeyCode::Up, crossterm::event::KeyModifiers::NONE);
        let mut repeated_up = up;
        repeated_up.kind = KeyEventKind::Repeat;
        options.step_with_wrap(
            &rows,
            false,
            true,
            nav.key_at(repeated_up, start + Duration::from_millis(2100)),
        );
        assert_eq!(options.selected, 0);
        options.step_with_wrap(&rows, false, true, {
            let mut release = up;
            release.kind = KeyEventKind::Release;
            nav.key_at(release, start + Duration::from_millis(2199));
            nav.key_at(up, start + Duration::from_millis(2200))
        });
        assert_eq!(options.selected, 2);
        let mut legacy = Navigation::default();
        assert!(legacy.key_at(down, start));
        assert!(legacy.key_at(down, start + Duration::from_millis(500)));
        assert!(!legacy.key_at(down, start + Duration::from_millis(530)));
        assert!(legacy.key_at(down, start + Duration::from_millis(1300)));
    }
    #[test]
    fn mixed_legacy_arrows_after_enhanced_events_remain_one_hold() {
        let start = Instant::now();
        let rows = vec![Row::yes("First"), Row::yes("Middle"), Row::yes("Last")];
        for previous in [false, true] {
            let key = KeyEvent::new(
                if previous { KeyCode::Up } else { KeyCode::Down },
                crossterm::event::KeyModifiers::NONE,
            );
            let mut nav = Navigation::default();
            let mut unrelated =
                KeyEvent::new(KeyCode::Char('x'), crossterm::event::KeyModifiers::NONE);
            unrelated.kind = KeyEventKind::Release;
            nav.key_at(unrelated, start);
            let mut options = Options {
                selected: 1,
                ..Options::default()
            };
            for i in 1..30 {
                if i == 10 {
                    nav.key_at(unrelated, start + Duration::from_millis(i * 30 - 1));
                }
                let fresh = nav.key_at(key, start + Duration::from_millis(i * 30));
                options.step_with_wrap(&rows, false, previous, fresh);
                assert_eq!(options.selected, if previous { 0 } else { 2 });
            }
            let mut released = key;
            released.kind = KeyEventKind::Release;
            nav.key_at(released, start + Duration::from_millis(901));
            options.step_with_wrap(
                &rows,
                false,
                previous,
                nav.key_at(key, start + Duration::from_millis(902)),
            );
            assert_eq!(options.selected, if previous { 2 } else { 0 });
        }
    }
    #[test]
    fn boundaries_skip_separators_and_include_unavailable_or_bottom_actions() {
        let rows = vec![
            Row::separator(),
            Row::yes("First"),
            Row::gate("Last", false),
            Row::separator(),
        ];
        let mut options = Options {
            back: true,
            ..Options::default()
        };
        options.boundary(&rows, false, true);
        assert_eq!(options.selected, 2);
        assert!(!options.back);
        options.boundary(&rows, false, false);
        assert_eq!(options.selected, 1);
        options.boundary(&rows, true, true);
        assert_eq!(options.selected, 4);
    }
    #[test]
    fn ellipsis_survives_reclipping_and_respects_terminal_cells() {
        assert_eq!(
            ellipsize("abcdefghijklmnopqrstuvwx", 16),
            "abcdefghijklmno…"
        );
        assert_eq!(ellipsize("abcdefghijklmno…", 10), "abcdefghi…");
        assert_eq!(ellipsize("界界界", 4), "界…");
        assert_eq!(ellipsize("long", 1), "…");
        assert_eq!(ellipsize("long", 0), "");
        assert_eq!(ellipsize("fits", 4), "fits");
    }
    #[test]
    fn held_wrap_uses_starting_row_in_both_directions() {
        let rows = vec![Row::yes("First"), Row::yes("Middle"), Row::yes("Last")];
        for previous in [false, true] {
            for origin in 0..3 {
                let mut options = Options {
                    selected: origin,
                    ..Options::default()
                };
                let edge = if previous { 0 } else { 2 };
                for step in 0..10 {
                    options.step_with_wrap(&rows, false, previous, step == 0);
                    let expected = if origin == edge {
                        if previous {
                            2_usize.saturating_sub(step)
                        } else {
                            step.min(2)
                        }
                    } else if previous {
                        origin.saturating_sub(step + 1)
                    } else {
                        (origin + step + 1).min(2)
                    };
                    assert_eq!(
                        options.selected, expected,
                        "origin={origin}, previous={previous}, step={step}"
                    );
                }
            }
        }
    }
    #[test]
    fn eighteen_options_fit_twenty_two_body_rows_before_scrolling() {
        for count in [18, 19, 20] {
            let rows = vec![Row::yes("Option"); count];
            let mut options = Options::default();
            let mut c = Canvas::new(80, 24);
            let (_, capacity) = options.paint_menu(&mut c, &rows, 3, 22, 23);
            if count <= 19 {
                assert_eq!(capacity, 19);
                let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
                assert!(!text.contains(" of "));
            } else {
                assert_eq!(capacity, 18);
                assert!(c.cells[21 * 80..22 * 80].iter().any(|c| c.glyph == "f"));
            }
            assert!(c.cells[22 * 80..23 * 80].iter().all(|c| c.glyph == " "));
        }
    }
    #[test]
    fn arrow_centering_bias_and_unavailable_styles_are_shared() {
        assert_eq!(Options::centered_left(31, 10), 11);
        let left = Options::centered_left(16, 10);
        assert_eq!(left, 4);
        let mut c = Canvas::new(16, 4);
        Options::default().paint_back(&mut c, left, 1, "Back");
        assert_eq!(c.cells[17].glyph, "◀");
        let row = Row::gate("Unavailable", false);
        let mut c = Canvas::new(30, 2);
        row.paint(&mut c, 2, 0, true, false);
        assert!(!c.cells[2].strike);
        assert!(c.cells[4].strike);
        assert!(!c.cells[4].bold);
    }
    #[test]
    fn unavailable_fields_strike_segments_without_striking_alignment_gaps() {
        let mut c = Canvas::new(50, 3);
        let mut row = Row::field("Dir", "my saved directory", 8);
        row.enabled = false;
        row.paint(&mut c, 0, 0, true, false);
        assert!(c.cells[2..6].iter().all(|c| c.strike));
        assert!(c.cells[6..12].iter().all(|c| !c.strike));
        assert!(c.cells[12..30].iter().all(|c| c.strike));
        Row::gate("Two words   next segment", false).paint(&mut c, 0, 1, false, false);
        assert!(
            c.cells[55].strike,
            "intra-label space retains strikethrough"
        );
        assert!(c.cells[61..64].iter().all(|c| !c.strike));
    }
    #[test]
    fn global_preview_fit_includes_offscreen_rows_and_back_hides_details() {
        let mut rows = vec![Row::yes("Item").with_preview("short"); 20];
        rows[19] =
            Row::yes("Item").with_preview("a very long path that cannot fit beside any option");
        assert!(Options::previews_detached(50, &rows));
        let mut options = Options::default();
        let mut c = Canvas::new(50, 18);
        let (left, capacity) = options.paint_menu(&mut c, &rows, 3, 14, 17);
        assert!(capacity < rows.len());
        let text = |c: &Canvas, y: usize| {
            c.cells[y * 50..(y + 1) * 50]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
        };
        assert!(!text(&c, 3).contains("short"));
        assert!(text(&c, 13).contains("of 20"));
        assert!(text(&c, 15).contains("short"));
        for cell in &c.cells[15 * 50..16 * 50] {
            assert!(!cell.bold);
        }
        options.back = true;
        let mut back = Canvas::new(50, 18);
        assert_eq!(options.paint_menu(&mut back, &rows, 3, 14, 17).0, left);
        assert!(!text(&back, 15).contains("short"));
    }
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
    fn disabled_rows_receive_focus_separators_skip_and_bottom_action_keeps_background() {
        let rows = vec![Row::yes("one"), Row::gate("two", false), Row::yes("three")];
        let mut options = Options::default();
        options.step(&rows, true, false);
        assert_eq!(options.selected, 1);
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
    fn menu_navigation_skips_separators_but_visits_unavailable_rows() {
        let rows = vec![
            Row::yes("First"),
            Row::separator(),
            Row::gate("Unavailable", false),
        ];
        let mut options = Options::default();
        options.step(&rows, false, false);
        assert_eq!(options.selected, 2);
        options.step(&rows, false, true);
        assert_eq!(options.selected, 0);
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
        assert_eq!(small_left, (40usize - 13).div_ceil(2));
        assert!(!small.cells.iter().any(|c| c.glyph == "/"));
        let mut wide = Canvas::new(120, 30);
        let (wide_left, _) = options.paint(&mut wide, &rows, 3, 8);
        assert_eq!(wide_left, (120usize - 13).div_ceil(2));
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
    fn footer_hints_share_readable_soft_accent_and_centering_at_every_frame() {
        let mut field = crate::render::Field::new(80, 24, 1967);
        for time in [0, 1467, 4060, 8000] {
            let mut canvas = field.paint(time);
            let hints = vec!["Esc: Back".into()];
            paint_hints(&mut canvas, &hints);
            let cell = &canvas.cells[23 * 80 + 35];
            assert_eq!(cell.glyph, "E");
            assert!(cell.fg.iter().all(|n| *n >= 80));
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
