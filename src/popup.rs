//! Shared flat popup surface. Content uses the normal editors/options in local cells.
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
            // HERDR draws popup titles in the border's own style, never bold.
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
/// Finish a choice list with one bullet spacer and Cancel, without mounting rows.
pub fn with_cancel(
    rows: impl IntoIterator<Item = crate::widgets::Row>,
) -> Vec<crate::widgets::Row> {
    use crate::widgets::Row;
    let mut rows: Vec<_> = rows.into_iter().collect();
    while rows.last().is_some_and(|r| r.separator) {
        rows.pop();
    }
    if !rows.is_empty() {
        rows.push(Row::separator());
    }
    rows.push(Row::yes("Cancel"));
    rows
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
    pub large: bool,
    pub fit_description: bool,
    pub browse_hint: bool,
    /// Mounted (head, tail) rows around a scrolling list, when the view has them.
    pub sections: Option<(usize, usize)>,
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
    pub fn measure(&self, viewport: (usize, usize)) -> Size {
        use unicode_width::UnicodeWidthStr;
        let natural = self.rows.iter().map(|r| r.content_end()).max().unwrap_or(0);
        let description = self
            .description
            .lines()
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0);
        let width = if self.editor.is_some() || self.fit_description {
            description + 4
        } else {
            (natural.max(description) + 4)
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
        let note = usize::from(!self.note.is_empty()) * 2;
        let height = if self.large {
            (viewport.1 * DIRECTORY_HEIGHT_PERCENT / 100)
                .max(description + 1 + usize::from(self.editor.is_some()) * 4 + 4)
        } else {
            description + 1 + self.rows.len().max(1) + preview + note + 2
        };
        let mut size = Size::fit(viewport, width, height);
        // Match viewport parity so centering leaves identical top/bottom margins.
        if self.editor.is_some() && size.height % 2 != viewport.1 % 2 {
            size.height = size.height.saturating_sub(1);
        }
        size
    }
    /// Polling never changes geometry. Only the selected, visible detail may
    /// increase the opening budget; keep this growth until close or resize.
    pub fn grow_for_selection(&mut self, opened: Size, selected: usize) -> Size {
        use unicode_width::UnicodeWidthStr;
        if self.editor.is_some() || self.fit_description {
            return opened;
        }
        let mut required = opened.width;
        // Statuses are always visible, so polling must reserve every row's width.
        for row in self.rows.iter().filter(|r| r.status.is_some()) {
            required = required.max(row.content_end() + 4);
        }
        if let Some(row) = self.rows.get(selected) {
            required = required.max(UnicodeWidthStr::width(row.label.as_str()) + 4);
            if row.preview_selected_only && !row.preview_info && row.preview.is_some() {
                required = required.max(Self::row_end(row, usize::MAX / 2) + 4);
            }
        }
        let width = required.min(opened.viewport.0.saturating_sub(4));
        if width > opened.width {
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
        if local.height < minimum
            || view
                .rows
                .iter()
                .filter_map(|r| r.status.as_ref())
                .any(|(s, _)| unicode_width::UnicodeWidthStr::width(s.as_str()) + 3 > local.width)
        {
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
        let note_height =
            (usize::from(!view.note.is_empty()) * 2).min(local.height.saturating_sub(
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
        let footer = view
            .sections
            .map_or(usize::from(view.editor.is_some()) * 2, |(_, tail)| tail);
        let (_, capacity) = if let Some((head, tail)) = view.sections {
            options.paint_sections(local, view.rows, head, tail, top, menu_bottom, bottom + 1)
        } else if footer > 0 {
            options.paint_footer_menu(local, view.rows, top, menu_bottom, bottom + 1)
        } else {
            options.paint_menu(local, view.rows, top, bottom, bottom + 1)
        };
        if !view.note.is_empty() {
            let y = top
                + view
                    .rows
                    .len()
                    .saturating_sub(footer)
                    .min(capacity.saturating_sub(footer))
                + usize::from(footer == 0 && view.rows.len() > capacity)
                + 1;
            let (x, y) = (
                (local.width.saturating_sub(view.note.chars().count())) / 2,
                y.min(menu_bottom.saturating_sub(footer + 1)),
            );
            local.text(
                x,
                y,
                &clip(view.note, local.width.saturating_sub(x)),
                Cell {
                    fg: if view.note_accent {
                        palette.accent
                    } else {
                        palette.secondary
                    },
                    ..style.clone()
                },
                false,
            );
        }
        if view.browse_hint {
            crate::widgets::paint_centered_hint(
                local,
                0,
                local.height.saturating_sub(1),
                local.width,
                "◀/▶: Parent/Open",
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
        local.popup_palette = Some(palette.clone());
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
    fn cancel_spacer_is_exactly_one_row_and_empty_lists_only_offer_cancel() {
        use crate::widgets::Row;
        for rows in [
            vec![],
            vec![Row::yes("A")],
            vec![Row::yes("A"), Row::separator(), Row::separator()],
        ] {
            let rows = with_cancel(rows);
            assert_eq!(rows.last().unwrap().label, "Cancel");
            if rows.len() > 1 {
                assert_eq!(rows.len(), 3);
                assert!(rows[1].separator);
            } else {
                assert_eq!(rows.len(), 1);
            }
        }
    }
    #[test]
    fn description_width_fits_a_sentence_with_one_space_before_the_border() {
        use crate::widgets::{Options, Row};
        for (title, sentence) in [
            ("Choose account", "Choose an account for this session."),
            ("Choose Space", "Choose a destination Space."),
        ] {
            let rows = with_cancel([Row::yes("Example").popup_option()]);
            let view = Choices {
                title,
                description: sentence,
                rows: &rows,
                editor: None,
                note: "",
                note_accent: false,
                large: false,
                fit_description: true,
                browse_hint: false,
                sections: None,
            };
            let size = view.measure((120, 30));
            assert_eq!(size.width, sentence.len() + 4);
            let mut c = Canvas::new(120, 30);
            paint_choices(
                &mut c,
                &Palette::default(),
                &view,
                &mut Options::default(),
                size,
            );
            assert_one_space_before_border(&c, sentence);
        }
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
            large: false,
            fit_description: false,
            browse_hint: false,
            sections: None,
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
            description: "Choose.",
            rows: &rows,
            editor: None,
            note: "",
            note_accent: false,
            large: false,
            fit_description: false,
            browse_hint: false,
            sections: None,
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
            description: "Choose a directory.\nDirectories are only created during Start.",
            rows: &rows,
            editor: Some(&editor),
            note: "",
            note_accent: false,
            large: true,
            fit_description: false,
            browse_hint: false,
            sections: None,
        };
        for height in 16..45 {
            let measured = view.measure((100, height));
            assert_eq!((height - measured.height) % 2, 0);
            assert!(measured.height <= height - 4);
        }
        let opened = view.measure((100, 30));
        assert_eq!((opened.width, opened.height), (46, 26));
        let larger = view.measure((200, 40));
        assert_eq!((larger.width, larger.height), (46, 36));
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
                large: false,
                fit_description: false,
                browse_hint: false,
                sections: None,
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
    fn directory_controls_use_host_text_independent_of_animation_and_coordinates() {
        use crate::{
            render::Field,
            widgets::{Options, Row},
        };
        let palette = Palette::default();
        let rows = with_cancel((0..30).map(|i| Row::yes(format!("entry{i:02}/")).popup_option()));
        let editor = crate::dialog::Field::new("Directory", "/projects");
        let view = Choices {
            title: "Directory",
            description: "Choose a directory.\nNew directories are created only when the session starts.",
            rows: &rows,
            editor: Some(&editor),
            note: "",
            note_accent: false,
            large: true,
            fit_description: false,
            browse_hint: true,
            sections: None,
        };
        for width in [60, 100] {
            for now in [0, 1467, 4700] {
                let mut field = Field::new(width, 30, 1967);
                let mut c = field.paint(now);
                let size = view.measure((width, 30));
                let mut options = Options {
                    selected: rows.len() - 1,
                    ..Options::default()
                };
                paint_choices(&mut c, &palette, &view, &mut options, size);
                let hint = "◀/▶: Parent/Open";
                let x = (width - size.width) / 2
                    + 2
                    + (size.width - 4 - unicode_width::UnicodeWidthStr::width(hint)) / 2;
                let y = (30 - size.height) / 2 + size.height - 2;
                assert_eq!(c.cells[y * width + x].glyph, "◀");
                for col in x..x + unicode_width::UnicodeWidthStr::width(hint) {
                    let cell = &c.cells[y * width + col];
                    assert_eq!(
                        cell.fg,
                        if col < x + 3 {
                            palette.text
                        } else {
                            palette.muted
                        }
                    );
                    assert_eq!(cell.bg, Some(palette.panel));
                    assert!(!cell.bold);
                }
                let cancel = c.cells.iter().position(|c| c.glyph == "●").unwrap();
                assert_eq!(c.cells[cancel].fg, palette.accent);
            }
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
