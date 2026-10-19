//! Reference session columns, groups and selected-only elapsed-time suffix.
use crate::{
    render::{Accent, Canvas},
    usage::{display_name, short_name},
    widgets::{Options, Row, ellipsize},
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
    Archive,
    Delete,
    Restore,
    Unavailable,
}
pub fn actions(s: &Value, resume: bool) -> Vec<(Row, Action)> {
    let c = &s["capabilities"];
    if s["availability"] == "archived" {
        return vec![
            (
                Row::gate("Restore Session", c["restore"] == true),
                Action::Restore,
            ),
            (Row::yes("Session Details"), Action::Details),
            (Row::separator(), Action::Unavailable),
            (
                Row::gate("Delete Session", c["delete"] == true),
                Action::Delete,
            ),
        ];
    }
    let inactive = s["availability"] != "running";
    let focus = Row::gate(
        "Focus Session",
        s["availability"] == "running" && s["attachment"]["host"] == "herdr",
    );
    let focus = if s["availability"] == "running" && s["attachment"]["host"] == "tmux" {
        focus
            .with_selected_preview("(tmux)")
            .preview_gap(1)
            .preview_color(crate::widgets::SECONDARY)
    } else {
        focus
    };
    let start = Row::gate("Start Session", resume && c["resume"] == true);
    let start = if start.enabled {
        start
    } else {
        start
            .with_selected_preview(format!("({})", resume_reason(s, resume)))
            .preview_gap(1)
            .preview_color(crate::widgets::SECONDARY)
    };
    let mut rows = vec![
        if inactive {
            (start, Action::Resume)
        } else {
            (focus, Action::Focus)
        },
        (Row::yes("Session Details"), Action::Details),
        (
            Row::gate("Rename Session", c["rename"] == true),
            Action::Rename,
        ),
    ];
    if inactive {
        rows.extend([
            (Row::separator(), Action::Unavailable),
            (
                Row::gate("Archive Session", c["archive"] == true),
                Action::Archive,
            ),
            (
                Row::gate("Delete Session", c["delete"] == true),
                Action::Delete,
            ),
        ]);
        return rows;
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
fn resume_reason(s: &Value, supported: bool) -> &'static str {
    if !supported {
        return "unsupported";
    }
    match s["resume_block_reason"].as_str() {
        Some("unknown_account") => "unknown account",
        Some("history_unavailable") => "history unavailable",
        Some("already_running") => "already running",
        Some("moving") => "moving",
        Some("missing_directory") => "missing directory",
        Some("history_busy") => "history busy",
        Some("unfinished_history") => "unfinished history",
        Some("conflicting_history") => "conflicting history",
        _ => "unverified history",
    }
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
    rows_with_archives(sessions, known, false)
}
pub fn rows_with_archives(sessions: &[Value], known: bool, archives: bool) -> Vec<Row> {
    let mut rows = vec![];
    let _ = known;
    rows.extend(
        sessions
            .iter()
            .map(|s| Row::yes(display_name(s["name"].as_str().unwrap_or("Unnamed")))),
    );
    if !sessions.is_empty() {
        rows.push(Row::separator());
    }
    if archives {
        rows.push(Row::yes("Archived Sessions"));
    }
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
    paint_with_archives(
        canvas,
        options,
        sessions,
        known,
        bottom,
        now,
        controls_start,
        false,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn paint_with_archives(
    canvas: &mut Canvas,
    options: &mut Options,
    sessions: &[Value],
    known: bool,
    bottom: usize,
    now: i64,
    controls_start: usize,
    archives: bool,
) -> (usize, usize) {
    let mut rows = rows_with_archives(sessions, known, archives);
    let account_width = sessions
        .iter()
        .map(|s| {
            UnicodeWidthStr::width(short_name(s["account_id"].as_str().unwrap_or("?")).as_str()) + 6
        })
        .max()
        .unwrap_or(0);
    let max_name = canvas.width.saturating_sub(account_width + 8);
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
    let table_width = name_width + if show_accounts { account_width } else { 0 };
    let prefix = 0;
    for (i, session) in sessions.iter().enumerate() {
        rows[prefix + i].label = ellipsize(
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
                table_width - if busy(session) { 0 } else { 2 }
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
            let account = format!(
                "[{}]",
                short_name(session["account_id"].as_str().unwrap_or("?"))
            );
            let pad = account_width.saturating_sub(UnicodeWidthStr::width(account.as_str()) + 4);
            let x = left + 2 + name_width + pad;
            let y = 3 + index - options.scroll;
            canvas.accent_text(
                x + 2,
                y,
                &account,
                Accent {
                    brightness: if options.selected == index
                        && !options.back
                        && session["availability"] == "running"
                    {
                        1000
                    } else {
                        600
                    },
                    bold: options.selected == index && !options.back,
                    flash: false,
                    ..Accent::default()
                },
            );
            let end = x + 2 + UnicodeWidthStr::width(account.as_str());
            if session["availability"] != "running"
                && !(options.selected == index
                    && options.preview_visible
                    && !Options::previews_detached(canvas.width, &rows))
            {
                canvas.text(
                    end,
                    y,
                    " *",
                    crate::render::Cell {
                        fg: crate::widgets::MUTED,
                        bold: false,
                        ..crate::render::Cell::default()
                    },
                    false,
                );
            }
            if busy(session) {
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
    fn empty_session_menu_has_only_operations_and_archives_are_conditional() {
        assert_eq!(
            rows(&[], true)
                .iter()
                .map(|r| r.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Operations"]
        );
        assert_eq!(
            rows_with_archives(&[], true, true)
                .iter()
                .map(|r| r.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Archived Sessions", "Operations"]
        );
        let rows = rows_with_archives(&[json!({"name":"Saved"})], true, true);
        assert_eq!(
            rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            vec!["Saved", "", "Archived Sessions", "Operations"]
        );
        assert!(rows[1].separator);
    }
    #[test]
    fn inactive_actions_start_with_history_and_hide_live_controls() {
        let rows = actions(
            &serde_json::json!({"availability":"saved","capabilities":{"resume":true,"rename":true}}),
            true,
        );
        assert_eq!(
            rows.iter()
                .map(|(r, _)| r.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Start Session",
                "Session Details",
                "Rename Session",
                "",
                "Archive Session",
                "Delete Session"
            ]
        );
        assert_eq!(rows[0].1, Action::Resume);
        assert!(rows[0].0.enabled);
        let rows = actions(
            &serde_json::json!({"availability":"running","attachment":{"host":"tmux"}}),
            true,
        );
        assert_eq!(rows[0].0.preview.as_deref(), Some("(tmux)"));
        assert_eq!(rows[0].0.preview_fg, crate::widgets::SECONDARY);
    }
    #[test]
    fn action_reasons_are_short_selected_only_and_hide_on_back() {
        for (session, expected) in [
            (
                json!({"availability":"running","attachment":{"host":"tmux"}}),
                "(tmux)",
            ),
            (
                json!({"availability":"unavailable","resume_block_reason":"missing_directory"}),
                "(missing directory)",
            ),
            (
                json!({"availability":"unavailable","resume_block_reason":"future"}),
                "(unverified history)",
            ),
        ] {
            let rows: Vec<_> = actions(&session, true)
                .into_iter()
                .map(|(row, _)| row)
                .collect();
            for width in [30, 80] {
                for (selected, back) in [(0, false), (1, false), (0, true)] {
                    let mut canvas = Canvas::new(width, 30);
                    let mut options = Options {
                        selected,
                        back,
                        ..Options::default()
                    };
                    options.paint_menu(&mut canvas, &rows, 3, 26, 29);
                    let text = canvas
                        .cells
                        .iter()
                        .map(|c| c.glyph.as_str())
                        .collect::<String>();
                    assert_eq!(text.contains(expected), selected == 0 && !back);
                    for cell in canvas.cells.iter().filter(|c| c.glyph == "(") {
                        assert_eq!(cell.fg, crate::widgets::SECONDARY);
                        assert!(!cell.bold);
                    }
                }
            }
        }
    }
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
        for selected in [2, 0] {
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
            let row = &c.cells[300..400];
            let bracket = row.iter().position(|c| c.glyph == "]").unwrap();
            assert_eq!(row[bracket + 2].glyph, "⠙");
            assert_eq!(row[bracket + 2].fg, row[bracket].fg);
            if selected == 0 {
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
    fn session_titles_keep_ellipsis_at_wide_and_narrow_widths() {
        let sessions = vec![
            json!({"name":"abcdefghijklmnopqrstuvwx", "account_id":"codex:personal", "availability":"saved"}),
        ];
        for width in [26, 38, 80] {
            let mut c = Canvas::new(width, 20);
            paint(
                &mut c,
                &mut Options::default(),
                &sessions,
                true,
                16,
                100,
                19,
            );
            let row = c.cells[3 * width..4 * width]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>();
            assert!(row.contains('…'), "{width}: {row}");
            assert!(!row.contains("abcdefghijklmnop"));
        }
    }
    #[test]
    fn timers_and_inactive_markers_share_hover_fallback_without_bold() {
        let sessions = vec![
            json!({"name":"abcdefghijklmnopqrstuvwx", "account_id":"codex:personal", "availability":"running", "active_since":1}),
            json!({"name":"Saved", "account_id":"codex:work", "availability":"saved"}),
        ];
        let mut options = Options {
            selected: 0,
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
        assert!(y > 6);
        assert!(
            c.cells[y * 38..(y + 1) * 38]
                .iter()
                .all(|c| !c.bold && c.fg == crate::widgets::SECONDARY)
        );
        assert!(text(&c, 4).contains("[work]"));
        assert!(!text(&c, 4).contains("[work]*"));
        options.selected = 1;
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
    fn account_tag_bolds_on_hover_including_inactive_without_changing_colors() {
        for availability in ["running", "saved"] {
            let sessions = vec![
                json!({"name":"Session", "account_id":"codex:personal", "availability":availability}),
            ];
            let mut field = crate::render::Field::new(80, 20, 1967);
            let base = field.paint(4200);
            for (selected, back, active_brightness) in
                [(2, false, 600), (0, false, 1000), (0, true, 600)]
            {
                let brightness = if availability == "running" {
                    active_brightness
                } else {
                    600
                };
                let mut c = base.clone();
                paint(
                    &mut c,
                    &mut Options {
                        selected,
                        back,
                        ..Options::default()
                    },
                    &sessions,
                    true,
                    16,
                    100,
                    19,
                );
                let x = c.cells[240..320]
                    .iter()
                    .position(|c| c.glyph == "[")
                    .unwrap();
                let mut expected = base.clone();
                expected.accent_text(
                    x,
                    3,
                    "[personal]",
                    Accent {
                        brightness,
                        bold: selected == 0 && !back,
                        flash: false,
                        ..Accent::default()
                    },
                );
                assert_eq!(c.cells[240 + x].fg, expected.cells[240 + x].fg);
                assert_eq!(c.cells[240 + x].bold, selected == 0 && !back);
                if availability == "saved" && (selected != 0 || back) {
                    assert_eq!(c.cells[240 + x + 10].glyph, " ");
                    let marker = &c.cells[240 + x + 11];
                    assert_eq!(marker.glyph, "*");
                    assert_eq!(marker.fg, crate::widgets::MUTED);
                    assert!(!marker.bold);
                }
            }
        }
    }
    #[test]
    fn inactive_marker_exchanges_with_visible_parenthetical() {
        let sessions =
            vec![json!({"name":"Saved","account_id":"codex:personal","availability":"saved"})];
        for width in [30, 80] {
            for (selected, back) in [(2, false), (0, false), (0, true)] {
                let mut c = Canvas::new(width, 20);
                let mut options = Options {
                    selected,
                    back,
                    ..Options::default()
                };
                paint(&mut c, &mut options, &sessions, true, 16, 100, 19);
                let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
                let parenthetical = selected == 0 && !back;
                assert_eq!(
                    text.contains("(inactive)"),
                    parenthetical,
                    "{width}: {text}"
                );
                let row = c.cells[3 * width..4 * width]
                    .iter()
                    .map(|c| c.glyph.as_str())
                    .collect::<String>();
                assert_eq!(text.contains('*'), !row.contains("(inactive)"));
                for cell in c.cells.iter().filter(|c| c.glyph == "*") {
                    assert_eq!(cell.fg, crate::widgets::MUTED);
                    assert!(!cell.bold);
                }
            }
        }
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
                    selected: 0,
                    ..Options::default()
                },
                &sessions,
                true,
                15,
                62,
                17,
            );
            let lines: Vec<_> = (3..5)
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
            let selected_account = c.cells[3 * width..4 * width]
                .iter()
                .find(|c| c.glyph == "[")
                .unwrap();
            let other_account = c.cells[4 * width..5 * width]
                .iter()
                .find(|c| c.glyph == "[")
                .unwrap();
            assert!(selected_account.bold);
            assert_eq!(selected_account.fg, other_account.fg);
            assert!(!other_account.bold);
        }
    }
}
