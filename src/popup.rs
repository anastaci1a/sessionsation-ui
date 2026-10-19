//! Shared flat popup surface. Content uses the normal editors/options in local cells.
const DIRECTORY_WIDTH_PERCENT: usize = 60;
const DIRECTORY_HEIGHT_PERCENT: usize = 90;

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
/// Opening geometry and detail budget. Visible selections may grow it;
/// terminal resize measures it afresh. Polling alone never changes geometry.
#[derive(Clone, Copy, Debug)]
pub struct Size {
    pub viewport: (usize, usize),
    pub width: usize,
    pub height: usize,
    pub note_width: usize,
}
impl Size {
    pub fn fit(viewport: (usize, usize), width: usize, height: usize) -> Self {
        Self {
            viewport,
            width: width.min(viewport.0.saturating_sub(4)),
            height: height.min(viewport.1.saturating_sub(4)),
            note_width: 0,
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
        let natural = self
            .rows
            .iter()
            .map(|r| {
                let status = if self.reserve_note && !r.separator && r.label != "Cancel" {
                    1 + self.inline_note_width
                } else {
                    0
                };
                Self::row_end(r, usize::MAX / 2) + status
            })
            .max()
            .unwrap_or(0);
        let width = if self.editor.is_some() {
            // Directory browsing has a stable, useful viewport independent of paths.
            (viewport.0 * DIRECTORY_WIDTH_PERCENT / 100).max(28)
        } else {
            (natural + 4)
                .max(UnicodeWidthStr::width(self.title) + 2)
                .max(8)
        }
        .min(viewport.0.saturating_sub(4));
        self.fit_width(viewport, width)
    }
    fn fit_width(&self, viewport: (usize, usize), width: usize) -> Size {
        let inner = width.saturating_sub(4);
        let description = crate::dialog::wrap(self.description, inner).len();
        let preview = usize::from(crate::widgets::Options::previews_detached(inner, self.rows)) * 2;
        let note =
            usize::from((self.reserve_note || !self.note.is_empty()) && !self.note_inline(inner))
                * 2;
        let height = if self.large {
            (viewport.1 * DIRECTORY_HEIGHT_PERCENT / 100)
                .max(description + 1 + usize::from(self.editor.is_some()) * 4 + self.pinned + 4)
        } else {
            description + 1 + self.rows.len().max(1) + preview + note + 2
        };
        let mut size = Size::fit(viewport, width, height);
        // Match viewport parity so centering leaves identical top/bottom margins.
        if self.editor.is_some() && size.height % 2 != viewport.1 % 2 {
            size.height = size.height.saturating_sub(1);
        }
        size.note_width = self.inline_note_width;
        size
    }
    /// Polling never changes geometry. Only the selected, visible detail may
    /// increase the opening budget; keep this growth until close or resize.
    pub fn grow_for_selection(&mut self, opened: Size, selected: usize) -> Size {
        use unicode_width::UnicodeWidthStr;
        self.inline_note_width = opened.note_width;
        if self.editor.is_some() {
            return opened;
        }
        let note = UnicodeWidthStr::width(self.note);
        let mut required = opened.width;
        if self.reserve_note && note > opened.note_width {
            self.inline_note_width = note;
            if let Some(row) = self.rows.get(selected) {
                required = required.max(Self::row_end(row, usize::MAX / 2) + 1 + note + 4);
            }
        }
        if let Some(row) = self.rows.get(selected) {
            required = required.max(UnicodeWidthStr::width(row.label.as_str()) + 4);
            if row.preview_selected_only && row.preview.is_some() {
                required = required.max(Self::row_end(row, usize::MAX / 2) + 4);
            }
        }
        let width = required.min(opened.viewport.0.saturating_sub(4));
        if width > opened.width || self.inline_note_width > opened.note_width {
            let mut next = self.fit_width(opened.viewport, width);
            next.height = next.height.max(opened.height);
            next
        } else {
            opened
        }
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
        let menu_bottom = local.height.saturating_sub(usize::from(view.large));
        let bottom = menu_bottom.saturating_sub(note_height);
        options.back = false;
        let (_, capacity) = if view.pinned > 0 {
            options.paint_pinned_menu(local, view.rows, view.pinned, top, menu_bottom, bottom + 1)
        } else {
            options.paint_menu(local, view.rows, top, bottom, bottom + 1)
        };
        if !view.note.is_empty() {
            let available = bottom.saturating_sub(top);
            let pinned_active = view.pinned > 0
                && view.rows.len() > available
                && available.saturating_sub(1) > 5
                && view.pinned < view.rows.len();
            let selected_y = if pinned_active && options.selected < view.pinned {
                Some(top + options.selected)
            } else {
                options
                    .selected
                    .checked_sub(options.scroll)
                    .filter(|i| {
                        *i < capacity.saturating_sub(if pinned_active { view.pinned } else { 0 })
                    })
                    .map(|i| top + i + if pinned_active { view.pinned } else { 0 })
            };
            let (x, y) = if inline {
                view.rows
                    .get(options.selected)
                    .zip(selected_y)
                    .map(|(r, y)| (Choices::row_end(r, local.width) + 1, y))
                    .unwrap_or((0, local.height))
            } else {
                let footer = usize::from(
                    view.pinned > 0
                        && available.saturating_sub(usize::from(view.rows.len() > available)) > 5
                        && view.rows.last().is_some_and(|r| r.label == "Cancel")
                        && view
                            .rows
                            .get(view.rows.len().saturating_sub(2))
                            .is_some_and(|r| r.separator)
                        && available > view.pinned + 3,
                ) * 2;
                let y = top
                    + view
                        .rows
                        .len()
                        .saturating_sub(footer)
                        .min(capacity.saturating_sub(footer))
                    + usize::from(view.rows.len() > capacity)
                    + 1;
                (
                    (local.width.saturating_sub(view.note.chars().count())) / 2,
                    y.min(menu_bottom.saturating_sub(footer + 1)),
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
    fn assert_one_space_before_border(canvas: &Canvas, text: &str) {
        let row = canvas
            .cells
            .chunks(canvas.width)
            .find(|r| {
                r.iter()
                    .map(|c| c.glyph.as_str())
                    .collect::<String>()
                    .contains(text)
            })
            .unwrap();
        let border = row.iter().rposition(|c| c.glyph == "│").unwrap();
        assert_eq!(row[border - 1].glyph, " ");
        assert_ne!(row[border - 2].glyph, " ");
    }
    #[test]
    fn account_status_has_exactly_one_trailing_space_on_open_and_growth() {
        use crate::widgets::{Options, Row};
        let rows = vec![
            Row::yes("Cancel").popup_option(),
            Row::separator().popup_option(),
            Row::yes("personal")
                .with_column_preview("90% wk", 11, false)
                .preview_gap(1)
                .popup_option(),
        ];
        let mut view = Choices {
            title: "Choose Account",
            description: "Choose an account for this session.",
            rows: &rows,
            editor: None,
            note: "Ready",
            note_accent: true,
            reserve_note: true,
            inline_note_width: 5,
            large: false,
            pinned: 2,
            browse_hint: false,
        };
        let mut options = Options {
            selected: 2,
            ..Options::default()
        };
        let opened = view.measure((100, 30));
        let mut canvas = Canvas::new(100, 30);
        paint_choices(
            &mut canvas,
            &Palette::default(),
            &view,
            &mut options,
            opened,
        );
        assert_one_space_before_border(&canvas, "Ready");
        view.note = "Login required";
        let grown = view.grow_for_selection(opened, 2);
        let mut canvas = Canvas::new(100, 30);
        paint_choices(&mut canvas, &Palette::default(), &view, &mut options, grown);
        assert_one_space_before_border(&canvas, "Login required");
    }
    #[test]
    fn space_ids_are_inline_with_one_trailing_space_unless_viewport_is_too_thin() {
        use crate::widgets::{Options, Row};
        let rows = vec![
            Row::yes("Longer space")
                .with_selected_preview("(space-id-012345)")
                .preview_gap(1)
                .popup_option(),
            Row::yes("Short")
                .with_selected_preview("(id)")
                .preview_gap(1)
                .popup_option(),
        ];
        let view = Choices {
            title: "Choose Space",
            description: "Choose a destination Space.",
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
        let size = view.measure((100, 30));
        assert!(!Options::previews_detached(size.width - 4, &rows));
        let mut canvas = Canvas::new(100, 30);
        paint_choices(
            &mut canvas,
            &Palette::default(),
            &view,
            &mut Options::default(),
            size,
        );
        assert_one_space_before_border(&canvas, "(space-id-012345)");
        let narrow = view.measure((30, 24));
        assert!(Options::previews_detached(narrow.width - 4, &rows));
    }
    #[test]
    fn hidden_space_ids_do_not_expand_the_frame_until_selected() {
        use crate::widgets::Row;
        let rows = vec![
            Row::yes("One").popup_option().with_selected_preview("(a)"),
            Row::yes("Two").popup_option().with_selected_preview("(b)"),
        ];
        let mut view = Choices {
            title: "Choose Space",
            description: "A long description wraps instead of making the popup wide.",
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
        let opened = view.measure((100, 30));
        assert_eq!(opened.width, 14);
        let updated = vec![
            rows[0].clone(),
            Row::yes("Two")
                .popup_option()
                .with_selected_preview("A newly observed Space identifier that is longer"),
        ];
        view.rows = &updated;
        assert_eq!(view.grow_for_selection(opened, 0).width, opened.width);
        assert!(view.grow_for_selection(opened, 1).width > opened.width);
    }
    #[test]
    fn directory_path_length_does_not_control_popup_width() {
        use crate::widgets::Row;
        let rows = vec![
            Row::yes("Use this directory"),
            Row::yes("Create subdirectory"),
            Row::separator(),
            Row::placeholder("(none)"),
            Row::separator(),
            Row::yes("Cancel"),
        ];
        let editor =
            crate::dialog::Field::new("Directory", format!("/{}", "long-component/".repeat(30)));
        let mut view = Choices {
            title: "Start directory",
            description: "Choose a directory. Directories are only created during Start.",
            rows: &rows,
            editor: Some(&editor),
            note: "",
            note_accent: false,
            reserve_note: false,
            inline_note_width: 0,
            large: true,
            pinned: 3,
            browse_hint: false,
        };
        for height in 16..45 {
            let measured = view.measure((100, height));
            assert_eq!((height - measured.height) % 2, 0);
            assert!(measured.height <= height - 4);
        }
        let opened = view.measure((100, 30));
        assert_eq!((opened.width, opened.height), (60, 26));
        let larger = view.measure((200, 40));
        assert_eq!((larger.width, larger.height), (120, 36));
        assert_eq!(view.grow_for_selection(opened, 4).width, opened.width);
    }
    #[test]
    fn choices_fit_exactly_and_keep_cancel_visible_with_detached_details() {
        let palette = Palette::default();
        for viewport in [(100, 30), (40, 16)] {
            let rows = vec![
                crate::widgets::Row::yes("First")
                    .popup_option()
                    .with_selected_preview("/a/long/directory/that/must/move/below"),
                crate::widgets::Row::yes("Second").popup_option(),
                crate::widgets::Row::yes("Cancel").popup_option(),
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
