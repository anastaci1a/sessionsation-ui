//! Shared, nonblocking dialogs. Validation and cancellation never depend on focus.
use crate::{
    render::{Canvas, Cell},
    widgets::{NAME_MAX_CHARS, Row, TextInput, clip},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
const INVALID: [u8; 3] = [215, 0, 0];
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
#[derive(Clone, Debug, Default)]
pub enum Rule {
    #[default]
    Any,
    AccountName,
    Percentage,
    Name,
    Rename(String),
    Exact(String),
}
#[derive(Clone, Debug)]
pub struct Field {
    pub label: String,
    pub input: TextInput,
    pub rule: Rule,
    pub excluded: Vec<String>,
}
impl Field {
    pub fn new(label: &str, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            input: TextInput::new(value.into()),
            rule: Rule::Any,
            excluded: vec![],
        }
    }
    pub fn validated(mut self, rule: Rule) -> Self {
        if matches!(rule, Rule::AccountName | Rule::Name | Rule::Rename(_)) {
            self.input = self.input.limited(NAME_MAX_CHARS);
        }
        self.rule = rule;
        self
    }
    pub fn valid(&self) -> bool {
        let v = &self.input.value;
        if matches!(&self.rule, Rule::Rename(original) if v == original) {
            return true;
        }
        if self.excluded.contains(v) {
            return false;
        }
        match &self.rule {
            Rule::Any => true,
            Rule::Exact(expected) => v == expected,
            Rule::Percentage => v
                .parse::<f64>()
                .is_ok_and(|n| n.is_finite() && (0. ..=100.).contains(&n)),
            Rule::AccountName => {
                !v.is_empty()
                    && v.len() <= NAME_MAX_CHARS
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            }
            Rule::Name | Rule::Rename(_) => {
                !v.is_empty()
                    && v.trim() == v
                    && v.chars().count() <= NAME_MAX_CHARS
                    && !v.chars().any(char::is_control)
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct Dialog {
    pub title: String,
    pub body: Vec<String>,
    pub fields: Vec<Field>,
    pub buttons: Vec<Row>,
    /// One-field forms keep their editor active while selection visits buttons only.
    pub selected: usize,
    pub offset: usize,
    pub visible: usize,
    pub wrapped: usize,
    pub busy: bool,
    pub compact: bool,
    pub status: Option<String>,
    navigation: crate::widgets::SelectionNavigation,
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
            compact: false,
            status: None,
            navigation: crate::widgets::SelectionNavigation::default(),
        }
    }
    pub fn progress(title: &str) -> Self {
        let mut d = Self::new(title, vec!["Loading…".into()], &["Continue in background"]);
        d.busy = true;
        d
    }
    pub fn finish(&mut self) {
        self.busy = false;
        self.buttons = vec![Row::yes("Close")];
        self.selected = self.field_slots();
    }
    pub fn values(&self) -> Vec<String> {
        self.fields.iter().map(|f| f.input.value.clone()).collect()
    }
    fn editor(&self) -> Option<usize> {
        if self.fields.len() == 1 {
            Some(0)
        } else {
            (self.selected < self.fields.len()).then_some(self.selected)
        }
    }
    fn field_slots(&self) -> usize {
        if self.fields.len() == 1 {
            0
        } else {
            self.fields.len()
        }
    }
    fn dismiss_button(&self, index: usize) -> bool {
        self.buttons.get(index).is_some_and(|b| {
            matches!(
                b.label.as_str(),
                "Cancel" | "Close" | "Continue in background"
            )
        })
    }
    pub fn paste(&mut self, text: &str) {
        if let Some(i) = self.editor() {
            self.fields[i].input.insert(text);
            self.status = None;
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Event {
        self.key_with_wrap(key, key.kind != crossterm::event::KeyEventKind::Repeat)
    }
    pub fn key_with_wrap(&mut self, key: KeyEvent, wrap: bool) -> Event {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return Event::None;
        }
        let slots = self.field_slots();
        let count = slots + self.buttons.len();
        self.selected = self.selected.min(count.saturating_sub(1));
        match key.code {
            KeyCode::Esc => return Event::Dismiss,
            KeyCode::PageDown if self.wrapped > self.visible => {
                self.offset = (self.offset + self.visible.max(1))
                    .min(self.wrapped.saturating_sub(self.visible))
            }
            KeyCode::PageUp if self.wrapped > self.visible => {
                self.offset = self.offset.saturating_sub(self.visible.max(1))
            }
            KeyCode::Up | KeyCode::Down if count > 0 => {
                self.selected = self.navigation.step(
                    self.selected,
                    count,
                    key.code == KeyCode::Up,
                    wrap,
                    |_| true,
                );
            }
            KeyCode::Home | KeyCode::End if self.editor().is_none() => {
                self.selected = self.navigation.boundary(
                    self.selected,
                    count,
                    key.code == KeyCode::End,
                    |_| true,
                );
            }
            KeyCode::Tab if count > 0 => {
                self.selected = (self.selected + 1) % count;
            }
            KeyCode::Enter if self.selected >= slots => {
                let button = self.selected - slots;
                let cancel = self.dismiss_button(button);
                if self.buttons.get(button).is_some_and(|b| b.enabled)
                    && (cancel || self.fields.iter().all(Field::valid))
                    && (cancel || self.busy || self.visible > 0)
                {
                    return Event::Choose(button, self.values());
                }
            }
            KeyCode::Enter => self.selected = (self.selected + 1).min(count.saturating_sub(1)),
            KeyCode::Char(c) => self.paste(&c.to_string()),
            code => {
                if let Some(i) = self.editor() {
                    self.fields[i].input.edit(code);
                    self.status = None;
                }
            }
        }
        Event::None
    }
    pub fn paint(&mut self, canvas: &mut Canvas, palette: &Palette) {
        canvas.overlay(|canvas| self.paint_panel(canvas, palette));
    }
    fn paint_panel(&mut self, canvas: &mut Canvas, palette: &Palette) {
        let width = canvas.width.saturating_sub(4).min(76);
        let max_height = canvas.height.saturating_sub(4);
        let mut lines: Vec<_> = self
            .body
            .iter()
            .flat_map(|l| wrap(&crate::messages::describe(l), width.saturating_sub(4)))
            .collect();
        if let Some(status) = &self.status {
            lines.extend(wrap(
                &crate::messages::describe(status),
                width.saturating_sub(4),
            ));
        }
        let mut normalized = Vec::new();
        for line in lines {
            if !line.trim().is_empty()
                || normalized
                    .last()
                    .is_some_and(|l: &String| !l.trim().is_empty())
            {
                normalized.push(line);
            }
        }
        while normalized.last().is_some_and(|l| l.trim().is_empty()) {
            normalized.pop();
        }
        let lines = normalized;
        let base_reserved = if self.compact {
            2
        } else {
            self.fields.len() * 3 + self.buttons.len() + if self.fields.is_empty() { 3 } else { 4 }
        };
        let scroll_hint = lines.len() > max_height.saturating_sub(base_reserved);
        let reserved = base_reserved + if scroll_hint { 2 } else { 0 };
        if width < 20 || max_height < reserved + 1 {
            self.visible = 0;
            self.wrapped = 1;
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
        let height = max_height.min(lines.len().max(1) + reserved);
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
        crate::popup::Panel {
            x,
            y,
            width,
            height,
        }
        .frame(canvas, &self.title, palette);
        for (i, line) in lines.iter().skip(self.offset).take(body_height).enumerate() {
            canvas.text(x + 2, y + 1 + i, line, style.clone(), false);
        }
        if !self.compact {
            let mut row = y + 1 + body_height + usize::from(!self.fields.is_empty());
            for (index, field) in self.fields.iter().enumerate() {
                let field_style = Cell {
                    fg: if field.valid() { palette.text } else { INVALID },
                    ..style.clone()
                };
                let label = clip(&format!(" {} ", field.label), width - 8);
                let label_width = UnicodeWidthStr::width(label.as_str());
                canvas.text(
                    x + 2,
                    row,
                    &format!(
                        "┌─{}{}┐",
                        label,
                        "─".repeat((width - 7).saturating_sub(label_width))
                    ),
                    field_style.clone(),
                    false,
                );
                canvas.text(x + 2, row + 1, "│", field_style.clone(), false);
                canvas.text(x + width - 3, row + 1, "│", field_style.clone(), false);
                field.input.paint_focused(
                    canvas,
                    x + 3,
                    row + 1,
                    width - 7,
                    field.valid(),
                    self.editor() == Some(index),
                );
                canvas.text(
                    x + 2,
                    row + 2,
                    &format!("└{}┘", "─".repeat(width - 6)),
                    field_style.clone(),
                    false,
                );
                if !field.valid() {
                    for col in x + 2..x + width - 2 {
                        let cell = &mut canvas.cells[(row + 1) * canvas.width + col];
                        if cell.bg.is_some_and(|bg| bg != palette.panel) {
                            cell.bg = Some(INVALID);
                        } else {
                            cell.fg = INVALID;
                        }
                    }
                }
                row += 3;
            }
            row += 1;
            let slots = self.field_slots();
            for (index, button) in self.buttons.iter().enumerate() {
                let mut button = button.clone();
                if !self.dismiss_button(index) && !self.fields.iter().all(Field::valid) {
                    button.enabled = false;
                }
                button.paint(
                    canvas,
                    x + 2,
                    row + index,
                    self.selected == slots + index,
                    false,
                );
            }
        }
        let hint = if self.wrapped > self.visible {
            "Esc: Back   PgUp/PgDn: Scroll"
        } else {
            "Esc: Back"
        };
        if scroll_hint {
            canvas.text(
                x + 2,
                y + height - 2,
                &clip(hint, width - 4),
                Cell {
                    fg: crate::widgets::SECONDARY,
                    ..style.clone()
                },
                false,
            );
        }
        for row in y..y + height {
            for col in x..x + width {
                let cell = &mut canvas.cells[row * canvas.width + col];
                if cell.bg.is_none() {
                    cell.bg = Some(palette.panel);
                }
                if matches!(cell.glyph.as_str(), "○" | "●") {
                    cell.fg = palette.accent;
                } else if cell.fg == [238; 3]
                    || cell.fg == [188; 3] && (!scroll_hint || row != y + height - 2)
                {
                    cell.fg = palette.text;
                }
            }
        }
    }
}
/// Prefer a space or dash; split a token only when the token exceeds the viewport.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = vec![];
    for paragraph in text.split('\n') {
        let mut remaining = paragraph;
        while UnicodeWidthStr::width(remaining) > width {
            let mut used = 0;
            let mut end = 0;
            let mut boundary = None;
            for (i, g) in remaining.grapheme_indices(true) {
                if used + UnicodeWidthStr::width(g) > width {
                    break;
                }
                used += UnicodeWidthStr::width(g);
                end = i + g.len();
                if g == " " || g == "-" {
                    boundary = Some(end);
                }
            }
            let end = boundary.filter(|n| *n > 0).unwrap_or(end);
            if end == 0 {
                break;
            }
            out.push(remaining[..end].trim_end().to_owned());
            remaining = remaining[end..].trim_start();
        }
        out.push(remaining.to_owned());
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    fn enter() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
    }
    #[test]
    fn title_touches_herdr_corner_without_padding_and_keeps_right_corner() {
        for title in [
            "Popup Title",
            "A deliberately long title ".repeat(6).as_str(),
        ] {
            let mut d = Dialog::new(title, vec!["Body".into()], &["Close"]);
            let mut canvas = Canvas::new(60, 24);
            d.paint(&mut canvas, &Palette::default());
            let start = canvas.cells.iter().position(|c| c.glyph == "┌").unwrap();
            let row = canvas.cells[start..start + 56]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>();
            assert!(row.starts_with(&format!("┌{}", clip(title, 54))));
            assert!(row.ends_with('┐'));
            if title == "Popup Title" {
                assert!(row.starts_with("┌Popup Title──"));
            }
        }
    }
    #[test]
    fn limited_editors_and_only_focused_field_has_a_cursor() {
        let mut d = Dialog::new("Edit", vec!["Names".into()], &["Save", "Cancel"]);
        let mut name = Field::new("Name", "").validated(Rule::AccountName);
        name.excluded = vec!["duplicate".into()];
        d.fields.push(name);
        d.fields.push(Field::new("Other", "second"));
        d.paste("duplicate");
        assert!(!d.fields[0].valid());
        d.selected = 2;
        d.paint(&mut Canvas::new(80, 32), &Palette::default());
        assert!(matches!(d.key(enter()), Event::None));
        d.selected = 0;
        d.fields[0].input.clear();
        d.paste("abcdefghijklmnopqrstuvwxyz123456789");
        assert_eq!(d.fields[0].input.value, "abcdefghijklmnopqrstuvwxyz1234");
        assert!(d.fields[0].valid());
        for selected in 0..4 {
            d.selected = selected;
            let mut canvas = Canvas::new(80, 32);
            d.paint(&mut canvas, &Palette::default());
            assert_eq!(canvas.cursor.is_some(), selected < 2);
            assert!(canvas.cells.iter().all(|c| c.bg != Some([238; 3])));
            assert!(canvas.cells.iter().all(|c| c.glyph != "▶"));
        }
    }
    #[test]
    fn single_editor_stays_live_while_arrows_select_only_buttons() {
        let mut d = Dialog::new("Edit", vec![], &["Save", "Cancel"]);
        d.fields.push(Field::new("Name", ""));
        for (code, selected, letter) in [(KeyCode::Down, 1, 'a'), (KeyCode::Up, 0, 'b')] {
            d.key(KeyEvent::new(code, KeyModifiers::NONE));
            assert_eq!(d.selected, selected);
            d.key(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE));
            let mut canvas = Canvas::new(80, 32);
            d.paint(&mut canvas, &Palette::default());
            assert!(canvas.cursor.is_some());
            assert!(canvas.cells.iter().all(|c| c.bg != Some([238; 3])));
            assert!(canvas.cells.iter().all(|c| c.glyph != "▶"));
        }
        assert_eq!(d.values(), vec!["ab"]);
        assert!(matches!(d.key(enter()), Event::Choose(0, _)));
    }
    #[test]
    fn dialog_hints_stay_readable_gray_when_the_menu_theme_animates() {
        let mut d = Dialog::new(
            "Confirm",
            (0..50).map(|i| format!("History row {i}")).collect(),
            &["Confirm", "Cancel"],
        );
        let mut field = crate::render::Field::new(80, 24, 1967);
        for now in [0, 1467, 4700] {
            let mut c = field.paint(now);
            d.paint(&mut c, &Palette::default());
            let cell = c.cells.iter().find(|cell| cell.glyph == "E").unwrap();
            assert_eq!(cell.fg, crate::widgets::SECONDARY);
            assert!(!cell.bold);
        }
    }
    #[test]
    fn wraps_words_dashes_and_unsplittable_graphemes() {
        assert_eq!(wrap("One two three", 8), vec!["One two", "three"]);
        assert_eq!(wrap("codex-personal", 8), vec!["codex-", "personal"]);
        let text = "/very/long/unbroken/αβ/é/👩‍💻";
        assert_eq!(wrap(text, 12).concat(), text);
    }
    #[test]
    fn cancel_works_before_reading_and_single_editor_never_takes_selection() {
        let mut d = Dialog::new(
            "Remove",
            (0..50).map(|i| format!("Session {i}")).collect(),
            &["Remove", "Cancel"],
        );
        d.fields.push(
            Field::new("Type codex-personal", "").validated(Rule::Exact("codex-personal".into())),
        );
        d.paint(&mut Canvas::new(60, 24), &Palette::default());
        assert!(matches!(d.key(enter()), Event::None));
        d.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        d.paste("codex-personal");
        assert_eq!(d.selected, 1);
        assert!(matches!(d.key(enter()), Event::Choose(1, _)));
        d.selected = 0;
        assert!(d.offset + d.visible < d.wrapped);
        assert!(matches!(d.key(enter()), Event::Choose(0, _)));
        for _ in 0..50 {
            d.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        }
        assert!(matches!(d.key(enter()), Event::Choose(0, _)));
    }
    #[test]
    fn invalid_field_stays_red_with_cancel_selected_and_short_body_has_no_scroll_hint() {
        let mut d = Dialog::new(
            "Remove",
            vec!["Keep history.".into()],
            &["Remove", "Cancel"],
        );
        d.fields.push(
            Field::new("Type codex-personal", "").validated(Rule::Exact("codex-personal".into())),
        );
        d.selected = 1;
        let mut c = Canvas::new(70, 24);
        d.paint(&mut c, &Palette::default());
        assert!(c.cells.iter().any(|c| c.fg == INVALID));
        let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
        assert!(!text.contains("PgUp"));
        assert!(!text.contains("Esc:"));
        d.paste("codex-personal");
        assert!(d.fields[0].valid());
    }
}
