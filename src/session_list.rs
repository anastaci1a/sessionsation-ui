//! Reference session columns, groups and selected-only elapsed-time suffix.
use crate::{
    render::{Accent, Canvas, Cell},
    usage::short_name,
    widgets::{Options, Row, clip},
};
use serde_json::Value;
use unicode_width::UnicodeWidthStr;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Focus,
    Details,
    Rename,
    Resume,
    Switch,
    Restart,
    Stop,
    Unavailable,
}
pub fn actions(s: &Value, resume: bool) -> Vec<(Row, Action)> {
    let c = &s["capabilities"];
    let mut rows = vec![
        (
            Row::gate(
                "Focus Session",
                s["availability"] == "running" && s["attachment"]["host"] == "herdr",
            ),
            Action::Focus,
        ),
        (Row::yes("Session Details"), Action::Details),
        (
            Row::gate("Rename Session", c["rename"] == true),
            Action::Rename,
        ),
    ];
    if c["resume"] == true {
        rows.push((Row::gate("Resume Session", resume), Action::Resume));
    }
    rows.extend([
        (Row::separator(), Action::Unavailable),
        (
            Row::gate("Switch account", c["switch_account"] == true),
            Action::Switch,
        ),
        (Row::separator(), Action::Unavailable),
        (Row::gate("Move project folder", false), Action::Unavailable),
        (
            Row::gate("Change working directory", false),
            Action::Unavailable,
        ),
        (Row::separator(), Action::Unavailable),
        (Row::gate("Pause work", false), Action::Unavailable),
        (
            Row::gate("Restart Codex", c["restart"] == true),
            Action::Restart,
        ),
        (Row::gate("End session", c["stop"] == true), Action::Stop),
        (Row::separator(), Action::Unavailable),
        (Row::gate("Delete conversation", false), Action::Unavailable),
        (Row::separator(), Action::Unavailable),
        (
            Row::gate("Experimental auto-start after reset", false),
            Action::Unavailable,
        ),
        (
            Row::gate("Experimental auto-switch", false),
            Action::Unavailable,
        ),
    ]);
    rows
}
pub fn duration(seconds: i64) -> String {
    let s = seconds.max(0);
    let (d, h, m, s) = (s / 86400, s / 3600 % 24, s / 60 % 60, s % 60);
    if d > 0 {
        format!("{d}:{h:02}:{m:02}:{s:02}")
    } else if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
pub fn rows(sessions: &[Value], known: bool) -> Vec<Row> {
    let mut rows = vec![Row::yes("[new session]")];
    if !known {
        rows.push(Row::gate("Loading…", false));
    }
    rows.extend(
        sessions
            .iter()
            .map(|s| Row::yes(s["name"].as_str().unwrap_or("Unnamed"))),
    );
    rows.push(Row::separator());
    rows.push(Row::yes("Operations"));
    rows
}
pub fn paint(
    canvas: &mut Canvas,
    options: &mut Options,
    sessions: &[Value],
    known: bool,
    bottom: usize,
    now: i64,
) -> (usize, usize) {
    let mut rows = rows(sessions, known);
    let account_width = sessions
        .iter()
        .map(|s| short_name(s["account_id"].as_str().unwrap_or("?")).len() + 2)
        .max()
        .unwrap_or(0);
    let max_name = canvas.width.saturating_sub(account_width + 9);
    let show_accounts = !sessions.is_empty() && max_name >= 4;
    let name_width = sessions
        .iter()
        .map(|s| UnicodeWidthStr::width(s["name"].as_str().unwrap_or("Unnamed")))
        .max()
        .unwrap_or(0)
        .min(if show_accounts {
            max_name
        } else {
            canvas.width.saturating_sub(8)
        });
    let prefix = if known { 1 } else { 2 };
    for (i, s) in sessions.iter().enumerate() {
        rows[prefix + i].label = clip(s["name"].as_str().unwrap_or("Unnamed"), name_width);
    }
    let table_width = name_width + if show_accounts { account_width + 1 } else { 0 };
    // Pad the complete table once. All account indicators share one decision/column.
    for row in &mut rows {
        if !row.separator {
            let width = UnicodeWidthStr::width(row.label.as_str());
            row.label
                .push_str(&" ".repeat(table_width.saturating_sub(width)));
        }
    }
    let (left, capacity) = options.paint(canvas, &rows, 3, bottom);
    for (i, s) in sessions.iter().enumerate() {
        let index = prefix + i;
        if index < options.scroll || index >= options.scroll + capacity {
            continue;
        }
        let y = 3 + index - options.scroll;
        let selected = options.selected == index && !options.back;
        let mut x = left + 2 + name_width;
        if show_accounts {
            let account = format!("[{}]", short_name(s["account_id"].as_str().unwrap_or("?")));
            let pad = account_width.saturating_sub(account.len());
            x += 1;
            canvas.accent_text(
                x + pad,
                y,
                &account,
                Accent {
                    brightness: if selected { 1000 } else { 600 },
                    bold: selected,
                    ..Accent::default()
                },
            );
            x += account_width;
        }
        let suffix = if s["availability"] != "running" {
            " (inactive)".into()
        } else if selected {
            s["active_since"]
                .as_i64()
                .map(|start| format!(" ({})", duration(now - start)))
                .unwrap_or_default()
        } else if s["activity"] == "unknown" {
            " (unknown)".into()
        } else {
            String::new()
        };
        if x + UnicodeWidthStr::width(suffix.as_str()) < canvas.width {
            canvas.text(
                x,
                y,
                &suffix,
                Cell {
                    fg: [108; 3],
                    bold: selected,
                    ..Cell::default()
                },
                false,
            );
        }
    }
    (left, capacity)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn duration_matches_reference() {
        assert_eq!(duration(-1), "0:00");
        assert_eq!(duration(61), "1:01");
        assert_eq!(duration(3661), "1:01:01");
        assert_eq!(duration(90061), "1:01:01:01");
    }
    #[test]
    fn account_columns_align_and_never_disappear_per_item() {
        let sessions = vec![
            json!({"name":"Short","account_id":"codex:a","availability":"running","active_since":1}),
            json!({"name":"A very very long session label","account_id":"codex:personal","availability":"saved"}),
        ];
        for width in [30, 60, 100] {
            let mut c = Canvas::new(width, 18);
            paint(
                &mut c,
                &mut Options {
                    selected: 1,
                    ..Options::default()
                },
                &sessions,
                true,
                15,
                62,
            );
            let lines: Vec<_> = (4..6)
                .map(|r| {
                    c.cells[r * width..(r + 1) * width]
                        .iter()
                        .map(|c| c.glyph.as_str())
                        .collect::<String>()
                })
                .collect();
            assert_eq!(lines[0].find(']'), lines[1].find(']'));
            assert!(lines[0].contains("[a]"));
            assert!(lines[1].contains("[personal]"));
        }
    }
}
