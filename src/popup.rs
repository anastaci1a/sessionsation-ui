//! Shared flat popup surface. Content uses the normal editors/options in local cells.
use crate::{
    dialog::Palette,
    render::{Canvas, Cell},
    widgets::clip,
};

#[derive(Clone, Copy, Debug)]
pub struct Panel {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}
impl Panel {
    pub fn frame(self, canvas: &mut Canvas, title: &str, palette: &Palette) {
        if self.width < 2 || self.height < 2 {
            return;
        }
        let style = Cell {
            fg: palette.text,
            bg: Some(palette.panel),
            ..Cell::default()
        };
        let border = Cell {
            fg: palette.accent,
            ..style.clone()
        };
        for row in self.y..self.y + self.height {
            canvas.text(self.x, row, &" ".repeat(self.width), style.clone(), false);
        }
        for row in self.y + 1..self.y + self.height - 1 {
            canvas.text(self.x, row, "│", border.clone(), false);
            canvas.text(self.x + self.width - 1, row, "│", border.clone(), false);
        }
        canvas.text(
            self.x,
            self.y,
            &format!("┌{}┐", "─".repeat(self.width - 2)),
            border.clone(),
            false,
        );
        canvas.text(
            self.x,
            self.y + self.height - 1,
            &format!("└{}┘", "─".repeat(self.width - 2)),
            border.clone(),
            false,
        );
        canvas.text(
            self.x + 1,
            self.y,
            &clip(title, self.width - 2),
            border,
            false,
        );
    }
}
/// Measured once per opened picker and remeasured only on terminal resize.
#[derive(Clone, Copy, Debug)]
pub struct Size {
    pub viewport: (usize, usize),
    pub width: usize,
    pub height: usize,
}
impl Size {
    pub fn fit(viewport: (usize, usize), width: usize, height: usize) -> Self {
        Self {
            viewport,
            width: width.min(viewport.0.saturating_sub(4)),
            height: height.min(viewport.1.saturating_sub(4)),
        }
    }
}
/// Choice template: descriptive text, optional editor, navigable list and Cancel.
/// Geometry belongs to the opened view; asynchronous content never remeasures it.
pub struct Choices<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub rows: &'a [crate::widgets::Row],
    pub editor: Option<&'a crate::dialog::Field>,
    pub note: &'a str,
    pub note_accent: bool,
    pub reserve_note: bool,
    pub inline_note_width: usize,
    pub large: bool,
    pub pinned: usize,
    pub browse_hint: bool,
}
/// Natural content width, capped for readable wrapping and the current terminal.
pub fn content_width(
    viewport: usize,
    title: &str,
    lines: impl IntoIterator<Item = usize>,
) -> usize {
    use unicode_width::UnicodeWidthStr;
    (lines.into_iter().max().unwrap_or(0) + 4)
        .max(UnicodeWidthStr::width(title) + 2)
        .clamp(24, 76)
        .min(viewport.saturating_sub(4))
}
impl Choices<'_> {
    fn row_end(row: &crate::widgets::Row, width: usize) -> usize {
        use unicode_width::UnicodeWidthStr;
        row.preview_position(width, 0)
            .zip(row.preview.as_deref())
            .map_or(2 + UnicodeWidthStr::width(row.label.as_str()), |(x, s)| {
                x + UnicodeWidthStr::width(s)
            })
    }
    fn note_inline(&self, width: usize) -> bool {
        self.inline_note_width > 0
            && self.rows.iter().filter(|r| r.label != "Cancel").all(|r| {
                (r.preview.is_none() || r.preview_position(width, 0).is_some())
                    && Self::row_end(r, width) + 1 + self.inline_note_width <= width
            })
    }
    pub fn measure(&self, viewport: (usize, usize)) -> Size {
        use unicode_width::UnicodeWidthStr;
        let natural = self.rows.iter().map(|r| {
            Self::row_end(r, usize::MAX / 2)
                + 2
                + usize::from(self.inline_note_width > 0) * (1 + self.inline_note_width)
        });
        let width = content_width(
            viewport.0,
            self.title,
            natural
                .chain([UnicodeWidthStr::width(self.description)])
                .chain(
                    self.editor
                        .map(|f| UnicodeWidthStr::width(f.input.value.as_str()).max(24)),
                ),
        );
        let inner = width.saturating_sub(4);
        let description = crate::dialog::wrap(self.description, inner).len();
        let preview = usize::from(crate::widgets::Options::previews_detached(inner, self.rows)) * 2;
        let note =
            usize::from((self.reserve_note || !self.note.is_empty()) && !self.note_inline(inner))
                * 2;
        let height = if self.large {
            viewport.1.saturating_sub(4)
        } else {
            description + 1 + self.rows.len().max(1) + preview + note + 2
        };
        Size::fit(viewport, width, height)
    }
}
pub fn paint_choices(
    canvas: &mut Canvas,
    palette: &Palette,
    view: &Choices<'_>,
    options: &mut crate::widgets::Options,
    size: Size,
) {
    paint_sized(canvas, view.title, palette, size, |local| {
        let style = Cell {
            fg: palette.text,
            bg: Some(palette.panel),
            ..Cell::default()
        };
        let description = crate::dialog::wrap(view.description, local.width);
        let minimum = description.len() + 1 + usize::from(view.editor.is_some()) * 4 + 1;
        if local.height < minimum {
            local.text(
                0,
                0,
                &clip("Resize to view; Esc cancels", local.width),
                style,
                false,
            );
            return;
        }
        let mut top = 0;
        for line in description {
            local.text(0, top, &line, style.clone(), false);
            top += 1;
        }
        top += 1;
        if let Some(editor) = view.editor {
            editor.paint_box(local, 0, top, local.width, palette, true);
            top += 4;
        }
        let inline = view.note_inline(local.width);
        let note_height = (usize::from((view.reserve_note || !view.note.is_empty()) && !inline)
            * 2)
        .min(local.height.saturating_sub(
            top + usize::from(view.large)
                + 2
                + usize::from(crate::widgets::Options::previews_detached(
                    local.width,
                    view.rows,
                )) * 2,
        ));
        let bottom = local
            .height
            .saturating_sub(usize::from(view.large) + note_height);
        options.back = false;
        let (_, capacity) = if view.pinned > 0 {
            options.paint_pinned_menu(local, view.rows, view.pinned, top, bottom, bottom + 1)
        } else {
            options.paint_menu(local, view.rows, top, bottom, bottom + 1)
        };
        if !view.note.is_empty() {
            let selected_y = if view.pinned > 0
                && capacity > 5
                && view.rows.len() > capacity
                && options.selected < view.pinned
            {
                Some(top + options.selected)
            } else {
                options
                    .selected
                    .checked_sub(options.scroll)
                    .filter(|i| *i < capacity)
                    .map(|i| top + i)
            };
            let (x, y) = if inline {
                view.rows
                    .get(options.selected)
                    .zip(selected_y)
                    .map(|(r, y)| (Choices::row_end(r, local.width) + 1, y))
                    .unwrap_or((0, local.height))
            } else {
                let y = top
                    + view.rows.len().min(capacity)
                    + usize::from(view.rows.len() > capacity)
                    + 1;
                (
                    (local.width.saturating_sub(view.note.chars().count())) / 2,
                    y.min(local.height.saturating_sub(usize::from(view.large) + 1)),
                )
            };
            local.text(
                x,
                y,
                &clip(view.note, local.width.saturating_sub(x)),
                Cell {
                    fg: if view.note_accent {
                        palette.accent
                    } else {
                        crate::widgets::SECONDARY
                    },
                    ..style.clone()
                },
                false,
            );
        }
        if view.browse_hint {
            local.text(
                0,
                local.height.saturating_sub(1),
                "◀/▶: Parent/Open",
                Cell {
                    fg: crate::widgets::SECONDARY,
                    ..style
                },
                false,
            );
        }
    });
}
pub fn paint(
    canvas: &mut Canvas,
    title: &str,
    palette: &Palette,
    desired_height: usize,
    content: impl FnOnce(&mut Canvas),
) {
    let size = Size::fit((canvas.width, canvas.height), 76, desired_height);
    paint_sized(canvas, title, palette, size, content);
}
pub fn paint_sized(
    canvas: &mut Canvas,
    title: &str,
    palette: &Palette,
    size: Size,
    content: impl FnOnce(&mut Canvas),
) {
    let Size { width, height, .. } = size;
    if width < 8 || height < 4 {
        return;
    }
    let panel = Panel {
        x: (canvas.width - width) / 2,
        y: (canvas.height - height) / 2,
        width,
        height,
    };
    canvas.overlay(|canvas| {
        panel.frame(canvas, title, palette);
        let mut local = Canvas::new(width - 4, height - 2);
        local.static_accent = Some(palette.accent);
        content(&mut local);
        for row in 0..local.height {
            for col in 0..local.width {
                let mut cell = local.cells[row * local.width + col].clone();
                cell.bg = Some(cell.bg.unwrap_or(palette.panel));
                canvas.cells[(panel.y + 1 + row) * canvas.width + panel.x + 2 + col] = cell;
            }
        }
        canvas.cursor = local
            .cursor
            .map(|(x, y)| (panel.x + 2 + x, panel.y + 1 + y));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        render::{Accent, Field},
        widgets::TextInput,
    };
    #[test]
    fn choices_fit_exactly_and_keep_cancel_visible_with_detached_details() {
        let palette = Palette::default();
        for viewport in [(100, 30), (40, 16)] {
            let rows = vec![
                crate::widgets::Row::yes("First")
                    .left_aligned()
                    .with_selected_preview("/a/long/directory/that/must/move/below"),
                crate::widgets::Row::yes("Second").left_aligned(),
                crate::widgets::Row::yes("Cancel").left_aligned(),
            ];
            let mut view = Choices {
                title: "Choose",
                description: "Choose an item.",
                rows: &rows,
                editor: None,
                note: "",
                note_accent: false,
                reserve_note: false,
                inline_note_width: 0,
                large: false,
                pinned: 0,
                browse_hint: false,
            };
            let size = view.measure(viewport);
            let detached = crate::widgets::Options::previews_detached(size.width - 4, &rows);
            assert_eq!(size.height, 7 + usize::from(detached) * 2);
            let mut options = crate::widgets::Options::default();
            let mut c = Canvas::new(viewport.0, viewport.1);
            paint_choices(&mut c, &palette, &view, &mut options, size);
            let lines: Vec<String> = c
                .cells
                .chunks(c.width)
                .map(|r| r.iter().map(|c| c.glyph.as_str()).collect())
                .collect();
            let first = lines.iter().position(|l| l.contains("First")).unwrap();
            let desc = lines
                .iter()
                .position(|l| l.contains("Choose an item."))
                .unwrap();
            assert_eq!(first, desc + 2);
            let cancel = lines.iter().position(|l| l.contains("Cancel")).unwrap();
            assert_eq!(cancel, first + 2);
            assert_eq!(
                lines[first].find('○').or_else(|| lines[first].find('●')),
                lines[cancel].find('○').or_else(|| lines[cancel].find('●'))
            );
            view.note = "Asynchronously updated";
            options.selected = rows.len() - 1;
            let mut c = Canvas::new(viewport.0, viewport.1);
            paint_choices(&mut c, &palette, &view, &mut options, size);
            assert!(
                c.cells
                    .iter()
                    .map(|c| c.glyph.as_str())
                    .collect::<String>()
                    .contains("Cancel"),
                "{:?}",
                c.cells
                    .chunks(c.width)
                    .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>())
                    .collect::<Vec<_>>()
            );
        }
    }
    #[test]
    fn popup_owns_native_cursor_and_uses_flat_colors() {
        let palette = Palette::default();
        let mut c = Canvas::new(100, 30);
        c.cursor = Some((1, 1));
        paint(&mut c, "Editor", &palette, 8, |local| {
            TextInput::new("界".into()).paint(local, 1, 1, 20, true);
            local.accent_text(1, 2, "Choice", Accent::default());
        });
        assert_eq!(c.cursor, Some((19, 13)));
        assert!(
            c.cells
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
                .contains("┌Editor")
        );
        let choice = c.cells.iter().position(|c| c.glyph == "C").unwrap();
        assert_eq!(c.cells[choice].fg, palette.accent);
        assert_eq!(c.cells[choice].bg, Some(palette.panel));
    }
    #[test]
    fn popup_does_not_delete_stars_behind_it() {
        let mut covered = Field::new(80, 24, 1967);
        let mut control = Field::new(80, 24, 1967);
        for now in (0..6000).step_by(20) {
            let mut c = covered.paint(now);
            let plain = control.paint(now);
            paint(&mut c, "Chooser", &Palette::default(), 12, |local| {
                local.text(0, 0, "Synthetic choices", Cell::default(), true);
            });
            covered.apply_masks(&c);
            control.apply_masks(&plain);
        }
        assert_eq!(covered.paint(6020).cells, control.paint(6020).cells);
    }
}
