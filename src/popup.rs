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
/// Leave enough centered room for the expanded Back arrow whenever possible.
pub fn picker_width(rows: &[crate::widgets::Row]) -> usize {
    use unicode_width::UnicodeWidthStr;
    let longest = rows
        .iter()
        .map(|row| {
            row.preview_column
                .zip(row.preview.as_deref())
                .map_or(UnicodeWidthStr::width(row.label.as_str()), |(col, text)| {
                    col.saturating_sub(2) + UnicodeWidthStr::width(text)
                })
        })
        .max()
        .unwrap_or(0);
    (longest + 24).clamp(48, 76)
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
