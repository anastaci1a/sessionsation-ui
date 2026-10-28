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
    AddToHerdr,
    Details,
    Rename,
    Resume,
    Switch,
    MoveSpace,
    MoveProject,
    ChangeDirectory,
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
/// A running session outside a HERDR pane whose backend advertises the
/// `add_to_herdr` capability gets "Add to HERDR" in place of "Focus Session".
/// Older backends (no such capability key) keep today's rows.
fn add_to_herdr_row(s: &Value) -> Option<Row> {
    if s["availability"] != "running"
        || s["attachment"]["host"] == "herdr"
        || !s["capabilities"]["add_to_herdr"].is_boolean()
    {
        return None;
    }
    let row = Row::gate("Add to HERDR", s["capabilities"]["add_to_herdr"] == true);
    if row.enabled {
        return Some(row);
    }
    Some(
        row.with_selected_preview(format!("({})", add_reason(s)))
            .preview_gap(1)
            .preview_color(crate::widgets::SECONDARY),
    )
}
/// Reasons arrive as "CODE: text"; show the text and never the bare code.
fn add_reason(s: &Value) -> String {
    let reason = s["capability_reasons"]["add_to_herdr"]
        .as_str()
        .unwrap_or("");
    match reason.split_once(": ") {
        Some((_, text)) if !text.trim().is_empty() => text.trim().to_owned(),
        _ if reason == "NO_NATIVE_ATTACH" => "no native attach".into(),
        _ if reason.is_empty() || reason.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') => {
            "unavailable".into()
        }
        _ => reason.to_owned(),
    }
}
/// Account tag for a session: the UI-attached display name, else the short name.
fn account_tag(s: &Value) -> String {
    let provider = s["provider"]
        .as_str()
        .or_else(|| {
            s["account_id"]
                .as_str()
                .and_then(|id| id.split_once(':').map(|(p, _)| p))
        })
        .unwrap_or("codex");
    let name = s["account_label"]
        .as_str()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| short_name(s["account_id"].as_str().unwrap_or("?")));
    format!("{} {name}", crate::usage::provider_symbol(provider))
}
pub fn is_gemini(s: &Value) -> bool {
    s["provider"] == "gemini"
        || s["account_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("gemini:"))
}
/// Claude Code and Gemini CLI sessions share one capability-gated action set.
pub fn is_native_provider(s: &Value) -> bool {
    is_claude(s) || is_gemini(s)
}
pub fn is_claude(s: &Value) -> bool {
    s["provider"] == "claude"
        || s["account_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("claude:"))
}
/// Claude Code sessions expose only what the backend capabilities allow:
/// focus (verified HERDR attachment), details, rename (Sessionsation label) and,
/// when stopped, start. Every other action is omitted, never merely greyed out.
fn claude_actions(s: &Value, resume: bool) -> Vec<(Row, Action)> {
    let c = &s["capabilities"];
    let first = if let Some(row) = add_to_herdr_row(s) {
        (row, Action::AddToHerdr)
    } else if s["availability"] == "running" {
        let reason = if s["attachment"]["host"] == "herdr" {
            None
        } else if s["attachment"].is_object() {
            Some("(host unavailable)")
        } else {
            Some("(not in HERDR)")
        };
        let row = Row::gate(
            "Focus Session",
            c["focus"] == true && s["attachment"]["host"] == "herdr",
        );
        let row = match reason.filter(|_| !row.enabled) {
            Some(reason) => row
                .with_selected_preview(reason)
                .preview_gap(1)
                .preview_color(crate::widgets::SECONDARY),
            None => row,
        };
        (row, Action::Focus)
    } else {
        let startable = resume && c["resume"] == true && s["availability"] == "stopped";
        let row = Row::gate("Start Session", startable);
        let row = if startable {
            row
        } else {
            let reason = match s["availability"].as_str() {
                Some("stale") => "stale",
                Some("unknown") => "state unknown",
                _ if !resume => "unsupported",
                _ => "unavailable",
            };
            row.with_selected_preview(format!("({reason})"))
                .preview_gap(1)
                .preview_color(crate::widgets::SECONDARY)
        };
        (row, Action::Resume)
    };
    vec![
        first,
        (Row::yes("Session Details"), Action::Details),
        (
            Row::gate("Rename Session", c["rename"] == true),
            Action::Rename,
        ),
    ]
}
pub fn actions(s: &Value, resume: bool) -> Vec<(Row, Action)> {
    if is_native_provider(s) {
        return claude_actions(s, resume);
    }
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
        } else if let Some(row) = add_to_herdr_row(s) {
            (row, Action::AddToHerdr)
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
        (
            Row::gate("Move project folder", c["move_project"] == true),
            Action::MoveProject,
        ),
        (
            Row::gate("Change working directory", c["change_directory"] == true),
            Action::ChangeDirectory,
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
        Some("stale") => "stale",
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
/// Gemini CLI's ink-spinner `dots`: one Braille frame every 80 ms, never bold.
pub fn gemini_spinner(now_ms: u64) -> &'static str {
    ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"][(now_ms / 80 % 10) as usize]
}
/// Claude Code 2.1.312 busy spinner: six single-cell frames on a cosine ease over a
/// 2000 ms cycle (dwelling on the first and last), elapsed time quantized down to
/// 100 ms, always at normal weight.
pub fn claude_spinner(now_ms: u64) -> &'static str {
    const FRAMES: [&str; 6] = ["·", "✢", "*", "✶", "✻", "✽"];
    let t = (now_ms / 100 * 100 % 2000) as f64;
    let phase = (1. - (std::f64::consts::TAU * t / 2000.).cos()) / 2. * 5.;
    FRAMES[(phase.round() as usize).min(5)]
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
        .map(|s| UnicodeWidthStr::width(account_tag(s).as_str()) + 6)
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
        let suffix = if session["availability"] == "stale" {
            "(stale)".to_owned()
        } else if session["availability"] == "unknown" {
            "(unknown)".to_owned()
        } else if session["availability"] != "running" && session["attachment"].is_object() {
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
            let account = format!("[{}]", account_tag(session));
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
                style.bold = false;
                if options.back {
                    style.fg = style.fg.map(|n| (u16::from(n) * 60 / 100) as u8);
                }
                let glyph = if is_gemini(session) {
                    gemini_spinner(now_ms)
                } else if is_claude(session) {
                    claude_spinner(now_ms)
                } else {
                    spinner(now_ms)
                };
                canvas.text(end + 1, y, glyph, style, false);
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
    fn directory_actions_follow_backend_capabilities() {
        for enabled in [false, true] {
            let rows = actions(
                &json!({"availability":"running","capabilities":{"move_project":enabled,"change_directory":enabled}}),
                true,
            );
            for action in [Action::MoveProject, Action::ChangeDirectory] {
                assert_eq!(
                    rows.iter().find(|(_, a)| *a == action).unwrap().0.enabled,
                    enabled
                );
            }
        }
    }

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
    fn gemini_spinner_is_ink_dots_at_80ms_and_distinct_from_codex() {
        let frames: Vec<_> = (0..10).map(|i| gemini_spinner(i * 80)).collect();
        assert_eq!(frames, ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);
        assert_eq!(gemini_spinner(79), "⠋");
        assert_eq!(gemini_spinner(800), "⠋");
        assert_eq!(spinner(80), "⠋");
        assert_eq!(spinner(100), "⠙");
        assert_eq!(gemini_spinner(100), "⠙");
        assert!(
            is_gemini(&json!({"account_id":"gemini:x"}))
                && !is_claude(&json!({"provider":"gemini"}))
        );
    }
    #[test]
    fn claude_spinner_follows_cosine_ease_with_100ms_quantization() {
        let frames = ["·", "✢", "*", "✶", "✻", "✽"];
        let want = [0, 0, 0, 1, 2, 2, 3, 4, 5, 5, 5, 5, 5, 4, 3, 3, 2, 1, 0, 0];
        let got: Vec<_> = (0..20).map(|i| claude_spinner(i * 100)).collect();
        assert_eq!(got, want.map(|i| frames[i]));
        assert_eq!(claude_spinner(99), "·");
        assert_eq!(claude_spinner(899), claude_spinner(800));
        assert_eq!(claude_spinner(2000 + 800), claude_spinner(800));
        assert_eq!(claude_spinner(1_000_000_000_050), "·");
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
            assert!(lines[0].contains("[⬢ a]"));
            assert!(!lines[1].contains("[⬢ personal]"));
            let marker = c.cells[5 * width..6 * width]
                .iter()
                .find(|c| c.glyph == "*")
                .unwrap();
            assert_eq!(marker.fg, crate::widgets::SECONDARY);
            assert!(!marker.bold);
        }
    }
    fn claude_row_labels(s: &Value, resume: bool) -> Vec<(String, bool)> {
        actions(s, resume)
            .into_iter()
            .map(|(r, _)| (r.label, r.enabled))
            .collect()
    }
    fn claude(availability: &str, attached: bool) -> Value {
        json!({"provider":"claude","account_id":"claude:main","availability":availability,
            "attachment":if attached{json!({"host":"herdr","target":"herdr/default/w:p","pid":9})}else{Value::Null},
            "capabilities":{"focus":attached,"rename":true,"resume":availability=="stopped",
                "switch_account":false,"restart":false,"stop":false,"archive":false,"delete":false,
                "move_space":false,"move_project":false,"change_directory":false,"steer":false}})
    }
    #[test]
    fn claude_sessions_offer_only_focus_details_rename_and_start() {
        assert_eq!(
            claude_row_labels(&claude("running", true), true),
            [
                ("Focus Session".into(), true),
                ("Session Details".into(), true),
                ("Rename Session".into(), true)
            ]
        );
        assert_eq!(
            claude_row_labels(&claude("stopped", false), true),
            [
                ("Start Session".into(), true),
                ("Session Details".into(), true),
                ("Rename Session".into(), true)
            ]
        );
        // No hidden Codex-only rows leak in, even if a stale payload claims them.
        let mut s = claude("running", true);
        s["capabilities"]["switch_account"] = json!(true);
        s["capabilities"]["stop"] = json!(true);
        s["capabilities"]["restart"] = json!(true);
        assert_eq!(actions(&s, true).len(), 3);
    }
    #[test]
    fn claude_focus_and_start_are_disabled_with_reasons_for_detached_stale_unknown() {
        let detached = actions(&claude("running", false), true);
        assert!(!detached[0].0.enabled);
        assert_eq!(detached[0].0.preview.as_deref(), Some("(not in HERDR)"));
        for (state, reason) in [("stale", "(stale)"), ("unknown", "(state unknown)")] {
            let rows = actions(&claude(state, false), true);
            assert_eq!(rows[0].0.label, "Start Session");
            assert!(!rows[0].0.enabled, "{state} is never startable");
            assert_eq!(rows[0].0.preview.as_deref(), Some(reason));
            assert_eq!(rows[0].1, Action::Resume);
        }
        // Without the resume capability even a stopped session cannot start.
        assert!(!actions(&claude("stopped", false), false)[0].0.enabled);
        let mut s = claude("stopped", false);
        s["capabilities"]["resume"] = json!(false);
        assert!(!actions(&s, true)[0].0.enabled);
    }
    #[test]
    fn codex_sessions_keep_their_full_menu_and_restart_label() {
        let s = json!({"provider":"codex","availability":"running","capabilities":{"restart":true,"stop":true,"switch_account":true}});
        let labels: Vec<_> = actions(&s, true)
            .into_iter()
            .map(|(r, _)| r.label)
            .collect();
        assert!(labels.contains(&"Restart Codex".to_owned()));
        assert!(labels.contains(&"Switch account".to_owned()));
        assert!(labels.contains(&"End session".to_owned()));
        assert!(!is_claude(&s));
        assert!(is_claude(&json!({"account_id":"claude:x"})));
    }
}
