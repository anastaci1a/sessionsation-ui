//! Session rows: name, state marker, dim provider/account column and operation badge.
use crate::preferences::{
    AntigravitySpinner, ClaudeSpinner, CodexSpinner, GeminiSpinner, Spinners,
};
use crate::widgets::RowMode;
use crate::{
    render::{Accent, Canvas, Cell},
    usage::short_name,
    widgets::{MUTED, Options, Row, SECONDARY, SELECTED, fit_selectable},
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
/// A session's pending or recently failed operation, shown at the end of its row.
#[derive(Clone, Debug, PartialEq)]
pub struct Badge {
    /// Operation list key: ("operation" | "launch" | "resume-launch", id).
    pub kind: String,
    pub id: String,
    pub text: String,
    pub active: bool,
    /// Waiting in the UI operation queue; not started yet.
    pub queued: bool,
}
/// Operation spinner frames (badges, the Operations popup and the status summary).
pub const OPERATION_SPINNER_FRAMES: [&str; 4] = ["◜", "◝", "◞", "◟"];
/// Milliseconds per operation spinner frame.
pub const OPERATION_SPINNER_FRAME_MS: u64 = 150;
/// Marker for an operation waiting in the queue.
pub const QUEUED_ICON: &str = "◌";
/// The shared operation spinner frame at `now_ms`.
pub fn operation_spinner(now_ms: u64) -> &'static str {
    OPERATION_SPINNER_FRAMES
        [(now_ms / OPERATION_SPINNER_FRAME_MS % OPERATION_SPINNER_FRAMES.len() as u64) as usize]
}
impl Badge {
    /// Animated arc for active work, a dotted circle while queued, a cross for failures.
    pub fn icon(&self, now_ms: u64) -> &'static str {
        if self.active {
            operation_spinner(now_ms)
        } else if self.queued {
            QUEUED_ICON
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
    if pending(s) && s["availability"] != "running" && QUEUEABLE.contains(&capability) {
        return queue_gate(label);
    }
    let reason = if pending(s) {
        "operation running"
    } else if let Some(reason) = backend_reason(s, capability) {
        return row.with_reason(reason);
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
/// A per-session refusal from `capability_reasons` ("CODE: text"), as a short
/// row reason. Provider-wide explanations (no code) are not per-session reasons.
fn backend_reason(s: &Value, capability: &str) -> Option<String> {
    let reason = s["capability_reasons"][capability].as_str()?;
    let (code, text) = reason.split_once(": ").unwrap_or((reason, ""));
    if code.is_empty() || !code.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') {
        return None;
    }
    let short = match code {
        "SESSION_RUNNING" => "exit the agent first",
        "SESSION_HOSTED" => "hosted worker running",
        "ACCOUNT_DELETED" => "account deleted",
        "SESSION_STATE_UNKNOWN" => "state unknown",
        "HISTORY_UNAVAILABLE" => "history unavailable",
        "SESSION_ALREADY_ARCHIVED" => "already archived",
        "SESSION_NOT_ARCHIVED" => "not archived",
        "SESSION_ARCHIVED" => "restore it first",
        _ => {
            let text = text.trim().trim_end_matches('.');
            let mut chars = text.chars();
            return Some(match chars.next() {
                Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
                None => "unavailable".into(),
            });
        }
    };
    Some(short.into())
}
/// A Claude Code or Gemini CLI lifecycle row, driven by the backend capability:
/// enabled when supported for this session, queued behind a running operation,
/// else the session's own reason, or "not supported for <provider>" when the
/// provider has no such control at all.
fn native_gate(s: &Value, label: &str, capability: &str, action: Action) -> (Row, Action) {
    let row = Row::gate(label, s["capabilities"][capability] == true);
    if row.enabled {
        return (row, action);
    }
    if pending(s) && QUEUEABLE.contains(&capability) {
        return (queue_gate(label), action);
    }
    let reason = backend_reason(s, capability)
        .unwrap_or_else(|| format!("not supported for {}", provider_label_of(s)));
    (row.with_reason(reason), Action::Unavailable)
}
/// Lifecycle capabilities whose actions queue behind a running operation.
const QUEUEABLE: [&str; 8] = [
    "switch_account",
    "move_space",
    "move_project",
    "change_directory",
    "restart",
    "stop",
    "archive",
    "delete",
];
/// While another operation runs on the session, the action is accepted and queued.
fn queue_gate(label: &str) -> Row {
    Row::gate(label, true).with_reason("queues after current")
}
/// A disabled lifecycle row: the shared reason, or a pending operation.
fn live_gate(s: &Value, label: &str, capability: &str) -> Row {
    let row = Row::gate(label, s["capabilities"][capability] == true);
    if row.enabled {
        row
    } else if pending(s) && QUEUEABLE.contains(&capability) {
        queue_gate(label)
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
pub fn is_antigravity(s: &Value) -> bool {
    s["provider"] == "antigravity"
        || s["account_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("antigravity:"))
}
/// Claude Code, Gemini CLI and Antigravity CLI sessions share one capability-gated action set.
pub fn is_native_provider(s: &Value) -> bool {
    is_claude(s) || is_gemini(s) || is_antigravity(s)
}
pub fn is_claude(s: &Value) -> bool {
    s["provider"] == "claude"
        || s["account_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("claude:"))
}
/// Claude Code and Gemini CLI sessions enable only what the backend capabilities
/// allow: focus (verified HERDR attachment), details, rename (Sessionsation label),
/// start when stopped, and archive, restore and delete of stopped conversations
/// (contract revision 17). Lifecycle rows the provider lacks stay visible but
/// greyed with the backend's reason, so every provider shares one menu shape.
fn claude_actions(s: &Value, resume: bool) -> Vec<(Row, Action)> {
    let c = &s["capabilities"];
    if s["availability"] == "archived" {
        return archived_actions(s);
    }
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
    vec![
        first,
        (Row::yes("Session details"), Action::Details),
        (
            Row::gate("Rename session", c["rename"] == true),
            Action::Rename,
        ),
        (Row::separator(), Action::Unavailable),
        native_gate(s, "Switch account", "switch_account", Action::Switch),
        native_gate(s, "Move to Space", "move_space", Action::MoveSpace),
        (Row::separator(), Action::Unavailable),
        native_gate(s, "Restart session", "restart", Action::Restart),
        native_gate(s, "End session", "stop", Action::Stop),
        (Row::separator(), Action::Unavailable),
        native_gate(s, "Archive session", "archive", Action::Archive),
        native_gate(s, "Delete session", "delete", Action::Delete),
    ]
}
/// An archived conversation (any provider): restore, details and delete.
fn archived_actions(s: &Value) -> Vec<(Row, Action)> {
    let c = &s["capabilities"];
    vec![
        (
            Row::gate("Restore session", c["restore"] == true),
            Action::Restore,
        ),
        (Row::yes("Session details"), Action::Details),
        (Row::separator(), Action::Unavailable),
        (session_gate(s, "Delete session", "delete"), Action::Delete),
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
        return archived_actions(s);
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
const BRAILLE_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
fn spinner_at(ms: u64, now_ms: u64) -> &'static str {
    BRAILLE_FRAMES[(now_ms / ms % 10) as usize]
}
/// cli-main-menu/select.bash: ten Braille frames, one frame every 100 ms.
pub fn spinner(now_ms: u64) -> &'static str {
    spinner_at(100, now_ms)
}
/// Gemini CLI's ink-spinner `dots`: one Braille frame every 80 ms, never bold.
pub fn gemini_spinner(now_ms: u64) -> &'static str {
    spinner_at(GEMINI_NATIVE_MS, now_ms)
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
fn running(session: &Value) -> bool {
    session["availability"] == "running"
}
/// Milliseconds per frame of the themed Codex and Gemini spinners.
pub const THEMED_FRAME_MS: u64 = 120;
/// Codex "fill pulse" (default).
pub const CODEX_FILL_PULSE_FRAMES: [&str; 6] = ["⬡", "⎔", "⏣", "⬢", "⏣", "⎔"];
pub const CODEX_FILL_PULSE_MS: u64 = THEMED_FRAME_MS;
/// Codex "rotate".
pub const CODEX_ROTATE_FRAMES: [&str; 4] = ["⬢", "⬣", "⬢", "⬣"];
pub const CODEX_ROTATE_MS: u64 = THEMED_FRAME_MS;
/// Codex "benzene".
pub const CODEX_BENZENE_FRAMES: [&str; 4] = ["⌬", "⏣", "⬡", "⏣"];
pub const CODEX_BENZENE_MS: u64 = THEMED_FRAME_MS;
/// Codex "build-up".
pub const CODEX_BUILD_UP_FRAMES: [&str; 6] = ["·", "⬡", "⏣", "⬢", "⏣", "⬡"];
pub const CODEX_BUILD_UP_MS: u64 = THEMED_FRAME_MS;
/// Codex "native/classic": the Braille spinner at 100 ms.
pub const CODEX_NATIVE_MS: u64 = 100;
/// Gemini "twinkle" (default).
pub const GEMINI_TWINKLE_FRAMES: [&str; 6] = ["⋅", "✧", "⟡", "✦", "⟡", "✧"];
pub const GEMINI_TWINKLE_MS: u64 = THEMED_FRAME_MS;
/// Gemini "spin fill".
pub const GEMINI_SPIN_FILL_FRAMES: [&str; 4] = ["⬖", "⬘", "⬗", "⬙"];
pub const GEMINI_SPIN_FILL_MS: u64 = THEMED_FRAME_MS;
/// Gemini "morph".
pub const GEMINI_MORPH_FRAMES: [&str; 4] = ["✦", "❖", "◆", "❖"];
pub const GEMINI_MORPH_MS: u64 = THEMED_FRAME_MS;
/// Gemini "concave blink".
pub const GEMINI_CONCAVE_BLINK_FRAMES: [&str; 4] = ["✦", "⟡", "✧", "⟡"];
pub const GEMINI_CONCAVE_BLINK_MS: u64 = THEMED_FRAME_MS;
/// Gemini "native": Gemini CLI's own Braille spinner at 80 ms.
pub const GEMINI_NATIVE_MS: u64 = 80;
/// Antigravity "native": the agy TUI's own eight-frame Braille cycle.
pub const ANTIGRAVITY_NATIVE_FRAMES: [&str; 8] = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
pub const ANTIGRAVITY_NATIVE_MS: u64 = 80;
/// Antigravity "pulse": a triangle that grows and shrinks.
pub const ANTIGRAVITY_PULSE_FRAMES: [&str; 6] = ["·", "▵", "▴", "◬", "▴", "▵"];
pub const ANTIGRAVITY_PULSE_MS: u64 = THEMED_FRAME_MS;
/// Antigravity "tilt": the dotted triangle fills its left and right halves in turn.
pub const ANTIGRAVITY_TILT_FRAMES: [&str; 4] = ["◬", "◭", "◬", "◮"];
pub const ANTIGRAVITY_TILT_MS: u64 = THEMED_FRAME_MS;
/// "Glow": a static glyph whose colour oscillates between two hueshift levels at
/// the picker arrow's rate. "Static" is the plain provider symbol, no animation.
pub const CODEX_GLOW_GLYPH: &str = "⏣";
pub const GEMINI_GLOW_GLYPH: &str = "✦";
pub const ANTIGRAVITY_GLOW_GLYPH: &str = "◬";
pub const CLAUDE_GLOW_GLYPH: &str = "✻";

fn cycle(frames: &'static [&'static str], ms: u64, now_ms: u64) -> &'static str {
    frames[(now_ms / ms % frames.len() as u64) as usize]
}
/// The busy frame of the chosen Codex spinner (Glow and Static are static glyphs).
pub fn codex_frame(kind: CodexSpinner, now_ms: u64) -> &'static str {
    match kind {
        CodexSpinner::FillPulse => cycle(&CODEX_FILL_PULSE_FRAMES, CODEX_FILL_PULSE_MS, now_ms),
        CodexSpinner::Rotate => cycle(&CODEX_ROTATE_FRAMES, CODEX_ROTATE_MS, now_ms),
        CodexSpinner::Benzene => cycle(&CODEX_BENZENE_FRAMES, CODEX_BENZENE_MS, now_ms),
        CodexSpinner::BuildUp => cycle(&CODEX_BUILD_UP_FRAMES, CODEX_BUILD_UP_MS, now_ms),
        CodexSpinner::Native => spinner_at(CODEX_NATIVE_MS, now_ms),
        CodexSpinner::Glow => CODEX_GLOW_GLYPH,
        CodexSpinner::Static => crate::usage::provider_symbol("codex"),
    }
}
/// The busy frame of the chosen Gemini spinner (Glow and Static are static glyphs).
pub fn gemini_frame(kind: GeminiSpinner, now_ms: u64) -> &'static str {
    match kind {
        GeminiSpinner::Twinkle => cycle(&GEMINI_TWINKLE_FRAMES, GEMINI_TWINKLE_MS, now_ms),
        GeminiSpinner::SpinFill => cycle(&GEMINI_SPIN_FILL_FRAMES, GEMINI_SPIN_FILL_MS, now_ms),
        GeminiSpinner::Morph => cycle(&GEMINI_MORPH_FRAMES, GEMINI_MORPH_MS, now_ms),
        GeminiSpinner::ConcaveBlink => cycle(
            &GEMINI_CONCAVE_BLINK_FRAMES,
            GEMINI_CONCAVE_BLINK_MS,
            now_ms,
        ),
        GeminiSpinner::Native => spinner_at(GEMINI_NATIVE_MS, now_ms),
        GeminiSpinner::Classic => spinner(now_ms),
        GeminiSpinner::Glow => GEMINI_GLOW_GLYPH,
        GeminiSpinner::Static => crate::usage::provider_symbol("gemini"),
    }
}
/// The busy frame of the chosen Antigravity spinner (Glow and Static are static glyphs).
pub fn antigravity_frame(kind: AntigravitySpinner, now_ms: u64) -> &'static str {
    match kind {
        AntigravitySpinner::Native => {
            cycle(&ANTIGRAVITY_NATIVE_FRAMES, ANTIGRAVITY_NATIVE_MS, now_ms)
        }
        AntigravitySpinner::Pulse => cycle(&ANTIGRAVITY_PULSE_FRAMES, ANTIGRAVITY_PULSE_MS, now_ms),
        AntigravitySpinner::Tilt => cycle(&ANTIGRAVITY_TILT_FRAMES, ANTIGRAVITY_TILT_MS, now_ms),
        AntigravitySpinner::Classic => spinner(now_ms),
        AntigravitySpinner::Glow => ANTIGRAVITY_GLOW_GLYPH,
        AntigravitySpinner::Static => crate::usage::provider_symbol("antigravity"),
    }
}
/// The busy frame of the chosen Claude spinner (Glow and Static are static glyphs).
pub fn claude_frame(kind: ClaudeSpinner, now_ms: u64) -> &'static str {
    match kind {
        ClaudeSpinner::Native => claude_spinner(now_ms),
        ClaudeSpinner::Classic => spinner(now_ms),
        ClaudeSpinner::Glow => CLAUDE_GLOW_GLYPH,
        ClaudeSpinner::Static => crate::usage::provider_symbol("claude"),
    }
}
/// Whether the busy row's chosen spinner is the static glyph with a colour oscillation.
pub fn row_glows(session: &Value, spinners: Spinners) -> bool {
    busy(session)
        && if is_antigravity(session) {
            spinners.antigravity == AntigravitySpinner::Glow
        } else if is_gemini(session) {
            spinners.gemini == GeminiSpinner::Glow
        } else if is_claude(session) {
            spinners.claude == ClaudeSpinner::Glow
        } else {
            spinners.codex == CodexSpinner::Glow
        }
}
/// The row's provider icon: the chosen provider spinner frame replaces it while
/// a turn runs, otherwise it is the static provider symbol.
pub fn row_icon(session: &Value, now_ms: u64, spinners: Spinners) -> &'static str {
    if busy(session) {
        if is_antigravity(session) {
            antigravity_frame(spinners.antigravity, now_ms)
        } else if is_gemini(session) {
            gemini_frame(spinners.gemini, now_ms)
        } else if is_claude(session) {
            claude_frame(spinners.claude, now_ms)
        } else {
            codex_frame(spinners.codex, now_ms)
        }
    } else {
        crate::usage::provider_symbol(provider(session))
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
/// Start (Unix seconds) of the session's attachment process, when the backend
/// reports `attachment.process_start` (Linux /proc start ticks) and that same
/// process is still alive on this host. Read once per state update, never while
/// painting; any mismatch or unreadable value yields nothing.
pub fn attachment_started(session: &Value) -> Option<i64> {
    let attachment = &session["attachment"];
    let pid = attachment["pid"].as_u64()?;
    let ticks = attachment["process_start"].as_u64()?;
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Fields after the parenthesised command; starttime is field 22 overall.
    let after = stat.rsplit_once(')')?.1;
    let start: u64 = after.split_whitespace().nth(19)?.parse().ok()?;
    if start != ticks {
        return None;
    }
    let boot: i64 = std::fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("btime ")?.trim().parse().ok())?;
    // SAFETY: sysconf has no preconditions.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    (hz > 0).then(|| boot + (ticks / hz as u64) as i64)
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
    } else if let Some(start) = session["active_since"]
        .as_i64()
        .or_else(|| session["attachment_since"].as_i64())
    {
        // The live counter, in the shared `duration` format.
        Some(format!("({})", duration(now - start)))
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
    pub spinners: Spinners,
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
    // The account column sits one space after the longest name.
    let columns = 1 + account_width;
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
        let selected = options.selected == i && !options.back;
        rows[i].label = fit_selectable(&name, name_width, now_ms, selected);
        let width = UnicodeWidthStr::width(rows[i].label.as_str());
        rows[i]
            .label
            .push_str(&" ".repeat(table_width.saturating_sub(width)));
    }
    // Right-side mode: focus is on a row's operation badge. That row's name turns
    // lighter, every other row's names and values darker, and all bullets and
    // icons drop to the dim hue variant.
    let mode = extras.badge_focus
        && !options.back
        && layout
            .get(options.selected)
            .copied()
            .flatten()
            .and_then(|i| sessions[i]["id"].as_str())
            .is_some_and(|id| extras.badges.is_some_and(|b| b.contains_key(id)));
    options.right_mode = mode;
    if mode {
        for (i, row) in rows.iter_mut().enumerate() {
            row.mode = if i == options.selected {
                RowMode::Active
            } else {
                RowMode::Other
            };
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
    let level = |back: bool| if back { 600 } else { 1000 };
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
        // This row's own visible end: its account tag, or its name without accounts.
        let row_end = if show_accounts {
            x + 1 + UnicodeWidthStr::width(account_tag(session).as_str())
        } else {
            left + 2 + UnicodeWidthStr::width(rows[index].label.trim_end())
        };
        if show_accounts {
            let icon = row_icon(session, now_ms, extras.spinners);
            let glows = row_glows(session, extras.spinners);
            let tag = account_tag(session);
            let rest = tag.split_once(' ').map_or("", |(_, r)| r);
            let tint = dim(if selected { SECONDARY } else { MUTED }, options.back);
            // The selected-bullet level only on the selected row; others (and the
            // back arrow or badge focus) use the dim bullet level.
            let icon_level = if mode { 600 } else { level(!selected) };
            // The selected row's account name and static icon are bold until focus
            // moves onto its badge; an animated icon keeps its own styling.
            let strong = selected && !mode;
            let animated = busy(session);
            if glows {
                // A static glyph whose hueshift oscillates, at the glyph's own cell,
                // from this row's icon level down to the next darker one: selected
                // normal to dim, otherwise dim to the even darker floor. Not bold,
                // like every animated spinner.
                let low = if icon_level == 1000 {
                    crate::render::DIM_LEVEL
                } else {
                    crate::render::GLOW_FLOOR_LEVEL
                };
                let fg = canvas
                    .theme
                    .as_ref()
                    .map_or(MUTED, |t| t.hue_pulse(x + 1, y, icon_level, low));
                canvas.widget_text(
                    x + 1,
                    y,
                    icon,
                    Cell {
                        fg,
                        ..Cell::default()
                    },
                    false,
                );
            } else if running(session) {
                canvas.accent_text(
                    x + 1,
                    y,
                    icon,
                    Accent {
                        brightness: icon_level,
                        bold: strong && !animated,
                        flash: false,
                        ..Accent::default()
                    },
                );
            } else {
                canvas.widget_text(
                    x + 1,
                    y,
                    icon,
                    Cell {
                        fg: tint,
                        bold: strong && !animated,
                        ..Cell::default()
                    },
                    false,
                );
            }
            if mode && !selected {
                canvas.widget_text(
                    x + 3,
                    y,
                    rest,
                    Cell {
                        fg: MUTED,
                        ..Cell::default()
                    },
                    false,
                );
            } else if running(session) {
                // The account name wears the icon's position-based colour.
                canvas.accent_text(
                    x + 3,
                    y,
                    rest,
                    Accent {
                        brightness: if mode { 600 } else { level(!selected) },
                        bold: strong,
                        flash: false,
                        ..Accent::default()
                    },
                );
            } else {
                canvas.widget_text(
                    x + 3,
                    y,
                    rest,
                    Cell {
                        fg: tint,
                        bold: strong,
                        ..Cell::default()
                    },
                    false,
                );
            }
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
                // The gaps before and after the badge icon belong to the row: they
                // cover stars like its text does.
                canvas.widget_text(x, y, "  ", Cell::default(), false);
                if room >= 2 {
                    canvas.widget_text(x + 3, y, " ", Cell::default(), false);
                }
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
                            fg: dim(if badge.queued { SECONDARY } else { FAILURE }, options.back),
                            ..Cell::default()
                        },
                        false,
                    );
                }
                let arrow = if focused { 2 } else { 0 };
                let text =
                    fit_selectable(&badge.text, room.saturating_sub(2 + arrow), now_ms, focused);
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
            // One space after this row's content, shown whole or not at all.
            && row_end + 1 + UnicodeWidthStr::width(detail.as_str()) < canvas.width
        {
            canvas.widget_text(
                row_end + 1,
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
        let rows: Vec<Row> = actions(&tmux, true).into_iter().map(|(r, _)| r).collect();
        let mut c = Canvas::new(130, 30);
        Options {
            selected: 1,
            ..Options::default()
        }
        .paint_menu(&mut c, &rows, 3, 26, 29);
        assert!(text(&c, 3).contains("· in tmux"), "visible without hover");
        // Too narrow beside the centred labels: none is drawn inline and the
        // selected row's reason moves below the list.
        let mut c = Canvas::new(80, 30);
        Options {
            selected: 0,
            ..Options::default()
        }
        .paint_menu(&mut c, &rows, 3, 26, 29);
        assert!(!text(&c, 3).contains("in tmux"));
        assert!((4..30).any(|y| text(&c, y).contains("· in tmux")));
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
    /// A revision 17 Claude session: provider-wide reasons for the lifecycle
    /// controls it lacks, per-session reasons for archive and delete.
    fn claude(availability: &str, attached: bool) -> Value {
        let stopped = availability == "stopped";
        let running = "SESSION_RUNNING: the provider process runs this conversation; end it first";
        json!({"provider":"claude","account_id":"claude:main","availability":availability,
            "attachment":if attached{json!({"host":"herdr","target":"herdr/default/w:p","pid":9})}else{Value::Null},
            "capabilities":{"focus":attached,"rename":true,"resume":stopped,
                "switch_account":false,"restart":false,"stop":false,"archive":stopped,"delete":stopped,
                "move_space":false,"move_project":false,"change_directory":false,"steer":false},
            "capability_reasons":{"switch_account":"native session is bound to its account config home",
                "restart":"no verified Claude process lifecycle control; stop it in its terminal",
                "stop":"no verified Claude process lifecycle control; stop it in its terminal",
                "move_space":"HERDR space moves are verified only for Codex",
                "archive":if stopped{Value::Null}else{json!(running)},
                "delete":if stopped{Value::Null}else{json!(running)}}})
    }
    #[test]
    fn claude_sessions_share_the_menu_shape_with_unsupported_rows_greyed() {
        let rows = labels(&claude("running", true), true);
        assert_eq!(rows[0], ("Focus session".into(), true));
        assert_eq!(rows[1], ("Session details".into(), true));
        assert_eq!(rows[2], ("Rename session".into(), true));
        // Lifecycle rows the provider lacks stay greyed.
        assert!(rows[3..].iter().all(|(_, enabled)| !enabled));
        let all = actions(&claude("running", true), true);
        let stop = all.iter().find(|(r, _)| r.label == "End session").unwrap();
        assert_eq!(stop.1, Action::Unavailable);
        assert_eq!(
            stop.0.preview.as_deref(),
            Some("· not supported for Claude Code")
        );
        // Archive and delete follow the session's own backend reason.
        let archive = all
            .iter()
            .find(|(r, _)| r.label == "Archive session")
            .unwrap();
        assert_eq!(archive.0.preview.as_deref(), Some("· exit the agent first"));
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
    fn native_archive_and_delete_follow_backend_capabilities() {
        // Stopped: both enabled with their real actions.
        let stopped = actions(&claude("stopped", false), true);
        let find = |rows: &[(Row, Action)], l: &str| {
            rows.iter().find(|(r, _)| r.label == l).cloned().unwrap()
        };
        let archive = find(&stopped, "Archive session");
        assert!(archive.0.enabled);
        assert_eq!(archive.1, Action::Archive);
        let delete = find(&stopped, "Delete session");
        assert!(delete.0.enabled);
        assert_eq!(delete.1, Action::Delete);
        assert!(!find(&stopped, "End session").0.enabled);
        // Hosted, deleted account and other refusals show the session's reason.
        for (code, reason) in [
            (
                "SESSION_HOSTED: a Sessionsation-hosted worker runs this conversation; stop it first",
                "· hosted worker running",
            ),
            (
                "ACCOUNT_DELETED: the account home is gone; its history is not managed here",
                "· account deleted",
            ),
            (
                "HISTORY_BUSY: Another operation holds this history.",
                "· another operation holds this history",
            ),
        ] {
            let mut s = claude("stopped", false);
            s["capabilities"]["archive"] = json!(false);
            s["capability_reasons"]["archive"] = json!(code);
            let row = find(&actions(&s, true), "Archive session");
            assert!(!row.0.enabled);
            assert_eq!(row.1, Action::Unavailable);
            assert_eq!(row.0.preview.as_deref(), Some(reason), "{code}");
        }
        // A backend without the revision 17 keys keeps the provider wording.
        let old =
            json!({"provider":"gemini","availability":"stopped","capabilities":{"resume":true}});
        let row = find(&actions(&old, true), "Archive session");
        assert_eq!(
            row.0.preview.as_deref(),
            Some("· not supported for Gemini CLI")
        );
        let agy = json!({"provider":"antigravity","availability":"stopped","capabilities":{"resume":true}});
        let rows = actions(&agy, true);
        for l in [
            "Archive session",
            "Delete session",
            "End session",
            "Switch account",
        ] {
            let r = find(&rows, l);
            assert!(!r.0.enabled && r.1 == Action::Unavailable, "{l}");
            assert_eq!(
                r.0.preview.as_deref(),
                Some("· not supported for Antigravity CLI")
            );
        }
        // Archived native sessions offer Restore and Delete like Codex.
        let mut archived = claude("archived", false);
        archived["capabilities"] = json!({"restore":true,"delete":true});
        let rows = labels(&archived, true);
        assert_eq!(
            rows,
            [
                ("Restore session".into(), true),
                ("Session details".into(), true),
                ("Delete session".into(), true)
            ]
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
    fn operation_glyphs_are_single_cells() {
        for g in OPERATION_SPINNER_FRAMES.iter().chain([&QUEUED_ICON, &"✗"]) {
            assert_eq!(UnicodeWidthStr::width(*g), 1, "{g}");
        }
        let frames: Vec<_> = (0..5)
            .map(|i| operation_spinner(i * OPERATION_SPINNER_FRAME_MS))
            .collect();
        assert_eq!(frames, ["◜", "◝", "◞", "◟", "◜"]);
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
    fn live_counter_is_parenthesised_one_space_after_its_own_row() {
        let mut sessions = sessions();
        // A short account tag on the selected row: the counter follows it, not
        // the shared account column of longer tags.
        sessions[0]["account_id"] = json!("codex:g");
        sessions[2]["account_id"] = json!("codex:a-much-longer-account");
        let mut field = crate::render::Field::new(100, 20, 1967);
        let mut c = field.paint(4000);
        paint(
            &mut c,
            &mut Options::default(),
            &sessions,
            true,
            18,
            61 + 1,
            19,
        );
        let line = text(&c, 3);
        let tag = line.find("⬢ g").map(|b| line[..b].chars().count()).unwrap();
        let counter = line
            .find("(1:01)")
            .map(|b| line[..b].chars().count())
            .expect("counter in the shared duration format, in parentheses");
        assert_eq!(counter, tag + "⬢ g".chars().count() + 1);
        // All or nothing: a narrow screen hides it rather than clipping it.
        let mut field = crate::render::Field::new(30, 20, 1967);
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
        let line = text(&c, 3);
        assert!(!line.contains('(') && !line.contains(':'), "{line}");
    }
    #[test]
    fn counter_falls_back_to_a_verified_local_attachment_start_only() {
        let me = std::process::id();
        let stat = std::fs::read_to_string(format!("/proc/{me}/stat")).unwrap();
        let ticks: u64 = stat
            .rsplit_once(')')
            .unwrap()
            .1
            .split_whitespace()
            .nth(19)
            .unwrap()
            .parse()
            .unwrap();
        let s = json!({"attachment":{"pid":me,"process_start":ticks}});
        let start = attachment_started(&s).expect("live process with matching start");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert!(start <= now && now - start < 3600, "{start} vs {now}");
        // A recycled PID (different start) or missing fields give no counter.
        assert_eq!(
            attachment_started(&json!({"attachment":{"pid":me,"process_start":ticks + 1}})),
            None
        );
        assert_eq!(attachment_started(&json!({"attachment":{"pid":me}})), None);
        let running = json!({"availability":"running","attachment_since":now - 61});
        assert_eq!(hover_detail(&running, now).as_deref(), Some("(1:01)"));
        assert_eq!(
            hover_detail(&json!({"availability":"running","activity":"busy"}), now),
            None
        );
    }
    #[test]
    fn account_and_icon_use_the_light_level_only_on_the_selected_row() {
        let sessions = sessions();
        let mut field = crate::render::Field::new(100, 20, 1967);
        let mut c = field.paint(4000);
        let mut options = Options::default();
        paint(&mut c, &mut options, &sessions, true, 18, 62, 19);
        let theme = c.theme.clone().unwrap();
        let level = |x: usize, y: usize, brightness: i64| {
            theme
                .accent(
                    x,
                    y,
                    &Accent {
                        brightness,
                        bold: false,
                        flash: false,
                        ..Accent::default()
                    },
                )
                .fg
        };
        let at = |c: &Canvas, y: usize, glyph: &str| {
            text(c, y)
                .find(glyph)
                .map(|b| text(c, y)[..b].chars().count())
                .unwrap()
        };
        // Row 3 (busy codex, selected) and row 4 (idle claude, not selected).
        let (sel_icon, sel_name) = (at(&c, 3, "⬢"), at(&c, 3, "gate1"));
        assert_eq!(c.cells[3 * 100 + sel_icon].fg, level(sel_icon, 3, 1000));
        assert_eq!(c.cells[3 * 100 + sel_name].fg, level(sel_name, 3, 1000));
        let (icon, name) = (at(&c, 4, "✻"), at(&c, 4, "main"));
        assert_eq!(c.cells[4 * 100 + icon].fg, level(icon, 4, 600));
        assert_eq!(c.cells[4 * 100 + name].fg, level(name, 4, 600));
        // Moving the cursor swaps the levels.
        options.selected = 1;
        let mut c = field.paint(4000);
        paint(&mut c, &mut options, &sessions, true, 18, 62, 19);
        assert_eq!(c.cells[4 * 100 + icon].fg, level(icon, 4, 1000));
        assert_eq!(c.cells[3 * 100 + sel_name].fg, level(sel_name, 3, 600));
    }
    #[test]
    fn selected_account_and_static_icon_are_bold_until_the_badge_takes_focus() {
        // Row 3: busy codex (animated icon); row 4: idle claude (static icon).
        let sessions = sessions();
        let mut badges = std::collections::BTreeMap::new();
        for id in ["a", "b"] {
            badges.insert(
                id.to_owned(),
                Badge {
                    kind: "operation".into(),
                    id: "op".into(),
                    text: "moving".into(),
                    active: true,
                    queued: false,
                },
            );
        }
        let draw = |selected: usize, focus: bool| {
            let mut c = crate::render::Field::new(100, 20, 1967).paint(4000);
            let mut options = Options {
                selected,
                ..Options::default()
            };
            paint_with(
                &mut c,
                &mut options,
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
        let find = |c: &Canvas, y: usize, s: &str| {
            let line = text(c, y);
            line.find(s).map(|b| line[..b].chars().count()).unwrap()
        };
        let level = |c: &Canvas, x: usize, y: usize, brightness: i64| {
            c.theme
                .as_ref()
                .unwrap()
                .accent(
                    x,
                    y,
                    &Accent {
                        brightness,
                        bold: false,
                        flash: false,
                        ..Accent::default()
                    },
                )
                .fg
        };
        // Static icon row selected: icon and name bold at the light level.
        let c = draw(1, false);
        let (icon, name) = (find(&c, 4, "✻"), find(&c, 4, "main"));
        for x in [icon, name] {
            let cell = &c.cells[4 * 100 + x];
            assert!(cell.bold, "x={x}");
            assert_eq!(cell.fg, level(&c, x, 4, 1000));
        }
        // Badge focus: unbold and the dark level.
        let c = draw(1, true);
        for x in [icon, name] {
            let cell = &c.cells[4 * 100 + x];
            assert!(!cell.bold, "x={x}");
            assert_eq!(cell.fg, level(&c, x, 4, 600));
        }
        // Animated icon: never bolded here, in either state; the name follows the rule.
        for focus in [false, true] {
            let c = draw(0, focus);
            let spin = find(&c, 3, "⬢");
            assert!(!c.cells[3 * 100 + spin].bold);
            let name = find(&c, 3, "gate1");
            assert_eq!(c.cells[3 * 100 + name].bold, !focus);
        }
    }
    #[test]
    fn every_row_has_a_busy_icon_and_dim_provider_account_column() {
        let sessions = sessions();
        let mut field = crate::render::Field::new(100, 20, 1967);
        let mut c = field.paint(4000);
        paint(&mut c, &mut Options::default(), &sessions, true, 18, 62, 19);
        let busy = text(&c, 3);
        let idle = text(&c, 4);
        let saved = text(&c, 6);
        // Busy replaces the provider icon itself; no second glyph, no dot.
        assert!(busy.contains("⬢ gate1") && !busy.contains('⏣'), "{busy}");
        assert!(idle.contains("✻ main") && !idle.contains('●'), "{idle}");
        assert!(saved.contains("⏣ work") && !saved.contains('●'), "{saved}");
        assert!(!saved.contains("inactive"));
        // Account columns start at one x on every row.
        let x = |line: &str, glyph: char| line.chars().position(|g| g == glyph).unwrap();
        assert_eq!(x(&busy, '⬢'), x(&saved, '⏣'));
        assert_eq!(x(&busy, '⬢'), x(&idle, '✻'));
        let tag = &c.cells[6 * 100 + x(&saved, '⏣') + 2];
        assert_eq!(tag.fg, MUTED);
        assert!(!tag.bold);
        assert!(!c.cells[3 * 100 + x(&busy, '⬢')].bold);
        // Hover shows the timer; no "(inactive)".
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
    fn tint_of(c: &Canvas, y: usize, x: usize) -> [u8; 3] {
        c.cells[y * c.width + x].fg
    }
    /// Running rows colour the account name with the icon's own position-based
    /// accent at each cell (light only on the selected row); names stay plain.
    #[test]
    fn running_account_names_use_the_icon_accent_at_their_own_cells_and_names_stay_plain() {
        let sessions = sessions();
        let mut field = crate::render::Field::new(100, 20, 1967);
        for (selected, back, level) in [(1, false, 1000), (0, false, 600), (1, true, 600)] {
            let mut c = field.paint(5000);
            let mut o = Options {
                selected,
                back,
                ..Options::default()
            };
            paint(&mut c, &mut o, &sessions, true, 18, 62, 19);
            let theme = c.theme.clone().unwrap();
            let line = text(&c, 4);
            let icon = line.chars().position(|g| g == '✻').unwrap();
            let accent = Accent {
                brightness: level,
                bold: false,
                flash: false,
                ..Accent::default()
            };
            for x in std::iter::once(icon).chain(icon + 2..icon + 6) {
                assert_eq!(
                    tint_of(&c, 4, x),
                    theme.accent(x, 4, &accent).fg,
                    "x={x} selected={selected} back={back}"
                );
            }
            let name = line.chars().position(|g| g == 'd').unwrap();
            for x in name..name + 4 {
                let fg = tint_of(&c, 4, x);
                assert!(fg == SELECTED || fg == SECONDARY, "x={x} {fg:?}");
            }
            // Saved rows keep the plain account tint.
            let saved = text(&c, 6);
            let at = saved.chars().position(|g| g == '⏣').unwrap() + 2;
            assert!(
                [MUTED, MUTED.map(|n| (u16::from(n) * 60 / 100) as u8)]
                    .contains(&tint_of(&c, 6, at))
            );
        }
    }
    #[test]
    fn row_icon_frames_follow_the_provider_and_chosen_spinner_while_busy() {
        use crate::preferences::{
            AntigravitySpinner as A, ClaudeSpinner as C, CodexSpinner as X, GeminiSpinner as G,
        };
        let busy = |p: &str| json!({"account_id":format!("{p}:a"),"availability":"running","activity":"busy"});
        let idle = |p: &str| json!({"account_id":format!("{p}:a"),"availability":"running","activity":"idle"});
        let d = Spinners::default();
        let with = |claude, codex, gemini| Spinners {
            claude,
            codex,
            gemini,
            antigravity: Default::default(),
        };
        assert_eq!(row_icon(&busy("claude"), 0, d), "·");
        assert_eq!(row_icon(&busy("claude"), 1000, d), "✽");
        let codex: Vec<_> = (0..7)
            .map(|i| row_icon(&busy("codex"), i * 120, d))
            .collect();
        assert_eq!(codex, ["⬡", "⎔", "⏣", "⬢", "⏣", "⎔", "⬡"]);
        let gemini: Vec<_> = (0..7)
            .map(|i| row_icon(&busy("gemini"), i * 120, d))
            .collect();
        assert_eq!(gemini, ["⋅", "✧", "⟡", "✦", "⟡", "✧", "⋅"]);
        assert_eq!(row_icon(&idle("claude"), 1000, d), "✻");
        assert_eq!(row_icon(&idle("gemini"), 80, d), "✦");
        assert_eq!(row_icon(&idle("antigravity"), 80, d), "◬");
        let agy: Vec<_> = (0..9)
            .map(|i| row_icon(&busy("antigravity"), i * 80, d))
            .collect();
        assert_eq!(agy, ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷", "⣾"]);
        let agy_with = |kind, n| -> Vec<&str> {
            (0..n)
                .map(|i| {
                    row_icon(
                        &busy("antigravity"),
                        i * 120,
                        Spinners {
                            antigravity: kind,
                            ..Default::default()
                        },
                    )
                })
                .collect()
        };
        assert_eq!(agy_with(A::Pulse, 6), ["·", "▵", "▴", "◬", "▴", "▵"]);
        assert_eq!(agy_with(A::Tilt, 4), ["◬", "◭", "◬", "◮"]);
        assert_eq!(agy_with(A::Glow, 3), ["◬", "◬", "◬"]);
        assert_eq!(row_icon(&idle("codex"), 100, d), "⏣");
        // Every option selects its own frames.
        let frames = |kind, n| -> Vec<&str> {
            (0..n)
                .map(|i| row_icon(&busy("codex"), i * 120, with(C::Native, kind, G::Twinkle)))
                .collect()
        };
        assert_eq!(frames(X::Rotate, 5), ["⬢", "⬣", "⬢", "⬣", "⬢"]);
        assert_eq!(frames(X::Benzene, 5), ["⌬", "⏣", "⬡", "⏣", "⌬"]);
        assert_eq!(frames(X::BuildUp, 6), ["·", "⬡", "⏣", "⬢", "⏣", "⬡"]);
        assert_eq!(frames(X::Glow, 3), ["⏣", "⏣", "⏣"]);
        // Codex's Native is the Braille spinner (labelled Native/Classic).
        let braille = with(C::Classic, X::Native, G::Native);
        assert_eq!(row_icon(&busy("codex"), 100, braille), "⠙");
        assert_eq!(row_icon(&busy("codex"), 99, braille), "⠋");
        // Gemini's Native is its own 80 ms Braille; Classic is the shared 100 ms one.
        assert_eq!(row_icon(&busy("gemini"), 80, braille), "⠙");
        assert_eq!(row_icon(&busy("gemini"), 79, braille), "⠋");
        let classic = with(C::Classic, X::Native, G::Classic);
        assert_eq!(row_icon(&busy("gemini"), 100, classic), "⠙");
        assert_eq!(row_icon(&busy("gemini"), 99, classic), "⠋");
        let agy_classic = Spinners {
            antigravity: A::Classic,
            ..d
        };
        assert_eq!(row_icon(&busy("antigravity"), 100, agy_classic), "⠙");
        assert_eq!(row_icon(&busy("antigravity"), 99, agy_classic), "⠋");
        // Static is the plain provider symbol while busy, whatever the time.
        let stat = Spinners {
            claude: C::Static,
            codex: X::Static,
            gemini: G::Static,
            antigravity: A::Static,
        };
        for (p, glyph) in [
            ("claude", "✻"),
            ("codex", "⏣"),
            ("gemini", "✦"),
            ("antigravity", "◬"),
        ] {
            for now in [0, 80, 333, 1000] {
                assert_eq!(row_icon(&busy(p), now, stat), glyph, "{p} {now}");
            }
            assert!(!row_glows(&busy(p), stat));
        }
        let gem = |kind, n| -> Vec<&str> {
            (0..n)
                .map(|i| {
                    row_icon(
                        &busy("gemini"),
                        i * 120,
                        with(C::Native, X::FillPulse, kind),
                    )
                })
                .collect()
        };
        assert_eq!(gem(G::SpinFill, 4), ["⬖", "⬘", "⬗", "⬙"]);
        assert_eq!(gem(G::Morph, 4), ["✦", "❖", "◆", "❖"]);
        assert_eq!(gem(G::ConcaveBlink, 4), ["✦", "⟡", "✧", "⟡"]);
        assert_eq!(gem(G::Glow, 3), ["✦", "✦", "✦"]);
        let claude =
            |kind, now| row_icon(&busy("claude"), now, with(kind, X::FillPulse, G::Twinkle));
        assert_eq!(claude(C::Classic, 100), "⠙");
        assert_eq!(claude(C::Classic, 99), "⠋");
        assert_eq!(claude(C::Glow, 0), "✻");
        assert_eq!(claude(C::Glow, 777), "✻");
        // Only busy Glow rows get the colour pulse.
        let mut glow = with(C::Glow, X::Glow, G::Glow);
        glow.antigravity = A::Glow;
        for p in ["claude", "codex", "gemini", "antigravity"] {
            assert!(row_glows(&busy(p), glow) && !row_glows(&idle(p), glow));
            assert!(!row_glows(&busy(p), d));
        }
    }
    /// A busy Codex row with the given spinner: x of its icon, the icon cell and
    /// the theme. `selected` picks whether that row or the idle one below has focus.
    fn busy_icon_cell(
        kind: CodexSpinner,
        selected: bool,
        now: u64,
    ) -> (usize, Cell, crate::render::Theme) {
        let sessions = vec![
            json!({"id":"a","name":"liq-glass-lead","account_id":"codex:gate1","availability":"running","activity":"busy"}),
            json!({"id":"b","name":"docs","account_id":"codex:main","availability":"running","activity":"idle"}),
        ];
        let mut field = crate::render::Field::new(100, 20, 1967);
        let mut c = field.paint(now);
        let mut o = Options {
            selected: if selected { 0 } else { 1 },
            ..Options::default()
        };
        let extras = Extras {
            spinners: Spinners {
                codex: kind,
                ..Spinners::default()
            },
            ..Extras::default()
        };
        paint_with(&mut c, &mut o, &sessions, true, 18, 62, 19, extras);
        let x = text(&c, 3).chars().position(|g| g == '⏣').unwrap();
        (
            x,
            c.cells[3 * c.width + x].clone(),
            c.theme.clone().unwrap(),
        )
    }
    fn hue(theme: &crate::render::Theme, x: usize, level: i64) -> [u8; 3] {
        theme
            .accent(
                x,
                3,
                &Accent {
                    brightness: level,
                    flash: false,
                    ..Accent::default()
                },
            )
            .fg
    }
    #[test]
    fn static_spinner_wears_the_normal_hue_selected_and_the_darker_hue_otherwise() {
        use crate::render::DIM_LEVEL;
        for now in [4000, 5000, 6123, 9999] {
            for (selected, level) in [(true, 1000), (false, DIM_LEVEL)] {
                let (x, cell, theme) = busy_icon_cell(CodexSpinner::Static, selected, now);
                assert_eq!(cell.fg, hue(&theme, x, level), "now={now} sel={selected}");
                assert!(!cell.bold, "animated rows are never bold");
            }
        }
    }
    #[test]
    fn glow_hue_stays_between_its_two_levels_at_the_arrow_pulse_period() {
        use crate::render::{ARROW_PULSE_MS, DIM_LEVEL, GLOW_FLOOR_LEVEL};
        let (floor, dim) = (std::hint::black_box(GLOW_FLOOR_LEVEL), DIM_LEVEL);
        assert!(floor < dim && floor > 0);
        for (selected, high, low) in [
            (true, 1000, DIM_LEVEL),
            (false, DIM_LEVEL, GLOW_FLOOR_LEVEL),
        ] {
            let mut seen = std::collections::BTreeSet::new();
            for base in [8000, 8000 + ARROW_PULSE_MS * 3] {
                for phase in (0..ARROW_PULSE_MS).step_by(125) {
                    let now = base + phase;
                    let (x, cell, theme) = busy_icon_cell(CodexSpinner::Glow, selected, now);
                    let (hi, lo) = (hue(&theme, x, high), hue(&theme, x, low));
                    for ch in 0..3 {
                        let (a, b) = (hi[ch].min(lo[ch]), hi[ch].max(lo[ch]));
                        assert!((a..=b).contains(&cell.fg[ch]), "now={now} sel={selected}");
                    }
                    assert!(!cell.bold, "animated rows are never bold");
                    match phase {
                        0 => assert_eq!(cell.fg, hi, "start of the period"),
                        500 => assert_eq!(cell.fg, lo, "mid-period"),
                        _ => {}
                    }
                    seen.insert(cell.fg);
                }
            }
            assert!(seen.len() >= 4, "Glow must change colour, sel={selected}");
        }
    }
    #[test]
    fn every_spinner_glyph_is_one_cell_wide() {
        use unicode_width::UnicodeWidthStr;
        let mut glyphs: Vec<&str> = [
            &CODEX_FILL_PULSE_FRAMES[..],
            &CODEX_ROTATE_FRAMES,
            &CODEX_BENZENE_FRAMES,
            &CODEX_BUILD_UP_FRAMES,
            &GEMINI_TWINKLE_FRAMES,
            &GEMINI_SPIN_FILL_FRAMES,
            &GEMINI_MORPH_FRAMES,
            &GEMINI_CONCAVE_BLINK_FRAMES,
            &BRAILLE_FRAMES,
            &ANTIGRAVITY_NATIVE_FRAMES,
            &ANTIGRAVITY_PULSE_FRAMES,
            &ANTIGRAVITY_TILT_FRAMES,
            &["·", "✢", "*", "✶", "✻", "✽"],
        ]
        .concat();
        glyphs.extend([
            CODEX_GLOW_GLYPH,
            GEMINI_GLOW_GLYPH,
            ANTIGRAVITY_GLOW_GLYPH,
            CLAUDE_GLOW_GLYPH,
        ]);
        for g in glyphs {
            assert_eq!(UnicodeWidthStr::width(g), 1, "{g}");
            if std::env::var_os("SPINNER_WIDTH_REPORT").is_some() {
                eprintln!("{g} cjk={}", unicode_width::UnicodeWidthStr::width_cjk(g));
            }
        }
    }
    #[test]
    fn account_column_sits_one_space_after_the_longest_name() {
        let sessions = vec![
            json!({"id":"a","name":"abcdefgh","account_id":"codex:p","availability":"saved"}),
            json!({"id":"b","name":"abc","account_id":"codex:p","availability":"saved"}),
        ];
        for width in [60usize, 100, 140] {
            let mut field = crate::render::Field::new(width, 20, 1967);
            let mut c = field.paint(100);
            paint(&mut c, &mut Options::default(), &sessions, true, 18, 0, 19);
            let long = text(&c, 3);
            let short = text(&c, 4);
            let name_end = long.chars().position(|g| g == 'a').unwrap() + 8;
            let col = long.chars().position(|g| g == '⏣').unwrap();
            assert_eq!(col, name_end + 1, "{long}");
            assert_eq!(short.chars().position(|g| g == '⏣').unwrap(), col);
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
    fn only_the_selected_long_name_scrolls_and_others_stay_ellipsized() {
        // Only the selected row scrolls.
        let sessions = vec![
            json!({"id":"a","name":"abcdefghijklmnopqrstuvwxyz0123456789","account_id":"codex:p","availability":"saved"}),
            json!({"id":"b","name":"abcdefghijklmnopqrstuvwxyz0123456789","account_id":"codex:p","availability":"saved"}),
        ];
        let mut field = crate::render::Field::new(60, 20, 1967);
        // Four steps into the cycle: the selected name has moved four graphemes.
        let now = crate::widgets::MARQUEE_PAUSE_MS + crate::widgets::MARQUEE_STEP_MS * 3;
        let mut c = field.paint(now);
        paint(
            &mut c,
            &mut Options::default(),
            &sessions,
            true,
            16,
            100,
            19,
        );
        let (selected, other) = (text(&c, 3), text(&c, 4));
        assert!(selected.contains("efghij"), "{selected}");
        assert!(!selected.contains('…'), "{selected}");
        assert!(other.contains("abcdef"), "{other}");
        assert!(
            other.contains('…'),
            "unselected keeps the ellipsis: {other}"
        );
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
                queued: false,
            },
        );
        badges.insert(
            "c".to_owned(),
            Badge {
                kind: "operation".into(),
                id: "op2".into(),
                text: "move failed".into(),
                active: false,
                queued: false,
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
        assert!(text(&c, 3).contains("◜") && text(&c, 3).contains("moving to work"));
        assert!(text(&c, 6).contains("✗ move failed"));
        assert!(text(&draw(150, false), 3).contains("◝"), "icon animates");
        let x = text(&c, 6).chars().position(|g| g == '✗').unwrap();
        assert_eq!(c.cells[6 * 100 + x].fg, FAILURE);
        let focused = draw(0, true);
        assert!(text(&focused, 3).contains("moving to work ◀"));
        let m = text(&focused, 3).chars().position(|g| g == 'm').unwrap();
        assert!(focused.cells[3 * 100 + m].bold);
    }
    #[test]
    fn badge_focus_steps_every_other_element_back_and_restores_on_leaving() {
        let sessions = sessions();
        let mut badges = std::collections::BTreeMap::new();
        badges.insert(
            "a".to_owned(),
            Badge {
                kind: "operation".into(),
                id: "op".into(),
                text: "moving to work".into(),
                active: true,
                queued: false,
            },
        );
        let draw = |focus: bool| {
            let mut c = crate::render::Field::new(100, 20, 1967).paint(5000);
            let mut options = Options::default();
            paint_with(
                &mut c,
                &mut options,
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
            let left = Options::left(100, &rows(&sessions, true));
            options.paint_back(&mut c, left, 3, "Back");
            c
        };
        let dim = |c: &Canvas, x: usize, y: usize, glyph_bold: bool| {
            c.theme
                .as_ref()
                .unwrap()
                .accent(
                    x,
                    y,
                    &Accent {
                        brightness: 600,
                        bold: glyph_bold,
                        ..Accent::default()
                    },
                )
                .fg
        };
        let find = |c: &Canvas, y: usize, glyph: &str| {
            (0..c.width).find(|&x| c.cells[y * c.width + x].glyph == glyph)
        };
        let focused = draw(true);
        let plain = draw(false);
        // Names: the active one lighter and unbolded, the others darker.
        let name =
            |c: &Canvas, y: usize, g: &str| c.cells[y * c.width + find(c, y, g).unwrap()].clone();
        let active = name(&focused, 3, "l");
        assert_eq!((active.fg, active.bold), (SECONDARY, false));
        assert_eq!(name(&focused, 4, "d").fg, MUTED);
        assert_eq!(name(&focused, 6, "o").fg, MUTED);
        // Bullets are the dim hue variant, including the active row's.
        for (y, selected) in [(3usize, true), (4, false), (6, false)] {
            let x = find(&focused, y, if selected { "●" } else { "○" }).unwrap();
            assert_eq!(
                focused.cells[y * 100 + x].fg,
                dim(&focused, x, y, selected),
                "bullet y={y}"
            );
        }
        // Icons are dim too; the idle row's account value goes darker grey.
        let icon = find(&focused, 4, "✻").unwrap();
        let want = focused
            .theme
            .as_ref()
            .unwrap()
            .accent(
                icon,
                4,
                &Accent {
                    brightness: 600,
                    flash: false,
                    ..Accent::default()
                },
            )
            .fg;
        assert_eq!(focused.cells[4 * 100 + icon].fg, want);
        assert_eq!(focused.cells[4 * 100 + icon + 2].fg, MUTED);
        // The back arrow is darker; the badge keeps its active styling.
        let back = find(&focused, 3, "◀").unwrap();
        assert_eq!(focused.cells[3 * 100 + back].fg, MUTED);
        let badge = find(&focused, 3, "◀").and_then(|_| text(&focused, 3).find("moving"));
        assert!(badge.is_some() && text(&focused, 3).contains("moving to work ◀"));
        // Leaving the mode restores the normal styling.
        let active = name(&plain, 3, "l");
        assert_eq!((active.fg, active.bold), (SELECTED, true));
        assert_eq!(name(&plain, 4, "d").fg, SECONDARY);
        let x = find(&plain, 4, "○").unwrap();
        assert_ne!(plain.cells[4 * 100 + x].fg, dim(&plain, x, 4, false));
        let back = find(&plain, 3, "◀").unwrap();
        assert_eq!(plain.cells[3 * 100 + back].fg, SECONDARY);
    }
}
