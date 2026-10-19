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
    MoveSpace,
    Restart,
    Stop,
    Archive,
    Delete,
    Restore,
    Unavailable,
}
fn session_gate(s: &Value, label: &str, capability: &str) -> Row {
    let row = Row::gate(label, s["capabilities"][capability] == true);
    if row.enabled {
        return row;
    }
    let reason = if s["pending_operation_id"].is_string() {
        "operation active"
    } else if capability == "archive" && s["availability"] == "archived" {
        "already archived"
    } else if s["availability"] == "running" {
        "end session first"
    } else if s["attachment"].is_object() {
        "host unavailable"
    } else {
        "unavailable"
    };
    row.with_selected_preview(format!("({reason})"))
        .preview_gap(1)
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
            (session_gate(s, "Delete Session", "delete"), Action::Delete),
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
                session_gate(s, "Archive Session", "archive"),
                Action::Archive,
            ),
            (session_gate(s, "Delete Session", "delete"), Action::Delete),
        ]);
        return rows;
    }
    rows.extend([
        (Row::separator(), Action::Unavailable),
        (
            Row::gate("Switch account", c["switch_account"] == true),
            Action::Switch,
        ),
        (
            Row::gate("Move to Space", c["move_space"] == true),
            Action::MoveSpace,
        ),
        (Row::separator(), Action::Unavailable),
        (Row::gate("Move project folder", false), Action::Unavailable),
        (
            Row::gate("Change working directory", false),
            Action::Unavailable,
        ),
        (Row::separator(), Action::Unavailable),
        (
            Row::gate("Pause work", false)
                .with_selected_preview("(not integrated)")
                .preview_gap(1),
            Action::Unavailable,
        ),
        (
            Row::gate("Restart Codex", c["restart"] == true),
            Action::Restart,
        ),
        (Row::gate("End session", c["stop"] == true), Action::Stop),
        (Row::separator(), Action::Unavailable),
        (
            session_gate(s, "Archive Session", "archive"),
            Action::Archive,
        ),
        (session_gate(s, "Delete Session", "delete"), Action::Delete),
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
    if s["activity"] == "idle" {
        rows.retain(|(row, _)| row.label != "Pause work");
    }
    rows
}
fn resume_reason(s: &Value, supported: bool) -> &'static str {
    if !supported {
        return "unsupported";
    }
    match s["resume_block_reason"].as_str() {
        Some("unknown_account") => "unknown account",
        Some("history_unavailable") => "history unavailable",
        Some("host_unavailable") => "host unavailable",
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
/// Display rows are shared by painting, activation and stable-focus restoration.
/// None is the nonselectable separator between active and inactive sessions.
pub fn session_layout(sessions: &[Value]) -> Vec<Option<usize>> {
    let active: Vec<_> = sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| s["availability"] == "running")
        .map(|(i, _)| Some(i))
        .collect();
    let inactive: Vec<_> = sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| s["availability"] != "running")
        .map(|(i, _)| Some(i))
        .collect();
    let separate = !active.is_empty() && !inactive.is_empty();
    active
        .into_iter()
        .chain(separate.then_some(None))
        .chain(inactive)
        .collect()
}
pub fn session_index(sessions: &[Value], row: usize) -> Option<usize> {
    session_layout(sessions).get(row).copied().flatten()
}
pub fn session_row(sessions: &[Value], index: usize) -> usize {
    session_layout(sessions)
        .iter()
        .position(|i| *i == Some(index))
        .unwrap_or(0)
}
pub fn rows_with_archives(sessions: &[Value], known: bool, archives: bool) -> Vec<Row> {
    let mut rows = vec![];
    let _ = known;
    rows.extend(session_layout(sessions).into_iter().map(|i| match i {
        Some(i) => Row::yes(display_name(
            sessions[i]["name"].as_str().unwrap_or("Unnamed"),
        )),
        None => Row::separator(),
    }));
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
        .filter(|s| s["availability"] == "running")
        .map(|s| {
            UnicodeWidthStr::width(short_name(s["account_id"].as_str().unwrap_or("?")).as_str()) + 6
        })
        .max()
        .unwrap_or(4);
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
    let layout = session_layout(sessions);
    for (i, session) in layout
        .iter()
        .enumerate()
        .filter_map(|(row, i)| i.map(|i| (row, &sessions[i])))
    {
        rows[i].label = ellipsize(
            &display_name(session["name"].as_str().unwrap_or("Unnamed")),
            name_width,
        );
        let suffix = if session["availability"] != "running" && session["attachment"].is_object() {
            "(host unavailable)".into()
        } else if session["availability"] != "running" {
            "(inactive)".to_owned()
        } else if let Some(start) = session["active_since"].as_i64() {
            format!("({})", duration(now - start))
        } else if session["activity"] == "unknown" {
            "(unknown)".into()
        } else {
            String::new()
        };
        if !suffix.is_empty() {
            rows[i] = rows[i]
                .clone()
                .with_selected_preview(suffix)
                .with_preview_group(u8::from(session["availability"] != "running"))
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
            let table_width = if let Some(session) = layout
                .get(index)
                .copied()
                .flatten()
                .and_then(|i| sessions.get(i))
                .filter(|_| show_accounts)
            {
                if session["availability"] != "running" {
                    name_width + 1
                } else {
                    table_width - if busy(session) { 0 } else { 2 }
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
        for (index, session) in layout
            .iter()
            .enumerate()
            .filter_map(|(row, i)| i.map(|i| (row, &sessions[i])))
        {
            if index < options.scroll || index >= options.scroll + capacity {
                continue;
            }
            let y = 3 + index - options.scroll;
            if session["availability"] != "running" {
                if !(session["attachment"].is_object()
                    || options.selected == index
                        && options.preview_visible
                        && !options.selected_preview_detached(canvas.width, &rows))
                {
                    canvas.text(
                        left + 2 + name_width + 2,
                        y,
                        "*",
                        crate::render::Cell {
                            fg: crate::widgets::SECONDARY,
                            ..crate::render::Cell::default()
                        },
                        false,
                    );
                }
                continue;
            }
            let account = format!(
                "[{}]",
                short_name(session["account_id"].as_str().unwrap_or("?"))
            );
            let pad = account_width.saturating_sub(UnicodeWidthStr::width(account.as_str()) + 4);
            let x = left + 2 + name_width + pad;
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
    fn activity_groups_share_one_nonselectable_separator_and_mapping() {
        let sessions = vec![
            json!({"name":"Saved","availability":"saved"}),
            json!({"name":"Idle","availability":"running"}),
            json!({"name":"Busy","availability":"running"}),
        ];
        assert_eq!(
            session_layout(&sessions),
            vec![Some(1), Some(2), None, Some(0)]
        );
        let rows = rows(&sessions, true);
        assert_eq!(rows[0].label, "Idle");
        assert_eq!(rows[1].label, "Busy");
        assert!(rows[2].separator);
        assert_eq!(rows[3].label, "Saved");
        assert!(rows[4].separator);
        assert_eq!(rows[5].label, "Operations");
        assert_eq!(session_index(&sessions, 2), None);
        for index in 0..sessions.len() {
            assert_eq!(
                session_index(&sessions, session_row(&sessions, index)),
                Some(index)
            );
        }
        assert!(!session_layout(&sessions[..1]).contains(&None));
        assert!(!session_layout(&sessions[1..]).contains(&None));
    }
    #[test]
    fn unobserved_live_attachments_explain_the_host_gate() {
        let s = serde_json::json!({"availability":"unavailable","attachment":{"host":"herdr"},"resume_block_reason":"host_unavailable","capabilities":{}});
        for (row, action) in actions(&s, true) {
            if matches!(action, Action::Resume | Action::Archive | Action::Delete) {
                assert!(!row.enabled);
                assert_eq!(row.preview.as_deref(), Some("(host unavailable)"));
            }
        }
    }
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
    fn archive_precedes_delete_unless_already_archived_and_idle_hides_pause() {
        for availability in ["running", "saved", "archived"] {
            let mut session = json!({"availability":availability,"activity":"idle","capabilities":{"archive":availability=="saved","delete":availability!="running"}});
            let rows = actions(&session, true);
            if availability == "archived" {
                assert!(!rows.iter().any(|(_, a)| *a == Action::Archive));
                assert!(rows.iter().any(|(_, a)| *a == Action::Restore));
                assert!(rows.iter().any(|(_, a)| *a == Action::Delete));
                continue;
            }
            let archive = rows
                .iter()
                .position(|(_, a)| *a == Action::Archive)
                .unwrap();
            assert_eq!(
                archive + 1,
                rows.iter().position(|(_, a)| *a == Action::Delete).unwrap()
            );
            assert!(!rows.iter().any(|(r, _)| r.label == "Pause work"));
            assert_eq!(rows[archive].0.enabled, availability == "saved");
            if availability != "saved" {
                assert!(rows[archive].0.preview_selected_only);
            }
            session["activity"] = json!("busy");
            if availability == "running" {
                assert!(
                    actions(&session, true)
                        .iter()
                        .any(|(r, _)| r.label == "Pause work" && !r.enabled)
                );
            }
        }
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
        assert!(!text(&c, 5).contains("[work]"));
        assert!(!text(&c, 5).contains("[work]*"));
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
    fn inactive_preview_fit_is_independent_of_active_status_width() {
        fn text(c: &Canvas, y: usize) -> String {
            c.cells[y * c.width..(y + 1) * c.width]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect()
        }
        let sessions = vec![
            json!({"name":"Active","account_id":"codex:personal","availability":"running","activity":"busy","active_since":1}),
            json!({"name":"Saved","account_id":"codex:removed","availability":"saved"}),
        ];
        let mut found = false;
        for width in 30..70 {
            let mut live = Canvas::new(width, 20);
            paint(
                &mut live,
                &mut Options::default(),
                &sessions,
                true,
                16,
                90062,
                19,
            );
            let active_below = (6..18).any(|y| text(&live, y).contains("(1:01:01:01)"));
            let mut saved = Canvas::new(width, 20);
            paint(
                &mut saved,
                &mut Options {
                    selected: 2,
                    ..Options::default()
                },
                &sessions,
                true,
                16,
                90062,
                19,
            );
            if active_below && text(&saved, 5).contains("(inactive)") {
                found = true;
                assert!(!text(&saved, 5).contains('*'));
                assert!(!text(&saved, 5).contains("[removed]"));
                break;
            }
        }
        assert!(
            found,
            "inactive status should fit inline before the active status does"
        );
    }
    #[test]
    fn duration_matches_reference() {
        assert_eq!(duration(-1), "0:00");
        assert_eq!(duration(61), "1:01");
        assert_eq!(duration(3661), "1:01:01");
        assert_eq!(duration(90061), "1:01:01:01");
    }
    #[test]
    fn active_account_tag_bolds_on_hover_without_changing_colors() {
        for availability in ["running"] {
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
                    assert_eq!(marker.fg, crate::widgets::SECONDARY);
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
                    assert_eq!(cell.fg, crate::widgets::SECONDARY);
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
            let lines: Vec<_> = [3, 5]
                .into_iter()
                .map(|r| {
                    c.cells[r * width..(r + 1) * width]
                        .iter()
                        .map(|c| c.glyph.as_str())
                        .collect::<String>()
                })
                .collect();
            assert_eq!(
                UnicodeWidthStr::width(&lines[0][..lines[0].find('[').unwrap()]),
                UnicodeWidthStr::width(&lines[1][..lines[1].find('*').unwrap()])
            );
            assert!(lines[0].contains("[a]"));
            assert!(!lines[1].contains("[personal]"));
            let marker = c.cells[5 * width..6 * width]
                .iter()
                .find(|c| c.glyph == "*")
                .unwrap();
            assert_eq!(marker.fg, crate::widgets::SECONDARY);
            assert!(!marker.bold);
        }
    }
}
