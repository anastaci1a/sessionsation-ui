//! Multiline text input: wrapping at word boundaries, cursor movement through the
//! wrapped lines and painting. Used by dialog fields ([`crate::dialog::Field::multiline`]);
//! Enter never inserts a newline (the dialog's default button takes it).
use crate::render::{Canvas, Cell};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The wrapped lines of `text` as byte ranges that together cover it exactly. A line
/// breaks after the last space that fits; a word wider than `width` breaks inside.
pub fn wrap(text: &str, width: usize) -> Vec<(usize, usize)> {
    let width = width.max(1);
    let mut lines = vec![];
    let (mut start, mut used, mut brk) = (0, 0, None::<usize>);
    for (i, g) in text.grapheme_indices(true) {
        let w = UnicodeWidthStr::width(g);
        while used + w > width && i > start {
            let cut = brk.filter(|b| *b > start && *b <= i).unwrap_or(i);
            lines.push((start, cut));
            start = cut;
            used = UnicodeWidthStr::width(&text[cut..i]);
            brk = None;
        }
        used += w;
        if g.chars().all(char::is_whitespace) {
            brk = Some(i + g.len());
        }
    }
    lines.push((start, text.len()));
    lines
}

/// Row and column (in cells) of the byte offset `cursor`. A cursor at a break belongs to
/// the start of the next line; the end of the text to the end of the last.
pub fn locate(text: &str, cursor: usize, lines: &[(usize, usize)]) -> (usize, usize) {
    let last = lines.len().saturating_sub(1);
    let row = lines
        .iter()
        .position(|(s, e)| cursor >= *s && cursor < *e)
        .unwrap_or(last);
    let start = lines.get(row).map_or(0, |l| l.0);
    (
        row,
        UnicodeWidthStr::width(&text[start..cursor.min(text.len())]),
    )
}

/// The cursor one wrapped line up or down at the same column (clamped to the line),
/// or `None` on the first/last line.
pub fn vertical(text: &str, cursor: usize, width: usize, up: bool) -> Option<usize> {
    let lines = wrap(text, width);
    let (row, col) = locate(text, cursor, &lines);
    let target = if up { row.checked_sub(1)? } else { row + 1 };
    let (start, end) = *lines.get(target)?;
    let mut at = start;
    let mut used = 0;
    for (i, g) in text[start..end].grapheme_indices(true) {
        let w = UnicodeWidthStr::width(g);
        // A line that is not the last ends where the next begins: never land there.
        let last_in_line = start + i + g.len() == end;
        if used + w > col || (last_in_line && target + 1 < lines.len()) {
            at = start + i;
            return Some(at);
        }
        used += w;
        at = start + i + g.len();
    }
    Some(at)
}

/// Paint `text` (or the grey `hint` while it is empty) in a `width` x `rows` area.
/// One column stays free so a cursor at the end of a full line has a cell. The view
/// scrolls so the cursor row is always visible.
#[allow(clippy::too_many_arguments)]
pub fn paint(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    rows: usize,
    text: &str,
    cursor: usize,
    hint: &str,
    style: &Cell,
    hint_style: &Cell,
    focused: bool,
) {
    let wrap_width = width.saturating_sub(1).max(1);
    if text.is_empty() {
        for (i, (s, e)) in wrap(hint, wrap_width).into_iter().take(rows).enumerate() {
            canvas.widget_text(x, y + i, hint[s..e].trim_end(), hint_style.clone(), false);
        }
    }
    let lines = wrap(text, wrap_width);
    let (crow, ccol) = locate(text, cursor, &lines);
    let top = (crow + 1).saturating_sub(rows);
    for (i, (s, e)) in lines.iter().skip(top).take(rows).enumerate() {
        if !text.is_empty() && y + i < canvas.height {
            canvas.widget_text(x, y + i, &text[*s..*e], style.clone(), false);
        }
    }
    let (cx, cy) = (x + ccol, y + crow - top);
    if focused && cx < canvas.width && cy < canvas.height {
        // The glyph under the cursor is already drawn (text, hint or blank).

        canvas.cursor = Some((cx, cy));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(text: &str, width: usize) -> Vec<&str> {
        wrap(text, width)
            .into_iter()
            .map(|(s, e)| &text[s..e])
            .collect()
    }

    #[test]
    fn wraps_at_word_boundaries_and_covers_the_text_exactly() {
        let t = "the quick brown fox";
        assert_eq!(texts(t, 10), ["the quick ", "brown fox"]);
        assert_eq!(texts(t, 4), ["the ", "quic", "k ", "brow", "n ", "fox"]);
        assert_eq!(texts("", 5), [""]);
        for w in 1..24 {
            assert_eq!(texts(t, w).concat(), t, "width {w}");
        }
    }

    #[test]
    fn multibyte_and_emoji_use_cell_widths() {
        // Each CJK char and emoji is two cells wide.
        assert_eq!(texts("日本語日本語", 6), ["日本語", "日本語"]);
        assert_eq!(texts("a😀b😀c", 3), ["a😀", "b😀", "c"]);
        // Combining marks stay with their base.
        let t = "e\u{301}e\u{301}e\u{301}";
        assert_eq!(texts(t, 2), ["e\u{301}e\u{301}", "e\u{301}"]);
        // A glyph wider than the box still gets a line of its own.
        assert_eq!(texts("😀😀", 1), ["😀", "😀"]);
    }

    #[test]
    fn locate_puts_a_break_on_the_next_line_and_the_end_on_the_last() {
        let t = "the quick brown fox";
        let lines = wrap(t, 10);
        assert_eq!(locate(t, 0, &lines), (0, 0));
        assert_eq!(locate(t, 9, &lines), (0, 9));
        assert_eq!(locate(t, 10, &lines), (1, 0));
        assert_eq!(locate(t, t.len(), &lines), (1, 9));
        assert_eq!(locate("", 0, &wrap("", 5)), (0, 0));
        // A full last line: the cursor sits one cell past it.
        let full = "abcde";
        assert_eq!(locate(full, 5, &wrap(full, 5)), (0, 5));
    }

    #[test]
    fn up_and_down_move_across_wrapped_lines_keeping_the_column() {
        let t = "the quick brown fox";
        // "the quick " / "brown fox": column 4 of the first line is 'q'.
        assert_eq!(vertical(t, 4, 10, false), Some(14));
        assert_eq!(vertical(t, 14, 10, true), Some(4));
        // Shorter target: clamp to the end (the last line may end at the text end).
        assert_eq!(vertical(t, 9, 10, false), Some(19));
        // Ends: nothing to move to.
        assert_eq!(vertical(t, 4, 10, true), None);
        assert_eq!(vertical(t, 14, 10, false), None);
        // Landing in a line that is not the last never lands on the next line's start.
        let u = "aaa bbb ccc";
        let to = vertical(u, 0, 4, false).unwrap();
        assert_eq!(to, 4);
        assert_eq!(vertical(u, 3, 4, false), Some(7));
        assert_eq!(vertical(u, 3, 4, true), None);
    }

    #[test]
    fn vertical_movement_respects_wide_glyphs_and_the_end_of_text() {
        let t = "日本語日本語";
        // width 6: two lines of three wide chars; column 4 is the start of the 3rd char.
        assert_eq!(vertical(t, 6, 6, false), Some(15));
        let u = "日本日本";
        assert_eq!(vertical(u, 3, 4, false), Some(9));
        // At the very end the last line has no line below.
        assert_eq!(vertical(t, t.len(), 6, false), None);
    }
}
