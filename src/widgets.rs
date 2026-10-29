//! Reusable terminal controls. No filesystem, provider, host or application policy.
use crate::render::{Accent, Canvas, Cell};
use crossterm::event::KeyCode;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
pub const SECONDARY: [u8; 3] = [188; 3];
pub const MUTED: [u8; 3] = [108; 3];
pub const SELECTED: [u8; 3] = [238; 3];
pub const UNSET: &str = "not set";
pub const NAME_MAX_CHARS: usize = 30;
pub use crate::navigation::{Navigation, SelectionNavigation};

#[derive(Clone, Debug, Default)]
pub struct TextInput {
    pub value: String,
    /// UTF-8 byte offset at a grapheme boundary.
    pub cursor: usize,
    pub max_chars: Option<usize>,
    /// Faint completion drawn after the value while the cursor is at its end.
    pub ghost: String,
}
impl TextInput {
    pub fn new(value: String) -> Self {
        let cursor = value.len();
        Self {
            value,
            cursor,
            max_chars: None,
            ghost: String::new(),
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
        let elided = usize::from(width > 3 && UnicodeWidthStr::width(before) > width - 3);
        let skip = UnicodeWidthStr::width(before).saturating_sub(width - 3 - elided);
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
                (*used <= width - 2 - elided).then_some(g)
            })
            .collect();
        canvas.widget_text(
            x,
            y,
            &format!("> {}{visible}", if elided > 0 { "…" } else { "" }),
            style.clone(),
            true,
        );
        let cursor = x + 2 + elided + UnicodeWidthStr::width(before).saturating_sub(removed);
        if focused && cursor < x + width && cursor < canvas.width && y < canvas.height {
            let glyph = self.value[self.cursor..]
                .graphemes(true)
                .next()
                .unwrap_or(" ");
            canvas.widget_text(cursor, y, glyph, style, false);
            canvas.cursor = Some((cursor, y));
        }
        if !self.ghost.is_empty() && self.cursor == self.value.len() && y < canvas.height {
            let room = (x + width).saturating_sub(cursor);
            let ghost = clip(&self.ghost, room);
            if !ghost.is_empty() {
                canvas.widget_text(
                    cursor,
                    y,
                    &ghost,
                    Cell {
                        fg: canvas.popup_palette.as_ref().map_or(MUTED, |p| p.muted),
                        ..Cell::default()
                    },
                    false,
                );
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Row {
    pub label: String,
    pub status: Option<(String, bool)>,
    pub enabled: bool,
    pub preview: Option<String>,
    pub right_preview: bool,
    pub preview_selected_only: bool,
    pub preview_column: Option<usize>,
    pub preview_accent: bool,
    pub preview_fg: [u8; 3],
    pub account: bool,
    /// Provider symbol drawn in place of the selection bullet.
    pub symbol: Option<&'static str>,
    pub preview_gap: usize,
    pub separator: bool,
    left_aligned: bool,
    preview_edge_padding: usize,
    placeholder: bool,
    preview_group: u8,
    field_padding: Option<std::ops::Range<usize>>,
    /// The preview is an interactive `‹ value ›` selector cycled with Left/Right.
    pub preview_choice: bool,
    /// The choice is grabbed (picker mode): drawn `▲ value ▼` with the cursor on it.
    pub preview_grabbed: bool,
    /// Leading label characters drawn in the bright text colour (typed prefix).
    pub highlight: usize,
}
impl Row {
    pub fn field(label: &str, value: impl std::fmt::Display, label_width: usize) -> Self {
        let value = value.to_string();
        if value == UNSET {
            // Placeholder: dim text in the value column, never part of the label.
            let mut row = Self::yes(format!("{label}:"));
            row.preview = Some(UNSET.into());
            row.preview_column = Some(2 + label_width + 2);
            row.preview_fg = MUTED;
            row.preview_gap = 1;
            row.field_padding = Some(label.len() + 1..label_width + 2);
            return row;
        }
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
            status: None,
            preview: None,
            right_preview: false,
            preview_selected_only: false,
            preview_column: None,
            preview_accent: false,
            preview_fg: SECONDARY,
            account: false,
            symbol: None,
            preview_gap: 2,
            separator: false,
            left_aligned: false,
            preview_edge_padding: 1,
            placeholder: false,
            preview_group: 0,
            field_padding: None,
            preview_choice: false,
            preview_grabbed: false,
            highlight: 0,
        }
    }
    pub fn with_preview(mut self, preview: impl Into<String>) -> Self {
        self.preview = Some(preview.into());
        self.preview_selected_only = true;
        self
    }
    /// Persistent trailing status, independent of selection and optional previews.
    pub fn with_status(mut self, text: impl Into<String>, accent: bool) -> Self {
        self.status = Some((text.into(), accent));
        self
    }
    pub fn content_end(&self) -> usize {
        let end = self
            .preview_position(usize::MAX / 2, 0)
            .zip(self.preview.as_deref())
            .map_or(2 + UnicodeWidthStr::width(self.label.as_str()), |(x, s)| {
                x + UnicodeWidthStr::width(s)
            });
        end + self
            .status
            .as_ref()
            .map_or(0, |(s, _)| 1 + UnicodeWidthStr::width(s.as_str()))
    }
    pub fn with_symbol(mut self, symbol: &'static str) -> Self {
        self.symbol = Some(symbol);
        self
    }
    pub fn account(mut self) -> Self {
        self.account = true;
        self
    }
    pub fn placeholder(label: impl Into<String>) -> Self {
        let mut row = Self::gate(label, false);
        row.placeholder = true;
        row
    }
    /// Popup frames already supply one outer padding cell on either side.
    pub fn popup_option(mut self) -> Self {
        self.left_aligned = true;
        self.preview_edge_padding = 0;
        self
    }
    pub fn with_preview_group(mut self, group: u8) -> Self {
        self.preview_group = group;
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
            width.saturating_sub(self.preview_edge_padding + UnicodeWidthStr::width(preview))
        } else {
            minimum
        };
        (start >= minimum
            && start + UnicodeWidthStr::width(preview)
                <= width.saturating_sub(self.preview_edge_padding))
        .then_some(start)
    }
    /// Nonselectable group label drawn after the standard bullet (`• Today`).
    pub fn heading(label: impl Into<String>) -> Self {
        let mut row = Self::separator();
        row.label = label.into();
        row
    }
    /// Short, always-visible dim reason for an unavailable row (`· reason`).
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.preview = Some(format!("· {}", reason.into()));
        self.preview_selected_only = false;
        self.preview_fg = MUTED;
        self.preview_gap = 2;
        self
    }
    /// An interactive value cycled with Left/Right, drawn as `‹ value ›` with dim
    /// arrows and the active value highlighted.
    pub fn with_choice(mut self, value: impl AsRef<str>) -> Self {
        self.preview = Some(format!("‹ {} ›", value.as_ref()));
        self.preview_selected_only = false;
        self.preview_choice = true;
        self.preview_gap = 2;
        self
    }
    /// Mark a choice row as grabbed by its picker.
    pub fn grabbed(mut self, grabbed: bool) -> Self {
        self.preview_grabbed = grabbed && self.preview_choice;
        self
    }
    pub fn has_reason(&self) -> bool {
        !self.preview_selected_only
            && !self.right_preview
            && self.preview.as_deref().is_some_and(|p| p.starts_with("· "))
    }
    pub fn separator() -> Self {
        Self {
            symbol: None,
            label: String::new(),
            enabled: false,
            status: None,
            preview: None,
            right_preview: false,
            preview_selected_only: false,
            preview_column: None,
            preview_accent: false,
            preview_fg: SECONDARY,
            account: false,
            preview_gap: 2,
            separator: true,
            left_aligned: false,
            preview_edge_padding: 1,
            placeholder: false,
            preview_group: 0,
            field_padding: None,
            preview_choice: false,
            preview_grabbed: false,
            highlight: 0,
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
        // A grabbed picker moves the row cursor onto the value.
        let grabbed = self.preview_grabbed && selected && !back;
        let selected = selected && !grabbed;
        if let Some((status, accent)) = &self.status {
            let status_width = UnicodeWidthStr::width(status.as_str());
            let start = self
                .content_end()
                .saturating_sub(status_width)
                .saturating_add(x)
                .min(
                    canvas
                        .width
                        .saturating_sub(self.preview_edge_padding + status_width),
                );
            if start >= x + 3 && y < canvas.height {
                let mut row = self.clone();
                row.status = None;
                row.label = ellipsize(&row.label, start.saturating_sub(x + 3));
                let mut left = Canvas::new(start - 1, 1);
                left.popup_palette = canvas.popup_palette.clone();
                row.paint_with_preview(&mut left, x, 0, selected, back, show_preview);
                for (col, cell) in left.cells.into_iter().enumerate() {
                    canvas.cells[y * canvas.width + col] = cell;
                }
                if *accent {
                    canvas.accent_text(
                        start,
                        y,
                        status,
                        Accent {
                            brightness: 1000,
                            bold: false,
                            ..Accent::default()
                        },
                    );
                } else {
                    canvas.widget_text(
                        start,
                        y,
                        status,
                        Cell {
                            fg: SECONDARY,
                            ..Cell::default()
                        },
                        false,
                    );
                }
                return;
            }
        }
        if self.placeholder {
            canvas.accent_text(
                x,
                y,
                if selected && !back { "●" } else { "○" },
                Accent {
                    brightness: if back { 600 } else { 1000 },
                    ..Accent::default()
                },
            );
            canvas.widget_text(x + 1, y, " ", Cell::default(), true);
            canvas.widget_text(
                x + 2,
                y,
                &self.label,
                Cell {
                    fg: SECONDARY,
                    ..Cell::default()
                },
                true,
            );
            return;
        }
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
            if !self.label.is_empty() {
                canvas.widget_text(
                    x + 2,
                    y,
                    &self.label,
                    Cell {
                        fg: MUTED,
                        ..Cell::default()
                    },
                    false,
                );
            }
            return;
        }
        canvas.accent_text(
            x,
            y,
            if let Some(symbol) = self.symbol {
                symbol
            } else if self.account {
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
        canvas.widget_text(x + 1, y, " ", Cell::default(), true);
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
            canvas.widget_text(
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
            if self.highlight > 0 && !selected {
                let prefix: String = self.label.chars().take(self.highlight).collect();
                canvas.widget_text(
                    x + 2,
                    y,
                    &prefix,
                    Cell {
                        fg: canvas.popup_palette.as_ref().map_or(SELECTED, |p| p.text),
                        ..Cell::default()
                    },
                    false,
                );
            }
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
            if grabbed {
                canvas.accent_text(
                    start.saturating_sub(2),
                    y,
                    "●",
                    Accent {
                        bold: true,
                        ..Accent::default()
                    },
                );
            }
            self.paint_preview(canvas, start, y, preview, selected && !back, grabbed);
        }
    }
    fn paint_preview(
        &self,
        canvas: &mut Canvas,
        x: usize,
        y: usize,
        preview: &str,
        selected: bool,
        grabbed: bool,
    ) {
        if self.preview_choice {
            let selected = selected || grabbed;
            let arrow = Cell {
                fg: if grabbed {
                    SELECTED
                } else if selected {
                    SECONDARY
                } else {
                    MUTED
                },
                bold: grabbed,
                ..Cell::default()
            };
            let inner = preview
                .trim_start_matches("‹ ")
                .trim_end_matches(" ›")
                .to_owned();
            canvas.widget_text(
                x,
                y,
                if grabbed { "▲ " } else { "‹ " },
                arrow.clone(),
                false,
            );
            canvas.widget_text(
                x + 2,
                y,
                &inner,
                Cell {
                    fg: if selected { SELECTED } else { SECONDARY },
                    bold: selected,
                    ..Cell::default()
                },
                false,
            );
            canvas.widget_text(
                x + 2 + UnicodeWidthStr::width(inner.as_str()),
                y,
                if grabbed { " ▼" } else { " ›" },
                arrow,
                false,
            );
            return;
        }
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
            canvas.widget_text(
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
    /// Visible list rows at the last paint; PageUp/PageDown move by this much.
    pub capacity: usize,
}
/// Align every row's always-visible reason into one column after the longest label.
pub fn align_reasons(rows: &mut [Row]) {
    let longest = rows
        .iter()
        .filter(|r| !r.separator)
        .map(|r| UnicodeWidthStr::width(r.label.as_str()))
        .max()
        .unwrap_or(0);
    for row in rows.iter_mut().filter(|r| r.has_reason()) {
        row.preview_column = Some(2 + longest + 2);
    }
}

/// Shared bottom hints: token wrapping never changes the star-field geometry.
pub fn paint_hint(canvas: &mut Canvas, x: usize, y: usize, text: &str) {
    if let Some(palette) = canvas.popup_palette.clone() {
        if let Some((keys, action)) = text.split_once(": ") {
            let key_width = UnicodeWidthStr::width(keys);
            canvas.text(
                x,
                y,
                keys,
                Cell {
                    fg: palette.text,
                    ..Default::default()
                },
                false,
            );
            canvas.text(
                x + key_width,
                y,
                &format!(": {action}"),
                Cell {
                    fg: palette.muted,
                    ..Default::default()
                },
                false,
            );
        } else {
            canvas.text(
                x,
                y,
                text,
                Cell {
                    fg: palette.secondary,
                    ..Default::default()
                },
                false,
            );
        }
        return;
    }
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
/// Center each hint within its own surface, retaining an opaque popup background.
pub fn paint_centered_hint(canvas: &mut Canvas, left: usize, y: usize, width: usize, text: &str) {
    let text = clip(text, width);
    let used = UnicodeWidthStr::width(text.as_str());
    let x = left + width.saturating_sub(used) / 2;
    if y >= canvas.height || x + used > canvas.width {
        return;
    }
    let start = y * canvas.width + x;
    let backgrounds: Vec<_> = canvas.cells[start..start + used]
        .iter()
        .map(|c| c.bg)
        .collect();
    paint_hint(canvas, x, y, &text);
    for (cell, bg) in canvas.cells[start..start + used]
        .iter_mut()
        .zip(backgrounds)
    {
        cell.bg = bg;
    }
}
/// Shared one-line status lane with one blank row before the actual controls.
pub fn paint_status(canvas: &mut Canvas, controls_start: usize, text: &str) {
    paint_status_colored(canvas, controls_start, text, false);
}
pub fn paint_status_colored(canvas: &mut Canvas, controls_start: usize, text: &str, accent: bool) {
    let text = clip(
        &crate::messages::describe(text),
        canvas.width.saturating_sub(2),
    );
    let width = UnicodeWidthStr::width(text.as_str());
    let x = canvas.width.saturating_sub(width) / 2;
    let y = controls_start.saturating_sub(2);
    if accent {
        canvas.accent_text(
            x,
            y,
            &text,
            Accent {
                brightness: 1000,
                bold: false,
                ..Accent::default()
            },
        );
    } else {
        canvas.widget_text(x, y, &text, Cell::default(), false);
    }
}
pub fn paint_hints(canvas: &mut Canvas, hints: &[String]) {
    let start = canvas.height.saturating_sub(hints.len());
    for (i, hint) in hints.iter().enumerate() {
        paint_centered_hint(canvas, 0, start + i, canvas.width, hint);
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
/// Keep the end of a long value (paths), marking hidden text with a leading `…`.
pub fn ellipsize_start(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    let mut used = 1;
    let mut start = graphemes.len();
    while start > 0 && used + UnicodeWidthStr::width(graphemes[start - 1]) <= width {
        start -= 1;
        used += UnicodeWidthStr::width(graphemes[start]);
    }
    format!("…{}", graphemes[start..].concat())
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
        let before = self.selected;
        self.selected = self
            .navigation
            .step(self.selected, count, previous, wrap, |index| {
                index == rows.len() || !rows[index].separator
            });
        if previous && self.selected > before {
            self.scroll = rows.len();
        } else if !previous && self.selected < before {
            self.scroll = 0;
        }
    }
    /// PageUp/PageDown: move one visible page, landing on the nearest selectable row.
    pub fn page(&mut self, rows: &[Row], previous: bool) {
        let count = rows.len();
        if count == 0 {
            return;
        }
        self.back = false;
        let step = self.capacity.saturating_sub(1).max(1);
        let target = if previous {
            self.selected.saturating_sub(step)
        } else {
            (self.selected + step).min(count - 1)
        };
        if let Some(found) = (0..count)
            .filter(|i| !rows[*i].separator)
            .min_by_key(|i| (i.abs_diff(target), if previous { *i } else { count - i }))
        {
            self.selected = found;
        }
    }
    /// Stepping in a list whose last `pinned` rows stay mounted below it: entering
    /// the pinned group never scrolls the list; leaving it upward re-enters the
    /// list at its last item.
    pub fn step_pinned(&mut self, rows: &[Row], pinned: usize, previous: bool, wrap: bool) {
        let scroll = self.scroll;
        self.step_with_wrap(rows, false, previous, wrap);
        if self.selected + pinned >= rows.len() {
            self.scroll = scroll;
        }
    }
    pub fn boundary_pinned(&mut self, rows: &[Row], pinned: usize, last: bool) {
        let scroll = self.scroll;
        self.boundary(rows, false, last);
        if last && pinned > 0 {
            self.scroll = scroll;
        }
    }
    pub fn boundary(&mut self, rows: &[Row], bottom_action: bool, last: bool) {
        self.back = false;
        self.selected = self.navigation.boundary(
            self.selected,
            rows.len() + usize::from(bottom_action),
            last,
            |i| i == rows.len() || !rows[i].separator,
        );
        self.scroll = if last { rows.len() } else { 0 };
    }
    pub fn left(width: usize, rows: &[Row]) -> usize {
        if rows.iter().any(|r| r.left_aligned) {
            return 0;
        }
        let longest = rows
            .iter()
            .map(|r| {
                r.preview_column.zip(r.preview.as_deref()).map_or(
                    UnicodeWidthStr::width(r.label.as_str()),
                    |(col, preview)| col.saturating_sub(2) + UnicodeWidthStr::width(preview),
                )
            })
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
    fn preview_groups(width: usize, rows: &[Row]) -> [bool; 256] {
        let mut groups = [false; 256];
        let left = Self::left(width, rows);
        for row in rows {
            if row.preview_selected_only
                && row.preview.is_some()
                && row.preview_position(width, left).is_none()
            {
                groups[row.preview_group as usize] = true;
            }
        }
        groups
    }
    pub fn previews_detached(width: usize, rows: &[Row]) -> bool {
        Self::preview_groups(width, rows)
            .iter()
            .any(|detached| *detached)
    }
    fn preview_group_detached(width: usize, rows: &[Row], group: u8) -> bool {
        Self::preview_groups(width, rows)[group as usize]
    }
    pub fn selected_preview_detached(&self, width: usize, rows: &[Row]) -> bool {
        rows.get(self.selected)
            .is_some_and(|r| Self::preview_group_detached(width, rows, r.preview_group))
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
        let selected_detached = self.selected_preview_detached(canvas.width, rows);
        self.preview_visible = !selected_detached
            && !self.back
            && self.selected >= self.scroll
            && self.selected < self.scroll + result.1
            && rows
                .get(self.selected)
                .is_some_and(|r| r.preview_position(canvas.width, result.0).is_some());
        self.paint_range(canvas, rows.len(), result.1, bottom);
        if selected_detached
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
                let max = canvas.width.saturating_sub(row.preview_edge_padding * 2);
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
                    false,
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
        self.capacity = capacity;
        let left = Self::left(canvas.width, rows);
        let detached_groups = Self::preview_groups(canvas.width, rows);
        for (i, row) in rows.iter().enumerate().skip(self.scroll).take(capacity) {
            row.paint_with_preview(
                canvas,
                left,
                top + i - self.scroll,
                i == self.selected && !self.back,
                self.back,
                !detached_groups[row.preview_group as usize] || !row.preview_selected_only,
            );
        }
        (left, capacity)
    }
    pub fn paint_footer_menu(
        &mut self,
        canvas: &mut Canvas,
        rows: &[Row],
        top: usize,
        bottom: usize,
        controls_start: usize,
    ) -> (usize, usize) {
        self.paint_pinned_menu(canvas, rows, 2, top, bottom, controls_start)
    }
    /// The last `pinned` rows (a bullet and actions) stay mounted below a list
    /// that keeps its own scroll position while focus visits them.
    pub fn paint_pinned_menu(
        &mut self,
        canvas: &mut Canvas,
        rows: &[Row],
        pinned: usize,
        top: usize,
        bottom: usize,
        controls_start: usize,
    ) -> (usize, usize) {
        if rows.len() < pinned || bottom.saturating_sub(top) < pinned + 2 {
            return self.paint_menu(canvas, rows, top, bottom, controls_start);
        }
        let end = rows.len() - pinned;
        let note_rows = bottom.saturating_sub(controls_start.saturating_sub(1));
        let capacity = bottom.saturating_sub(top + pinned + note_rows).min(end);
        let mut body = self.clone();
        if self.selected >= end {
            body.selected = usize::MAX;
        }
        // Range and spacer share a row; the body does not reserve another one.
        let (left, _) = body.paint(canvas, &rows[..end], top, top + capacity);
        self.scroll = body.scroll;
        self.preview_visible = false;
        let footer_top = top + capacity + note_rows;
        for (i, row) in rows[end..].iter().enumerate() {
            row.paint(
                canvas,
                left,
                footer_top + i,
                self.selected == end + i && !self.back,
                self.back,
            );
        }
        body.paint_range(canvas, end, capacity, footer_top);
        self.capacity = capacity;
        (left, capacity + pinned)
    }
    /// A scrolling list between mounted sections: the first `head` rows stay above
    /// it and the last `tail` rows below it. The list keeps its own scroll index
    /// (`scroll` is relative to the list) while focus visits either section.
    #[allow(clippy::too_many_arguments)]
    pub fn paint_sections(
        &mut self,
        canvas: &mut Canvas,
        rows: &[Row],
        head: usize,
        tail: usize,
        top: usize,
        bottom: usize,
        controls_start: usize,
    ) -> (usize, usize) {
        if rows.len() < head + tail || bottom.saturating_sub(top) < head + tail + 2 {
            return self.paint_menu(canvas, rows, top, bottom, controls_start);
        }
        let end = rows.len() - tail;
        let list = &rows[head..end];
        let note_rows = bottom.saturating_sub(controls_start.saturating_sub(1));
        let capacity = bottom
            .saturating_sub(top + head + tail + note_rows)
            .min(list.len());
        let mut body = self.clone();
        body.selected = if (head..end).contains(&self.selected) {
            self.selected - head
        } else {
            usize::MAX
        };
        let left = Self::left(canvas.width, rows);
        for (i, row) in rows[..head].iter().enumerate() {
            row.paint(
                canvas,
                left,
                top + i,
                self.selected == i && !self.back,
                self.back,
            );
        }
        body.paint(canvas, list, top + head, top + head + capacity);
        self.scroll = body.scroll;
        self.preview_visible = false;
        let footer_top = top + head + capacity + note_rows;
        for (i, row) in rows[end..].iter().enumerate() {
            row.paint(
                canvas,
                left,
                footer_top + i,
                self.selected == end + i && !self.back,
                self.back,
            );
        }
        body.paint_range(canvas, list.len(), capacity, footer_top);
        self.capacity = capacity;
        (left, head + capacity + tail)
    }
    /// Stepping with mounted sections: focus entering a section never moves the
    /// list; leaving one re-enters the list at its nearest edge.
    pub fn step_sections(
        &mut self,
        rows: &[Row],
        head: usize,
        tail: usize,
        previous: bool,
        wrap: bool,
    ) {
        let scroll = self.scroll;
        self.step_with_wrap(rows, false, previous, wrap);
        if self.selected < head || self.selected + tail >= rows.len() {
            self.scroll = scroll;
        }
    }
    pub fn boundary_sections(&mut self, rows: &[Row], last: bool) {
        let scroll = self.scroll;
        self.boundary(rows, false, last);
        self.scroll = scroll;
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
        canvas.widget_text(
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
    fn overflowing_input_shows_an_ellipsis_and_keeps_the_full_value() {
        let mut input = TextInput::new("/projects/deeper/path".into());
        for suffix in ["", "/child", "/grandchild"] {
            input.insert(suffix);
            let mut canvas = Canvas::new(14, 1);
            input.paint(&mut canvas, 0, 0, 14, true);
            assert_eq!(canvas.cells[2].glyph, "…");
            assert!(canvas.cursor.is_some_and(|(x, y)| x < 14 && y == 0));
            assert!(input.value.starts_with("/projects/deeper/path"));
        }
        input.edit(KeyCode::Home);
        let mut canvas = Canvas::new(14, 1);
        input.paint(&mut canvas, 0, 0, 14, true);
        assert_eq!(canvas.cells[2].glyph, "/");
        assert_eq!(canvas.cursor, Some((2, 0)));
        input.paint(&mut Canvas::new(3, 1), 0, 0, 3, true);
    }
    #[test]
    fn whole_choice_list_scrolls_actions_and_cancel_and_wraps_once() {
        let mut rows = vec![
            Row::yes("[use this directory]"),
            Row::yes("[create subdirectory]"),
            Row::separator(),
        ];
        rows.extend((0..30).map(|i| Row::yes(format!("directory{i:02}/"))));
        let rows = crate::popup::with_cancel(rows);
        let mut options = Options::default();
        let paint = |o: &mut Options, bottom| {
            let mut c = Canvas::new(60, 24);
            o.paint_menu(&mut c, &rows, 1, bottom, bottom + 1);
            c.cells
                .chunks(60)
                .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>())
                .collect::<Vec<_>>()
        };
        for bottom in [7, 19] {
            options.boundary(&rows, false, true);
            let lines = paint(&mut options, bottom);
            let cancel = lines.iter().position(|l| l.contains("Cancel")).unwrap();
            assert!(lines[cancel - 1].contains('•'));
            assert!(lines[cancel - 2].contains("directory29/"));
            assert!(!lines.iter().any(|l| l.contains("[use this directory]")));
            options.step(&rows, false, false);
            assert_eq!(options.selected, 0);
            let lines = paint(&mut options, bottom);
            assert!(lines[1].contains("[use this directory]"));
            assert!(!lines.iter().any(|l| l.contains("Cancel")));
            options.step(&rows, false, true);
            assert_eq!(options.selected, rows.len() - 1);
            assert!(
                paint(&mut options, bottom)
                    .iter()
                    .any(|l| l.contains("directory29/"))
            );
            options.boundary(&rows, false, false);
            assert!(paint(&mut options, bottom)[1].contains("[use this directory]"));
        }
    }
    #[test]
    fn directory_footer_stays_mounted_while_all_top_actions_scroll() {
        let rows = crate::popup::with_cancel(
            [Row::yes("[use this directory]"), Row::yes("<new>/")]
                .into_iter()
                .chain((0..30).map(|i| Row::yes(format!("directory{i:02}/")))),
        );
        let mut o = Options::default();
        let paint = |o: &mut Options| {
            let mut c = Canvas::new(60, 24);
            o.paint_footer_menu(&mut c, &rows, 1, 20, 21);
            c.cells
                .chunks(60)
                .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>())
                .collect::<Vec<_>>()
        };
        let top = paint(&mut o);
        assert!(top[1].contains("[use this directory]"));
        assert!(top[2].contains("<new>/"));
        assert!(top[3].contains("directory00/"));
        assert!(top[18].contains('•') && top[19].contains("Cancel"));
        o.boundary(&rows, false, true);
        let bottom = paint(&mut o);
        assert!(!bottom.iter().any(|l| l.contains("[use this directory]")));
        assert!(bottom[17].contains("directory29/"));
        assert!(bottom[18].contains(" of "));
        assert!(bottom[18].contains('•') && bottom[19].contains("Cancel"));
        o.step(&rows, false, false);
        assert!(paint(&mut o)[1].contains("[use this directory]"));
    }
    #[test]
    fn footer_connects_to_short_and_empty_directory_lists_without_an_unused_gap() {
        for children in [vec![Row::placeholder("(none)")], vec![Row::yes("child/")]] {
            let rows = crate::popup::with_cancel(
                [Row::yes("[use this directory]"), Row::yes("<new>/")]
                    .into_iter()
                    .chain(children),
            );
            for selected in [0, 2, 4] {
                let mut options = Options {
                    selected,
                    ..Options::default()
                };
                let mut c = Canvas::new(60, 24);
                options.paint_footer_menu(&mut c, &rows, 1, 20, 21);
                let line = |y: usize| {
                    c.cells[y * 60..(y + 1) * 60]
                        .iter()
                        .map(|c| c.glyph.as_str())
                        .collect::<String>()
                };
                assert!(line(3).contains(&rows[2].label));
                assert!(line(4).contains('•'));
                assert!(line(5).contains("Cancel"));
                assert!(
                    !c.cells
                        .iter()
                        .map(|c| c.glyph.as_str())
                        .collect::<String>()
                        .contains(" of ")
                );
            }
        }
    }
    #[test]
    fn persistent_status_takes_priority_over_usage_at_narrow_widths() {
        let row = Row::yes("longaccount")
            .with_column_preview("100% 5h · 100% wk", 15, false)
            .with_status("Ready", true)
            .popup_option();
        for width in [18, 24, 35, 80] {
            let mut c = Canvas::new(width, 1);
            row.paint(&mut c, 0, 0, false, false);
            let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
            assert!(text.contains("Ready"));
            let i = c.cells.iter().position(|c| c.glyph == "R").unwrap();
            assert!(c.cells[i..i + 5].iter().all(|c| !c.bold && !c.strike));
        }
    }
    #[test]
    fn placeholder_has_normal_bullet_and_short_cancel_follows_without_extra_space() {
        let rows = crate::popup::with_cancel([Row::placeholder("(none)")]);
        let mut c = Canvas::new(40, 20);
        Options::default().paint_menu(&mut c, &rows, 1, 16, 17);
        let line = |y: usize| {
            c.cells[y * 40..(y + 1) * 40]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
        };
        assert!(line(1).contains("(none)"));
        assert!(line(2).contains('•'));
        assert!(line(3).contains("Cancel"));
        assert_eq!(c.cells[40 + Options::left(40, &rows)].glyph, "●");
        assert!(c.cells.iter().all(|c| !c.strike && !c.bold));
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
    fn input_viewport_preserves_wide_characters_and_native_cursor() {
        let input = TextInput::new("界界界界界".into());
        let mut canvas = Canvas::new(10, 1);
        input.paint(&mut canvas, 0, 0, 9, true);
        assert_eq!(canvas.cursor, Some((7, 0)));
        assert_eq!(canvas.cells[2].glyph, "…");
        assert!(canvas.cells.iter().all(|c| c.bg.is_none()));
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
    fn each_wrapped_control_line_is_centered_independently() {
        let hints = vec!["Enter/Space: Select".into(), "Esc: Back".into()];
        let mut c = Canvas::new(40, 10);
        paint_hints(&mut c, &hints);
        for (y, hint) in [(8, &hints[0]), (9, &hints[1])] {
            let start = (40 - hint.len()) / 2;
            assert_eq!(c.cells[y * 40 + start].glyph, "E");
            assert_eq!(c.cells[y * 40 + start - 1].glyph, " ");
            assert_ne!(c.cells[y * 40 + start].fg, SECONDARY);
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
