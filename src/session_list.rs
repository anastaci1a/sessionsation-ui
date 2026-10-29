//! Session rows: name, state marker, dim provider/account column and operation badge.
use crate::{
    render::{Accent, Canvas, Cell},
    usage::short_name,
    widgets::{MUTED, Options, Row, SECONDARY, SELECTED, ellipsize},
};
use serde_json::Value;
use unicode_segmentation::UnicodeSegmentation;
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
/// A session's pending or recently failed operation, shown at the end of its row.
#[derive(Clone, Debug, PartialEq)]
pub struct Badge {
    /// Operation list key: ("operation" | "launch" | "resume-launch", id).
    pub kind: String,
    pub id: String,
    pub text: String,
    pub active: bool,
}
impl Badge {
    /// Animated arc for active work, a cross for failures.
    pub fn icon(&self, now_ms: u64) -> &'static str {
        if self.active {
            ["◜", "◝", "◞", "◟"][(now_ms / 150 % 4) as usize]
        } else {
            "✗"
        }
    }
}
/// Soft failure red used only for the badge cross.
pub const FAILURE: [u8; 3] = [215, 95, 95];
fn provider_label_of(s: &Value) -> String {
    crate::client::provider_label(provider(s)).to_owned()
}
fn provider(s: &Value) -> &str {
    s["provider"]
        .as_str()
        .or_else(|| {
            s["account_id"]
                .as_str()
                .and_then(|id| id.split_once(':').map(|(p, _)| p))
        })
        .unwrap_or("codex")
}
fn pending(s: &Value) -> bool {
    s["pending_operation_id"].is_string() || s["resume_block_reason"] == "moving"
}
fn session_gate(s: &Value, label: &str, capability: &str) -> Row {
    let row = Row::gate(label, s["capabilities"][capability] == true);
    if row.enabled {
        return row;
    }
    let reason = if pending(s) {
        "operation running"
    } else if capability == "archive" && s["availability"] == "archived" {
        "already archived"
    } else if s["availability"] == "running" {
        "end session first"
    } else if s["attachment"].is_object() {
        "host unavailable"
    } else {
        "unavailable"
    };
    row.with_reason(reason)
}
/// A disabled lifecycle row: the shared reason, or a pending operation.
fn live_gate(s: &Value, label: &str, capability: &str) -> Row {
    let row = Row::gate(label, s["capabilities"][capability] == true);
    if row.enabled {
        row
    } else if pending(s) {
        row.with_reason("operation running")
    } else {
        row.with_reason("not available now")
    }
}
/// A running session outside a HERDR pane whose backend advertises the
/// `add_to_herdr` capability gets "Add to HERDR" in place of "Focus session".
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
    Some(row.with_reason(add_reason(s)))
}
/// Reasons arrive as "CODE: text"; show the text and never the bare code.
fn add_reason(s: &Value) -> String {
    let reason = s["capability_reasons"]["add_to_herdr"]
        .as_str()
        .unwrap_or("");
    match reason.split_once(": ") {
        Some((_, text)) if !text.trim().is_empty() => {
            let text = text.trim().trim_end_matches('.');
            let mut chars = text.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_lowercase().collect::<String>() + chars.as_str()
            })
        }
        _ if reason == "NO_NATIVE_ATTACH" => "no native attach".into(),
        _ if reason.is_empty() || reason.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') => {
            "unavailable".into()
        }
        _ => reason.to_owned(),
    }
}
/// Account column for a session: provider symbol and the UI-attached display
/// name, else the short name.
pub fn account_tag(s: &Value) -> String {
    let name = s["account_label"]
        .as_str()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| short_name(s["account_id"].as_str().unwrap_or("?")));
    format!("{} {name}", crate::usage::provider_symbol(provider(s)))
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
/// Claude Code and Gemini CLI sessions enable only what the backend capabilities
/// allow: focus (verified HERDR attachment), details, rename (Sessionsation label)
/// and, when stopped, start. Codex-only lifecycle rows stay visible but greyed with
/// a short reason, so every provider shares one menu shape.
fn claude_actions(s: &Value, resume: bool) -> Vec<(Row, Action)> {
    let c = &s["capabilities"];
    let first = if let Some(row) = add_to_herdr_row(s) {
        (row, Action::AddToHerdr)
    } else if s["availability"] == "running" {
        let reason = if s["attachment"]["host"] == "herdr" {
            None
        } else if s["attachment"].is_object() {
            Some("host unavailable")
        } else {
            Some("not in HERDR")
        };
        let row = Row::gate(
            "Focus session",
            c["focus"] == true && s["attachment"]["host"] == "herdr",
        );
        let row = match reason.filter(|_| !row.enabled) {
            Some(reason) => row.with_reason(reason),
            None => row,
        };
        (row, Action::Focus)
    } else {
        let startable = resume && c["resume"] == true && s["availability"] == "stopped";
        let row = Row::gate("Start session", startable);
        let row = if startable {
            row
        } else {
            let reason = match s["availability"].as_str() {
                _ if pending(s) => "operation running",
                Some("stale") => "stale",
                Some("unknown") => "state unknown",
                _ if !resume => "unsupported",
                _ => "unavailable",
            };
            row.with_reason(reason)
        };
        (row, Action::Resume)
    };
    let unsupported = format!("not supported for {}", provider_label_of(s));
    let no = |label: &str| {
        (
            Row::gate(label, false).with_reason(unsupported.clone()),
            Action::Unavailable,
        )
    };
    vec![
        first,
        (Row::yes("Session details"), Action::Details),
        (
            Row::gate("Rename session", c["rename"] == true),
            Action::Rename,
        ),
        (Row::separator(), Action::Unavailable),
        no("Switch account"),
        no("Move to Space"),
        (Row::separator(), Action::Unavailable),
        no("Restart session"),
        no("End session"),
        (Row::separator(), Action::Unavailable),
        no("Archive session"),
        no("Delete session"),
    ]
}
pub fn actions(s: &Value, resume: bool) -> Vec<(Row, Action)> {
    let mut rows = if is_native_provider(s) {
        claude_actions(s, resume)
    } else {
        codex_actions(s, resume)
    };
    let mut aligned: Vec<Row> = rows.iter().map(|(r, _)| r.clone()).collect();
    crate::widgets::align_reasons(&mut aligned);
    for ((row, _), aligned) in rows.iter_mut().zip(aligned) {
        *row = aligned;
    }
    rows
}
fn codex_actions(s: &Value, resume: bool) -> Vec<(Row, Action)> {
    let c = &s["capabilities"];
    if s["availability"] == "archived" {
        return vec![
            (
                Row::gate("Restore session", c["restore"] == true),
                Action::Restore,
            ),
            (Row::yes("Session details"), Action::Details),
            (Row::separator(), Action::Unavailable),
            (session_gate(s, "Delete session", "delete"), Action::Delete),
        ];
    }
    let inactive = s["availability"] != "running";
    let focus = Row::gate(
        "Focus session",
        s["availability"] == "running" && s["attachment"]["host"] == "herdr",
    );
    let focus = if focus.enabled {
        focus
    } else if s["attachment"]["host"] == "tmux" {
        focus.with_reason("in tmux")
    } else {
        focus.with_reason("not in HERDR")
    };
    let start = Row::gate("Start session", resume && c["resume"] == true);
    let start = if start.enabled {
        start
    } else {
        start.with_reason(resume_reason(s, resume))
    };
    let mut rows = vec![
        if inactive {
            (start, Action::Resume)
        } else if let Some(row) = add_to_herdr_row(s) {
            (row, Action::AddToHerdr)
        } else {
            (focus, Action::Focus)
        },
        (Row::yes("Session details"), Action::Details),
        (
            Row::gate("Rename session", c["rename"] == true),
            Action::Rename,
        ),
    ];
    if inactive {
        rows.extend([
            (Row::separator(), Action::Unavailable),
            (
                session_gate(s, "Archive session", "archive"),
                Action::Archive,
            ),
            (session_gate(s, "Delete session", "delete"), Action::Delete),
        ]);
        return rows;
    }
    rows.extend([
        (Row::separator(), Action::Unavailable),
        (
            live_gate(s, "Switch account", "switch_account"),
            Action::Switch,
        ),
        (
            live_gate(s, "Move to Space", "move_space"),
            Action::MoveSpace,
        ),
        (Row::separator(), Action::Unavailable),
        (
            live_gate(s, "Move project folder", "move_project"),
            Action::MoveProject,
        ),
        (
            live_gate(s, "Change working directory", "change_directory"),
            Action::ChangeDirectory,
        ),
        (Row::separator(), Action::Unavailable),
        (
            Row::gate("Pause work", false).with_reason("not available yet"),
            Action::Unavailable,
        ),
        (live_gate(s, "Restart Codex", "restart"), Action::Restart),
        (live_gate(s, "End session", "stop"), Action::Stop),
        (Row::separator(), Action::Unavailable),
        (
            session_gate(s, "Archive session", "archive"),
            Action::Archive,
        ),
        (session_gate(s, "Delete session", "delete"), Action::Delete),
        (Row::separator(), Action::Unavailable),
        (
            Row::gate("Experimental auto-start after reset", false)
                .with_reason("not available yet"),
            Action::Unavailable,
        ),
        (
            Row::gate("Experimental auto-switch", false).with_reason("not available yet"),
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
    if pending(s) {
        return "operation running";
    }
    match s["resume_block_reason"].as_str() {
        Some("unknown_account") => "unknown account",
        Some("account_deleted") => "account deleted",
        Some("history_unavailable") => "history unavailable",
        Some("host_unavailable") => "host unavailable",
        Some("already_running") => "already running",
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
/// Compact state marker: the provider's spinner while busy, a dot while running
/// idle, nothing for saved history.
pub fn state_marker(session: &Value, now_ms: u64) -> &'static str {
    if busy(session) {
        if is_gemini(session) {
            gemini_spinner(now_ms)
        } else if is_claude(session) {
            claude_spinner(now_ms)
        } else {
            spinner(now_ms)
        }
    } else if session["availability"] == "running" {
        "●"
    } else {
        ""
    }
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
/// Full session name without control characters (never pre-shortened).
pub fn full_name(s: &Value) -> String {
    s["name"]
        .as_str()
        .unwrap_or("Unnamed")
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}
pub fn rows_with_archives(sessions: &[Value], known: bool, archives: bool) -> Vec<Row> {
    let mut rows = vec![];
    let _ = known;
    rows.extend(session_layout(sessions).into_iter().map(|i| match i {
        Some(i) => Row::yes(full_name(&sessions[i])),
        None => Row::separator(),
    }));
    if !sessions.is_empty() {
        rows.push(Row::separator());
    }
    if archives {
        rows.push(Row::yes("Archived sessions"));
    }
    rows.push(Row::yes("Operations"));
    rows
}
/// Name budget grows with the popup: about 30% of its width, 12–40 cells.
pub fn name_budget(width: usize) -> usize {
    (width * 3 / 10).clamp(12, 40)
}
/// Selected-name marquee: pause at the start, scroll left one cell at a time to
/// reveal the end, pause, then loop. Hidden text is marked with `…` on either side.
pub fn marquee(text: &str, width: usize, now_ms: u64) -> String {
    const PAUSE: u64 = 1500;
    const STEP: u64 = 160;
    if UnicodeWidthStr::width(text) <= width || width < 3 {
        return ellipsize(text, width);
    }
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    let tail = |k: usize| graphemes[k..].concat();
    // Smallest offset whose remainder fits after a leading ellipsis.
    let last = (1..graphemes.len())
        .find(|k| UnicodeWidthStr::width(tail(*k).as_str()) < width)
        .unwrap_or(graphemes.len() - 1);
    let cycle = PAUSE * 2 + STEP * last as u64;
    let t = now_ms % cycle;
    let offset = if t < PAUSE {
        0
    } else {
        (((t - PAUSE) / STEP) as usize + 1).min(last)
    };
    if offset == 0 {
        ellipsize(text, width)
    } else {
        ellipsize(&format!("…{}", tail(offset)), width)
    }
}
/// What a session row shows after its account column while selected and no
/// badge occupies that place.
fn hover_detail(session: &Value, now: i64) -> Option<String> {
    if session["availability"] == "stale" {
        Some("stale".into())
    } else if session["availability"] == "unknown" {
        Some("state unknown".into())
    } else if session["availability"] != "running" && session["attachment"].is_object() {
        Some("host unavailable".into())
    } else if session["availability"] != "running" {
        None
    } else if let Some(start) = session["active_since"].as_i64() {
        Some(duration(now - start))
    } else if session["activity"] == "unknown" {
        Some("state unknown".into())
    } else {
        None
    }
}
/// Session list presentation inputs beyond the sessions themselves.
#[derive(Clone, Copy, Default)]
pub struct Extras<'a> {
    pub archives: bool,
    pub badges: Option<&'a std::collections::BTreeMap<String, Badge>>,
    pub badge_focus: bool,
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
    paint_with(
        canvas,
        options,
        sessions,
        known,
        bottom,
        now,
        controls_start,
        Extras::default(),
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
    paint_with(
        canvas,
        options,
        sessions,
        known,
        bottom,
        now,
        controls_start,
        Extras {
            archives,
            ..Extras::default()
        },
    )
}
/// Columns: bullet, name (scaled to the popup), state marker, dim provider and
/// account, then the operation badge or the selected row's detail hanging right.
#[allow(clippy::too_many_arguments)]
pub fn paint_with(
    canvas: &mut Canvas,
    options: &mut Options,
    sessions: &[Value],
    known: bool,
    bottom: usize,
    now: i64,
    controls_start: usize,
    extras: Extras<'_>,
) -> (usize, usize) {
    let now_ms = canvas.theme.as_ref().map_or(0, |t| t.now);
    let mut rows = rows_with_archives(sessions, known, extras.archives);
    let account_width = sessions
        .iter()
        .map(|s| UnicodeWidthStr::width(account_tag(s).as_str()))
        .max()
        .unwrap_or(0);
    // Marker column (two spaces, one cell) and the account column (two spaces).
    let columns = 3 + 2 + account_width;
    let longest = sessions
        .iter()
        .map(|s| UnicodeWidthStr::width(full_name(s).as_str()))
        .max()
        .unwrap_or(0);
    let budget = name_budget(canvas.width);
    let fits = |name: usize| name + columns + 8 <= canvas.width;
    let mut name_width = longest.min(budget);
    while name_width > 8 && !fits(name_width) {
        name_width -= 1;
    }
    let show_accounts = !sessions.is_empty() && fits(name_width);
    if !show_accounts {
        name_width = longest.min(budget).min(canvas.width.saturating_sub(8));
    }
    let table_width = name_width + if show_accounts { columns } else { 0 };
    let layout = session_layout(sessions);
    for (i, session) in layout
        .iter()
        .enumerate()
        .filter_map(|(row, i)| i.map(|i| (row, &sessions[i])))
    {
        let name = full_name(session);
        rows[i].label = if options.selected == i && !options.back {
            marquee(&name, name_width, now_ms)
        } else {
            ellipsize(&name, name_width)
        };
        let width = UnicodeWidthStr::width(rows[i].label.as_str());
        rows[i]
            .label
            .push_str(&" ".repeat(table_width.saturating_sub(width)));
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
    let dim = |c: [u8; 3], back: bool| {
        if back {
            c.map(|n| (u16::from(n) * 60 / 100) as u8)
        } else {
            c
        }
    };
    for (index, session) in layout
        .iter()
        .enumerate()
        .filter_map(|(row, i)| i.map(|i| (row, &sessions[i])))
    {
        if index < options.scroll || index >= options.scroll + capacity {
            continue;
        }
        let y = 3 + index - options.scroll;
        let selected = options.selected == index && !options.back;
        let mut x = left + 2 + name_width;
        if show_accounts {
            let marker = state_marker(session, now_ms);
            if !marker.is_empty() {
                canvas.accent_text(
                    x + 2,
                    y,
                    marker,
                    Accent {
                        brightness: if options.back {
                            600
                        } else if busy(session) || selected {
                            1000
                        } else {
                            700
                        },
                        bold: false,
                        flash: false,
                        ..Accent::default()
                    },
                );
            }
            canvas.widget_text(
                x + 5,
                y,
                &account_tag(session),
                Cell {
                    fg: dim(if selected { SECONDARY } else { MUTED }, options.back),
                    ..Cell::default()
                },
                false,
            );
            x += columns;
        }
        // Anything after the table hangs right of the centred block and is shown
        // only where it fits, keeping one cell before the edge.
        let room = canvas.width.saturating_sub(x + 3);
        let badge = extras
            .badges
            .and_then(|b| session["id"].as_str().and_then(|id| b.get(id)));
        if let Some(badge) = badge {
            let focused = selected && extras.badge_focus;
            if room >= 1 {
                let icon = badge.icon(now_ms);
                if badge.active {
                    canvas.accent_text(
                        x + 2,
                        y,
                        icon,
                        Accent {
                            brightness: if options.back { 600 } else { 1000 },
                            bold: false,
                            flash: false,
                            ..Accent::default()
                        },
                    );
                } else {
                    canvas.widget_text(
                        x + 2,
                        y,
                        icon,
                        Cell {
                            fg: dim(FAILURE, options.back),
                            ..Cell::default()
                        },
                        false,
                    );
                }
                let arrow = if focused { 2 } else { 0 };
                let text = ellipsize(&badge.text, room.saturating_sub(2 + arrow));
                if !text.is_empty() {
                    canvas.widget_text(
                        x + 4,
                        y,
                        &text,
                        Cell {
                            fg: dim(
                                if focused {
                                    SELECTED
                                } else if selected {
                                    SECONDARY
                                } else {
                                    MUTED
                                },
                                options.back,
                            ),
                            bold: focused,
                            ..Cell::default()
                        },
                        false,
                    );
                    if focused {
                        canvas.widget_text(
                            x + 4 + UnicodeWidthStr::width(text.as_str()),
                            y,
                            " ◀",
                            Cell {
                                fg: SELECTED,
                                bold: true,
                                ..Cell::default()
                            },
                            false,
                        );
                    }
                }
            }
        } else if selected
            && let Some(detail) = hover_detail(session, now)
            && UnicodeWidthStr::width(detail.as_str()) <= room
        {
            canvas.widget_text(
                x + 2,
                y,
                &detail,
                Cell {
                    fg: MUTED,
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
    fn text(c: &Canvas, y: usize) -> String {
        c.cells[y * c.width..(y + 1) * c.width]
            .iter()
            .map(|c| c.glyph.as_str())
            .collect()
    }
    fn labels(s: &Value, resume: bool) -> Vec<(String, bool)> {
        actions(s, resume)
            .into_iter()
            .filter(|(r, _)| !r.separator)
            .map(|(r, _)| (r.label, r.enabled))
            .collect()
    }
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
        assert!(rows[2].separator);
        assert_eq!(rows[3].label, "Saved");
        assert_eq!(rows[5].label, "Operations");
        for index in 0..sessions.len() {
            assert_eq!(
                session_index(&sessions, session_row(&sessions, index)),
                Some(index)
            );
        }
    }
    #[test]
    fn empty_session_menu_has_only_operations_and_archives_are_conditional() {
        let names = |rows: Vec<Row>| rows.into_iter().map(|r| r.label).collect::<Vec<_>>();
        assert_eq!(names(rows(&[], true)), vec!["Operations"]);
        assert_eq!(
            names(rows_with_archives(&[], true, true)),
            vec!["Archived sessions", "Operations"]
        );
        assert_eq!(
            names(rows_with_archives(&[json!({"name":"Saved"})], true, true)),
            vec!["Saved", "", "Archived sessions", "Operations"]
        );
    }
    #[test]
    fn unavailable_actions_are_greyed_with_one_aligned_short_reason() {
        let s = json!({"availability":"unavailable","attachment":{"host":"herdr"},"resume_block_reason":"host_unavailable","capabilities":{}});
        let rows = actions(&s, true);
        for (row, action) in &rows {
            if matches!(action, Action::Resume | Action::Archive | Action::Delete) {
                assert!(!row.enabled);
                assert_eq!(row.preview.as_deref(), Some("· host unavailable"));
                assert!(!row.preview_selected_only, "reasons are always visible");
                assert_eq!(row.preview_fg, MUTED);
            }
        }
        let columns: Vec<_> = rows
            .iter()
            .filter(|(r, _)| r.preview.is_some())
            .map(|(r, _)| r.preview_column)
            .collect();
        assert!(columns.iter().all(|c| c.is_some() && *c == columns[0]));
        // A moving session explains the operation instead of leaking its kind.
        let moving =
            json!({"availability":"saved","resume_block_reason":"moving","capabilities":{}});
        assert_eq!(
            actions(&moving, true)[0].0.preview.as_deref(),
            Some("· operation running")
        );
        let tmux = json!({"availability":"running","attachment":{"host":"tmux"}});
        assert_eq!(
            actions(&tmux, true)[0].0.preview.as_deref(),
            Some("· in tmux")
        );
        let mut c = Canvas::new(80, 30);
        let rows: Vec<Row> = actions(&tmux, true).into_iter().map(|(r, _)| r).collect();
        Options {
            selected: 1,
            ..Options::default()
        }
        .paint_menu(&mut c, &rows, 3, 26, 29);
        assert!(text(&c, 3).contains("· in tmux"), "visible without hover");
    }
    #[test]
    fn sentence_case_menus_and_archive_before_delete() {
        let saved = json!({"availability":"saved","capabilities":{"resume":true,"rename":true}});
        assert_eq!(
            labels(&saved, true)
                .into_iter()
                .map(|(l, _)| l)
                .collect::<Vec<_>>(),
            vec![
                "Start session",
                "Session details",
                "Rename session",
                "Archive session",
                "Delete session"
            ]
        );
        for availability in ["running", "saved", "archived"] {
            let session = json!({"availability":availability,"activity":"idle","capabilities":{"archive":availability=="saved","delete":availability!="running"}});
            let rows = actions(&session, true);
            assert!(!rows.iter().any(|(r, _)| r.label == "Pause work"));
            if availability == "archived" {
                assert!(rows.iter().any(|(_, a)| *a == Action::Restore));
                continue;
            }
            let archive = rows
                .iter()
                .position(|(_, a)| *a == Action::Archive)
                .unwrap();
            assert_eq!(rows[archive + 1].1, Action::Delete);
        }
        let busy = json!({"availability":"running","activity":"busy","capabilities":{}});
        assert!(
            actions(&busy, true)
                .iter()
                .any(|(r, _)| r.label == "Pause work" && !r.enabled)
        );
    }
    fn claude(availability: &str, attached: bool) -> Value {
        json!({"provider":"claude","account_id":"claude:main","availability":availability,
            "attachment":if attached{json!({"host":"herdr","target":"herdr/default/w:p","pid":9})}else{Value::Null},
            "capabilities":{"focus":attached,"rename":true,"resume":availability=="stopped",
                "switch_account":true,"restart":true,"stop":true,"archive":false,"delete":false,
                "move_space":false,"move_project":false,"change_directory":false,"steer":false}})
    }
    #[test]
    fn claude_sessions_share_the_menu_shape_with_unsupported_rows_greyed() {
        let rows = labels(&claude("running", true), true);
        assert_eq!(rows[0], ("Focus session".into(), true));
        assert_eq!(rows[1], ("Session details".into(), true));
        assert_eq!(rows[2], ("Rename session".into(), true));
        // Codex-only lifecycle rows never enable, even if a payload claims them.
        assert!(rows[3..].iter().all(|(_, enabled)| !enabled));
        let all = actions(&claude("running", true), true);
        let stop = all.iter().find(|(r, _)| r.label == "End session").unwrap();
        assert_eq!(stop.1, Action::Unavailable);
        assert_eq!(
            stop.0.preview.as_deref(),
            Some("· not supported for Claude Code")
        );
        let gemini =
            json!({"provider":"gemini","availability":"stopped","capabilities":{"resume":true}});
        assert!(
            actions(&gemini, true)
                .iter()
                .any(|(r, _)| r.preview.as_deref() == Some("· not supported for Gemini CLI"))
        );
        assert_eq!(
            labels(&claude("stopped", false), true)[0],
            ("Start session".into(), true)
        );
    }
    #[test]
    fn claude_focus_and_start_are_disabled_with_reasons_for_detached_stale_unknown() {
        let detached = actions(&claude("running", false), true);
        assert!(!detached[0].0.enabled);
        assert_eq!(detached[0].0.preview.as_deref(), Some("· not in HERDR"));
        for (state, reason) in [("stale", "· stale"), ("unknown", "· state unknown")] {
            let rows = actions(&claude(state, false), true);
            assert_eq!(rows[0].0.label, "Start session");
            assert!(!rows[0].0.enabled);
            assert_eq!(rows[0].0.preview.as_deref(), Some(reason));
            assert_eq!(rows[0].1, Action::Resume);
        }
        assert!(!actions(&claude("stopped", false), false)[0].0.enabled);
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
    #[test]
    fn duration_matches_reference() {
        assert_eq!(duration(-1), "0:00");
        assert_eq!(duration(61), "1:01");
        assert_eq!(duration(3661), "1:01:01");
        assert_eq!(duration(90061), "1:01:01:01");
    }
    #[test]
    fn spinners_follow_their_provider_references() {
        let frames: Vec<_> = (0..10).map(|i| gemini_spinner(i * 80)).collect();
        assert_eq!(frames, ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);
        assert_eq!(spinner(100), "⠙");
        assert_eq!(spinner(1000), "⠋");
        let claude = ["·", "✢", "*", "✶", "✻", "✽"];
        let want = [0, 0, 0, 1, 2, 2, 3, 4, 5, 5, 5, 5, 5, 4, 3, 3, 2, 1, 0, 0];
        let got: Vec<_> = (0..20).map(|i| claude_spinner(i * 100)).collect();
        assert_eq!(got, want.map(|i| claude[i]));
        assert_eq!(claude_spinner(2000 + 800), claude_spinner(800));
    }
    fn sessions() -> Vec<Value> {
        vec![
            json!({"id":"a","name":"liq-glass-lead","account_id":"codex:gate1","availability":"running","activity":"busy","active_since":1}),
            json!({"id":"b","name":"docs","provider":"claude","account_id":"claude:main","availability":"running","activity":"idle"}),
            json!({"id":"c","name":"old-notes","account_id":"codex:work","availability":"saved"}),
        ]
    }
    #[test]
    fn every_row_has_a_state_marker_and_dim_provider_account_column() {
        let sessions = sessions();
        let mut field = crate::render::Field::new(100, 20, 1967);
        let mut c = field.paint(4100);
        paint(&mut c, &mut Options::default(), &sessions, true, 18, 62, 19);
        let busy = text(&c, 3);
        let idle = text(&c, 4);
        let saved = text(&c, 6);
        assert!(busy.contains("⠙") && busy.contains("⏣ gate1"), "{busy}");
        assert!(idle.contains("●") && idle.contains("✻ main"), "{idle}");
        assert!(saved.contains("⏣ work") && !saved.contains('●'), "{saved}");
        assert!(!saved.contains("inactive"));
        // Account columns start at one x on every row.
        let x = |line: &str, glyph: char| line.chars().position(|g| g == glyph).unwrap();
        assert_eq!(x(&busy, '⏣'), x(&saved, '⏣'));
        assert_eq!(x(&busy, '⏣'), x(&idle, '✻'));
        assert_eq!(x(&busy, '⠙'), x(&idle, '●'));
        assert_eq!(
            c.cells[3 * 100 + x(&busy, '⏣')].fg,
            SECONDARY,
            "selected row"
        );
        let tag = &c.cells[4 * 100 + x(&idle, '✻')];
        assert_eq!(tag.fg, MUTED);
        assert!(!tag.bold);
        let marker = &c.cells[3 * 100 + x(&busy, '⠙')];
        assert!(!marker.bold);
        // Hover brightens the account and shows the timer; no "(inactive)".
        let mut c = field.paint(4100);
        paint(
            &mut c,
            &mut Options::default(),
            &sessions,
            true,
            18,
            90062,
            19,
        );
        assert!(text(&c, 3).contains("1:01:01:01"));
        assert_eq!(c.cells[3 * 100 + x(&busy, '⏣')].fg, SECONDARY);
        let mut c = field.paint(4100);
        paint(
            &mut c,
            &mut Options {
                selected: 3,
                ..Options::default()
            },
            &sessions,
            true,
            18,
            62,
            19,
        );
        assert!(!(0..20).any(|y| text(&c, y).contains("inactive")));
        // Idle, exited or unknown activity has no spinner.
        for (availability, activity) in [
            ("running", "idle"),
            ("saved", "busy"),
            ("running", "unknown"),
        ] {
            let s = vec![
                json!({"name":"x","account_id":"codex:a","availability":availability,"activity":activity}),
            ];
            let mut c = field.paint(4200);
            paint(&mut c, &mut Options::default(), &s, true, 18, 62, 19);
            assert!(!c.cells.iter().any(|c| {
                ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"].contains(&c.glyph.as_str())
            }));
        }
    }
    #[test]
    fn name_budget_scales_with_width_and_unselected_names_truncate() {
        assert_eq!(name_budget(40), 12);
        assert_eq!(name_budget(80), 24);
        assert_eq!(name_budget(110), 33);
        assert_eq!(name_budget(400), 40);
        let long = "abcdefghijklmnopqrstuvwxyz0123456789abcdefghij";
        let sessions = vec![
            json!({"id":"a","name":"short","account_id":"codex:p","availability":"saved"}),
            json!({"id":"b","name":long,"account_id":"codex:p","availability":"saved"}),
        ];
        for width in [40, 80, 140] {
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
            let row = text(&c, 4);
            let shown: String = row
                .trim_start()
                .chars()
                .skip(2)
                .take_while(|c| *c != ' ')
                .collect();
            assert!(shown.ends_with('…'), "{width}: {row}");
            assert!(
                shown.chars().count() <= name_budget(width),
                "{width}: {shown}"
            );
        }
    }
    #[test]
    fn selected_long_name_marquees_with_pauses_and_loops() {
        let name = "abcdefghijklmnop";
        assert_eq!(marquee(name, 8, 0), "abcdefg…");
        assert_eq!(marquee(name, 8, 1499), "abcdefg…");
        assert_eq!(marquee(name, 8, 1500), "…bcdefg…");
        assert_eq!(marquee(name, 8, 1660), "…cdefgh…");
        let end = marquee(name, 8, 1500 + 160 * 40);
        assert_eq!(end, "…jklmnop", "stops with the end revealed");
        let cycle = 1500 * 2 + 160 * 9;
        assert_eq!(
            marquee(name, 8, cycle),
            "abcdefg…",
            "loops back to the start"
        );
        assert_eq!(marquee("short", 8, 5000), "short");
        // Only the selected row scrolls.
        let sessions = vec![
            json!({"id":"a","name":"abcdefghijklmnopqrstuvwxyz0123456789","account_id":"codex:p","availability":"saved"}),
            json!({"id":"b","name":"abcdefghijklmnopqrstuvwxyz0123456789","account_id":"codex:p","availability":"saved"}),
        ];
        let mut field = crate::render::Field::new(60, 20, 1967);
        let mut c = field.paint(1500 + 160 * 3);
        paint(
            &mut c,
            &mut Options::default(),
            &sessions,
            true,
            16,
            100,
            19,
        );
        assert!(text(&c, 3).contains("…ef"), "{}", text(&c, 3));
        assert!(text(&c, 4).contains("abc"));
    }
    #[test]
    fn badges_hang_right_animate_when_active_and_focus_with_an_arrow() {
        let sessions = sessions();
        let mut badges = std::collections::BTreeMap::new();
        badges.insert(
            "a".to_owned(),
            Badge {
                kind: "operation".into(),
                id: "op".into(),
                text: "moving to work".into(),
                active: true,
            },
        );
        badges.insert(
            "c".to_owned(),
            Badge {
                kind: "operation".into(),
                id: "op2".into(),
                text: "move failed".into(),
                active: false,
            },
        );
        let draw = |now: u64, focus: bool| {
            let mut c = crate::render::Field::new(100, 20, 1967).paint(now);
            paint_with(
                &mut c,
                &mut Options::default(),
                &sessions,
                true,
                18,
                62,
                19,
                Extras {
                    badges: Some(&badges),
                    badge_focus: focus,
                    ..Extras::default()
                },
            );
            c
        };
        let c = draw(0, false);
        assert!(text(&c, 3).contains("◜ moving to work"));
        assert!(text(&c, 6).contains("✗ move failed"));
        assert!(
            text(&draw(150, false), 3).contains("◝ moving to work"),
            "icon animates"
        );
        let x = text(&c, 6).chars().position(|g| g == '✗').unwrap();
        assert_eq!(c.cells[6 * 100 + x].fg, FAILURE);
        let focused = draw(0, true);
        assert!(text(&focused, 3).contains("moving to work ◀"));
        let m = text(&focused, 3).chars().position(|g| g == 'm').unwrap();
        assert!(focused.cells[3 * 100 + m].bold);
    }
}
