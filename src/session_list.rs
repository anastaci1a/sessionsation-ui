//! Reference session columns, groups and selected-only elapsed-time suffix.
use crate::{
    render::{Accent, Canvas},
    usage::{display_name, short_name},
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
/// cli-main-menu/select.bash: ten Braille frames, one frame every 100 ms.
pub fn spinner(now_ms: u64) -> &'static str {
    ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"][(now_ms / 100 % 10) as usize]
}
fn busy(session: &Value) -> bool {
    session["availability"] == "running" && session["activity"] == "busy"
}
pub fn rows(sessions: &[Value], known: bool) -> Vec<Row> {
    let mut rows = vec![Row::yes("[new session]")];
    let _ = known;
    rows.extend(
        sessions
            .iter()
            .map(|s| Row::yes(display_name(s["name"].as_str().unwrap_or("Unnamed")))),
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
    controls_start: usize,
) -> (usize, usize) {
    let mut rows = rows(sessions, known);
    let account_width = sessions
        .iter()
        .map(|s| {
            UnicodeWidthStr::width(short_name(s["account_id"].as_str().unwrap_or("?")).as_str()) + 6
        })
        .max()
        .unwrap_or(0);
    let max_name = canvas.width.saturating_sub(account_width + 9);
    let show_accounts = !sessions.is_empty() && max_name >= 4;
    let name_width = sessions
        .iter()
        .map(|s| {
            UnicodeWidthStr::width(display_name(s["name"].as_str().unwrap_or("Unnamed")).as_str())
        })
        .max()
        .unwrap_or(0)
        .min(if show_accounts {
            max_name
        } else {
            canvas.width.saturating_sub(8)
        });
    let table_width = name_width + if show_accounts { account_width + 1 } else { 0 };
    let prefix = 1;
    for (i, session) in sessions.iter().enumerate() {
        rows[prefix + i].label = clip(
            &display_name(session["name"].as_str().unwrap_or("Unnamed")),
            name_width,
        );
        let suffix = if session["availability"] != "running" {
            "(inactive)".to_owned()
        } else if let Some(start) = session["active_since"].as_i64() {
            format!("({})", duration(now - start))
        } else if session["activity"] == "unknown" {
            "(unknown)".into()
        } else {
            String::new()
        };
        if !suffix.is_empty() {
            rows[prefix + i] = rows[prefix + i]
                .clone()
                .with_selected_preview(suffix)
                .preview_gap(1)
                .preview_color(if session["availability"] == "running" {
                    crate::widgets::SECONDARY
                } else {
                    crate::widgets::MUTED
                });
        }
    }
    for (index, row) in rows.iter_mut().enumerate() {
        if !row.separator {
            let width = UnicodeWidthStr::width(row.label.as_str());
            let table_width = if let Some(session) = index
                .checked_sub(prefix)
                .and_then(|i| sessions.get(i))
                .filter(|_| show_accounts)
            {
                table_width
                    - if busy(session) {
                        0
                    } else if session["availability"] != "running" {
                        1
                    } else {
                        2
                    }
            } else {
                table_width
            };
            row.label
                .push_str(&" ".repeat(table_width.saturating_sub(width)));
        }
    }
    let list_controls = if known {
        controls_start
    } else {
        controls_start.saturating_sub(2)
    };
    let (left, capacity) = options.paint_menu(
        canvas,
        &rows,
        3,
        bottom.min(list_controls.saturating_sub(1)),
        list_controls,
    );
    if !known {
        let y = if rows.len() > capacity {
            3 + capacity + 2
        } else {
            3 + rows.len() + 1
        };
        if y + 1 < controls_start {
            crate::widgets::paint_hint(canvas, canvas.width.saturating_sub(8) / 2, y, "Loading…");
        }
    }
    if show_accounts {
        for (i, session) in sessions.iter().enumerate() {
            let index = prefix + i;
            if index < options.scroll || index >= options.scroll + capacity {
                continue;
            }
            let selected = options.selected == index && !options.back;
            let account = format!(
                "[{}]",
                short_name(session["account_id"].as_str().unwrap_or("?"))
            );
            let pad = account_width.saturating_sub(UnicodeWidthStr::width(account.as_str()) + 4);
            let x = left + 2 + name_width + 1 + pad;
            let y = 3 + index - options.scroll;
            if selected {
                canvas.accent_text(x, y, "▸", Accent::default());
            }
            canvas.accent_text(
                x + 2,
                y,
                &account,
                Accent {
                    brightness: 1000,
                    bold: selected,
                    ..Accent::default()
                },
            );
            if session["availability"] != "running" {
                canvas.accent_text(
                    x + 2 + UnicodeWidthStr::width(account.as_str()),
                    y,
                    "*",
                    Accent::default(),
                );
            } else if busy(session) {
                let end = x + 2 + UnicodeWidthStr::width(account.as_str());
                let now_ms = canvas.theme.as_ref().map_or(0, |t| t.now);
                let mut style = canvas.cells[y * canvas.width + end - 1].clone();
                style.bold = true;
                if options.back {
                    style.fg = style.fg.map(|n| (u16::from(n) * 60 / 100) as u8);
                }
                canvas.text(end + 1, y, spinner(now_ms), style, false);
            }
        }
    }
    (left, capacity)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn reference_spinner_is_persistent_but_disappears_on_idle_exit_or_unknown() {
        for (i, glyph) in ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
            .iter()
            .enumerate()
        {
            assert_eq!(spinner(i as u64 * 100), *glyph);
            assert_eq!(spinner(i as u64 * 100 + 99), *glyph);
        }
        assert_eq!(spinner(1000), "⠋");
        let mut sessions = vec![
            json!({"name":"Busy", "account_id":"codex:work", "availability":"running", "activity":"busy", "active_since":1}),
        ];
        let mut field = crate::render::Field::new(100, 20, 1967);
        for selected in [0, 1] {
            let mut c = field.paint(4100);
            paint(
                &mut c,
                &mut Options {
                    selected,
                    ..Options::default()
                },
                &sessions,
                true,
                18,
                62,
                19,
            );
            let row = &c.cells[400..500];
            let bracket = row.iter().position(|c| c.glyph == "]").unwrap();
            assert_eq!(row[bracket + 2].glyph, "⠙");
            assert_eq!(row[bracket + 2].fg, row[bracket].fg);
            if selected == 1 {
                assert_eq!(row[bracket + 4].glyph, "(");
                assert!(!row[bracket + 4].bold);
            }
        }
        for (availability, activity) in [
            ("running", "idle"),
            ("saved", "busy"),
            ("running", "unknown"),
        ] {
            sessions[0]["availability"] = json!(availability);
            sessions[0]["activity"] = json!(activity);
            let mut c = field.paint(4200);
            paint(&mut c, &mut Options::default(), &sessions, true, 18, 62, 19);
            assert!(!c.cells.iter().any(|c| {
                ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"].contains(&c.glyph.as_str())
            }));
        }
    }
    #[test]
    fn timers_and_inactive_markers_share_hover_fallback_without_bold() {
        let sessions = vec![
            json!({"name":"abcdefghijklmnopqrstuvwx", "account_id":"codex:personal", "availability":"running", "active_since":1}),
            json!({"name":"Saved", "account_id":"codex:work", "availability":"saved"}),
        ];
        let mut options = Options {
            selected: 1,
            ..Options::default()
        };
        let mut c = Canvas::new(38, 20);
        paint(&mut c, &mut options, &sessions, true, 16, 90062, 19);
        let text = |c: &Canvas, y: usize| {
            c.cells[y * 38..(y + 1) * 38]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
        };
        let y = (0..20)
            .find(|y| text(&c, *y).contains("(1:01:01:01)"))
            .unwrap();
        assert!(y > 7);
        assert!(
            c.cells[y * 38..(y + 1) * 38]
                .iter()
                .all(|c| !c.bold && c.fg == crate::widgets::SECONDARY)
        );
        assert!(text(&c, 5).contains("[work]*"));
        options.selected = 2;
        let mut c = Canvas::new(38, 20);
        paint(&mut c, &mut options, &sessions, true, 16, 90062, 19);
        let y = (0..20)
            .find(|y| text(&c, *y).contains("(inactive)"))
            .unwrap();
        let i = c.cells[y * 38..(y + 1) * 38]
            .iter()
            .position(|c| c.glyph == "(")
            .unwrap();
        assert_eq!(c.cells[y * 38 + i].fg, crate::widgets::MUTED);
        assert!(!c.cells[y * 38 + i].bold);
        options.back = true;
        let mut c = Canvas::new(38, 20);
        paint(&mut c, &mut options, &sessions, true, 16, 90062, 19);
        assert!(!(0..20).any(|y| text(&c, y).contains("(inactive)")));
    }
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
                17,
            );
            let lines: Vec<_> = (4..6)
                .map(|r| {
                    c.cells[r * width..(r + 1) * width]
                        .iter()
                        .map(|c| c.glyph.as_str())
                        .collect::<String>()
                })
                .collect();
            assert_eq!(
                UnicodeWidthStr::width(&lines[0][..lines[0].find(']').unwrap()]),
                UnicodeWidthStr::width(&lines[1][..lines[1].find(']').unwrap()])
            );
            assert!(lines[0].contains("[a]"));
            assert!(lines[1].contains("[personal]"));
            let selected_account = c.cells[4 * width..5 * width]
                .iter()
                .find(|c| c.glyph == "[")
                .unwrap();
            let other_account = c.cells[5 * width..6 * width]
                .iter()
                .find(|c| c.glyph == "[")
                .unwrap();
            assert!(selected_account.bold);
            assert!(!other_account.bold);
        }
    }
}
