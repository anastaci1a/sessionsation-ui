//! Public-contract management reducer. Rendering and selection perform no I/O.
use crate::{
    client::Account,
    dialog::{self, Dialog, Field, Palette, Rule},
    render::Canvas,
    usage::{AccountsLayout, Usage, account_title},
    widgets::{NAME_MAX_CHARS, Options, Row, hint_lines},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};
mod queue;
pub use queue::RECENT_LIMIT;
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Account(String),
    Sessions,
    Archives,
    Session(String),
    Spaces(String),
    Operations,
    Operation(String),
    Launch(&'static str, String),
    /// Operations queue popup over the page it was opened from.
    Queue,
}
#[derive(Clone, Debug)]
pub enum Intent {
    Api(u64, Vec<String>),
    /// A request whose stdin JSON carries a secret; the secret is never logged.
    ApiInput(u64, Vec<String>, Secret),
    New,
    Settings,
    Resume(Value),
    Switch(Value),
    Focus(Value),
    /// Run a backend login plan in a HERDR pane; the plan is never logged.
    LaunchLogin(Vec<String>),
    /// Read the visible text of the open login pane for this account key.
    ReadLoginPane(String),
    Exit,
}
/// Stdin JSON for a backend request that carries a secret. It never appears in
/// arguments, `Debug` output or traces.
#[derive(Clone, PartialEq)]
pub struct Secret(pub Value);
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}
/// One Gemini sign-in option as the backend lists it in `auth_modes`.
#[derive(Clone, Debug)]
struct AuthChoice {
    mode: String,
    label: String,
    available: bool,
    reason: Option<String>,
}
/// Where a session is placed: an existing Space by id, or a Space to create by label.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SpaceTarget {
    Existing(String),
    New(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Add,
    Start,
}
#[derive(Clone, Debug)]
enum Purpose {
    State,
    Archives,
    Operations,
    Resume(String),
    Spaces(Value),
    /// Spaces on the UI's own HERDR server for a session that is not in HERDR.
    Placement(Value, Place),
    AddToHerdr(Value),
    Start(Value),
    Relocation(Vec<String>),
    Launches(&'static str),
    Launch(&'static str, String),
    Remove(String),
    Delete(String),
    DeleteAvailability(String),
    LiveSteering(String),
    BypassWarning(String),
    Limits(String),
    Mutation,
    /// Gemini API-key save and validation (secret stdin, up to ~15 s).
    AuthKey(String),
    LoginPoll(String),
    Observe(String),
    ActionCheck(Value, crate::session_list::Action),
    /// Home move-mode save; holds the order the backend last reported.
    Reorder(Vec<Account>),
}
#[derive(Clone, Debug)]
enum Form {
    None,
    Loading,
    AddProvider(Vec<&'static str>),
    Add(String),
    Login(String),
    /// Gemini sign-in choice for an account, in backend order.
    GeminiAuth(String, Vec<AuthChoice>),
    /// Masked API-key entry for a Gemini account.
    GeminiKey(String),
    LoginWait(String),
    /// A login already running in its pane: its sign-in link, read from the screen.
    LoginLink(String),
    AccountRename(String),
    /// Rename popup of a 1.30+ backend: key, the current name, name differs from folder.
    AccountRenameBackend(String, String, bool),
    Limits(String),
    Remove(Value),
    Delete(Value),
    ResumeWarning(Value),
    ActionWarning(Value, crate::session_list::Action),
    OperationApproval(String, u64),
    Rename(Value),
    Relocation(Value, bool),
    SessionMutation(Value, &'static str),
    SessionDetails(String),
    Placement(Value, SpaceTarget, Place),
    /// Naming the Space to create; `None` is a move between Spaces.
    NewSpace(Value, Option<Place>),
    Added(Value),
    Confirm(Vec<String>),
    Progress,
    Operation(String),
    /// A queued entry: Cancel or Close.
    Queued(String),
    /// A queue entry that failed to start: Dismiss or Close.
    QueuedFailed(String),
}
#[derive(Clone)]
enum Focus {
    Account(String),
    Session(String),
    Operation(String, String),
    Label(String),
}
struct Location {
    page: Page,
    options: Options,
    focus: Option<Focus>,
    usage_focus: bool,
}
pub struct Manager {
    pub page: Page,
    pub options: Options,
    pub accounts: Vec<Account>,
    pub sessions: Vec<Value>,
    pub archives: Vec<Value>,
    pub operations: Vec<Value>,
    pub launches: BTreeMap<&'static str, Vec<Value>>,
    pub capabilities: Value,
    /// Busy spinner choices, synced from the preferences before painting.
    pub spinners: crate::preferences::Spinners,
    pub dialog: Option<Dialog>,
    pub palette: Palette,
    pub note: String,
    /// First account drawn on the (scrollable) home list.
    pub account_scroll: usize,
    /// Order before an unsaved move; present only while move mode is active.
    moving: Option<Vec<Account>>,
    pub usage_focus: bool,
    /// Home column the cursor returns to on rows that have it (0 = names, 1 =
    /// usage), kept while it crosses single-column rows.
    home_column: usize,
    pub inventory_known: bool,
    pub sessions_known: bool,
    /// True while the session list comes from the on-disk cache or a stale snapshot:
    /// rows are shown but no session action is offered until fresh state arrives.
    pub cached_snapshot: bool,
    /// A finished full state result to persist (taken by the event loop).
    pub cache_out: Option<Value>,
    /// Finished operations whose badges the user dismissed, by operation ID.
    pub dismissed: std::collections::BTreeSet<String>,
    /// Changed dismissals to persist (taken by the event loop).
    pub dismissed_out: Option<Vec<String>>,
    /// False while the popup is not visible: background polling pauses.
    pub visible: bool,
    ticks_since_state: u32,
    ticks_since_full: u32,
    cached_tokens: std::collections::BTreeSet<u64>,
    last_full_token: u64,
    form: Form,
    sequence: u64,
    pending: BTreeMap<u64, Purpose>,
    readiness: BTreeMap<String, Value>,
    space_choices: Value,
    space_session: Value,
    deletion_previews: BTreeMap<String, Value>,
    /// Last live-steering state read for each Claude account (`api account live-steering`).
    steering: BTreeMap<String, Value>,
    /// Why live steering cannot be changed for an account (failed read, symlinked settings).
    steering_unavailable: BTreeMap<String, String>,
    /// The Live steering selector is grabbed: the choice pending Enter (true = On).
    steering_pick: Option<bool>,
    /// Last bypass-warning state read for each Claude account (`api account bypass-warning`).
    bypass: BTreeMap<String, Value>,
    /// Why the bypass warning cannot be changed for an account.
    bypass_unavailable: BTreeMap<String, String>,
    /// The Bypass warning selector is grabbed: the choice pending Enter (true = Skip).
    bypass_pick: Option<bool>,
    /// The Operations filter is grabbed: the value to restore on Esc.
    filter_grab: Option<OperationFilter>,
    pub active_operation: Option<String>,
    foreground: Option<u64>,
    details_open: bool,
    session_return: Option<String>,
    observe_after: Option<std::time::Instant>,
    history: Vec<Location>,
    popup_size: Option<crate::popup::Size>,
    login_wait: Option<LoginWait>,
    login_link: Option<LoginLink>,
    mutation: Vec<String>,
    /// The UI's own HERDR server: {"session", "socket"}.
    pub host: Value,
    /// The one focused Space of that server, when unambiguous.
    pub focused_space: Option<String>,
    place: Option<Place>,
    merge_target: Option<(String, u8)>,
    /// Focus is on the selected session row's operation badge.
    pub badge_focus: bool,
    /// Operations page filter.
    pub operation_filter: OperationFilter,
    /// First Esc on Home arms closing; a second within one second closes.
    close_armed: Option<std::time::Instant>,
    /// Start of the running "Press Esc again" band animation.
    close_band: Option<std::time::Instant>,
    /// Terminal background beneath cells without one (for band fading).
    pub band_base: [u8; 3],
    /// HERDR Space names by ID, for operation badges.
    pub space_names: BTreeMap<String, String>,
    ticks_since_operations: u32,
    /// Snapshot of the UI operation queue (`operation-queue.json`).
    pub queue: Vec<crate::op_queue::Entry>,
    /// Queue edits to persist (taken by the event loop).
    pub queue_out: Vec<crate::op_queue::Command>,
    /// Direct mutation tokens and the queue entries registered for them.
    direct: BTreeMap<u64, String>,
    /// Paint time in milliseconds, for operation spinners.
    pub anim_ms: u64,
    /// Selected Operations popup item, kept through live updates.
    queue_focus: Option<String>,
}
/// Operations page filter, an Up/Down picker on its selector row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OperationFilter {
    #[default]
    All,
    InProgress,
    Failed,
    Completed,
}
impl OperationFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::InProgress => "In progress",
            Self::Failed => "Failed",
            Self::Completed => "Completed",
        }
    }
    pub const ALL: [OperationFilter; 4] = [
        OperationFilter::All,
        OperationFilter::InProgress,
        OperationFilter::Failed,
        OperationFilter::Completed,
    ];
}
/// Automatic provider login in progress: the backend plan runs in a HERDR pane
/// while the account is polled until it no longer needs authentication.
pub struct LoginWait {
    key: String,
    argv: Vec<String>,
    started: std::time::Instant,
    last_poll: Option<std::time::Instant>,
}
/// A login already running in its HERDR pane, whose screen is read for a sign-in link.
struct LoginLink {
    key: String,
    /// A login pane has been found at least once.
    seen: bool,
    in_flight: bool,
    last: Option<std::time::Instant>,
    screen: Option<String>,
}
const LOGIN_POLL: std::time::Duration = std::time::Duration::from_secs(2);
const LOGIN_LIMIT: std::time::Duration = std::time::Duration::from_secs(600);
const CLOSE_WINDOW: std::time::Duration = crate::close_band::CLOSE_WINDOW;
/// The operation counts take the status lane only when the list keeps at least
/// four rows below the title rows (top 3 + 4 rows + the lane's 2).
const SUMMARY_MIN_CONTROLS: usize = 9;
impl Default for Manager {
    fn default() -> Self {
        Self {
            page: Page::Home,
            options: Options::default(),
            accounts: vec![],
            sessions: vec![],
            archives: vec![],
            operations: vec![],
            launches: BTreeMap::new(),
            capabilities: json!({}),
            spinners: Default::default(),
            dialog: None,
            palette: Palette::default(),
            note: String::new(),
            account_scroll: 0,
            moving: None,
            usage_focus: false,
            home_column: 0,
            inventory_known: false,
            sessions_known: false,
            cached_snapshot: false,
            cache_out: None,
            dismissed: Default::default(),
            dismissed_out: None,
            visible: true,
            ticks_since_state: 0,
            ticks_since_full: 0,
            cached_tokens: Default::default(),
            last_full_token: 0,
            form: Form::None,
            sequence: 0,
            pending: BTreeMap::new(),
            readiness: BTreeMap::new(),
            space_choices: Value::Null,
            space_session: Value::Null,
            deletion_previews: BTreeMap::new(),
            steering: BTreeMap::new(),
            steering_unavailable: BTreeMap::new(),
            steering_pick: None,
            bypass: BTreeMap::new(),
            bypass_unavailable: BTreeMap::new(),
            bypass_pick: None,
            filter_grab: None,
            active_operation: None,
            foreground: None,
            details_open: false,
            session_return: None,
            observe_after: None,
            history: vec![],
            popup_size: None,
            login_wait: None,
            login_link: None,
            mutation: vec![],
            host: Value::Null,
            focused_space: None,
            place: None,
            merge_target: None,
            badge_focus: false,
            operation_filter: OperationFilter::All,
            close_armed: None,
            close_band: None,
            band_base: [16; 3],
            space_names: BTreeMap::new(),
            ticks_since_operations: 0,
            queue: vec![],
            queue_out: vec![],
            direct: BTreeMap::new(),
            anim_ms: 0,
            queue_focus: None,
        }
    }
}
fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).into()).collect()
}
/// A generation mismatch, raw (`STALE_TARGET`, `SESSION_STALE…`) or described.
fn stale_error(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("STALE")
        || text.starts_with("SESSION_STALE")
        || text == crate::messages::describe("STALE_TARGET")
}
/// `SESSION_MERGED: <surviving id>`: the entry was merged into another session.
fn merged_survivor(text: &str) -> Option<String> {
    let rest = text.trim_start().strip_prefix("SESSION_MERGED: ")?;
    let tokens = || {
        rest.split_whitespace()
            .map(|t| t.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')))
            .filter(|t| !t.is_empty())
    };
    tokens()
        .find(|t| uuid::Uuid::parse_str(t).is_ok())
        .or_else(|| tokens().next())
        .map(str::to_owned)
}
fn id(v: &Value) -> &str {
    v["id"].as_str().unwrap_or("")
}
fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}
/// "5m ago" style age; future or missing observations are "unknown".
fn relative(now: i64, then: Option<i64>) -> String {
    let Some(then) = then else {
        return "unknown".into();
    };
    // Observations are Unix seconds; tolerate a millisecond producer.
    let then = if then > 100_000_000_000 {
        then / 1000
    } else {
        then
    };
    let age = now - then;
    match age {
        i64::MIN..=-1 => "unknown".into(),
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", age / 60),
        3600..=86399 => format!("{}h ago", age / 3600),
        _ => format!("{}d ago", age / 86400),
    }
}
/// RFC 3339 timestamp (`2026-01-01T09:00:00Z`, optional fraction and offset) in Unix seconds.
fn parse_time(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    let num = |r: std::ops::Range<usize>| -> Option<i64> { text.get(r)?.parse().ok() };
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b' ') {
        return None;
    }
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hh, mm, ss) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let mut rest = &text[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        rest = fraction.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let offset = match rest {
        "" | "Z" | "z" => 0,
        _ => {
            let sign = if rest.starts_with('-') { -1 } else { 1 };
            let h: i64 = rest.get(1..3)?.parse().ok()?;
            let m: i64 = rest.get(4..6)?.parse().ok()?;
            sign * (h * 3600 + m * 60)
        }
    };
    // Days from civil (proleptic Gregorian).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss - offset)
}
/// When an inactive session was last opened, in Unix seconds: the backend's
/// `last_accessed` (Codex), else the conversation's creation time. Never invented.
fn last_opened(s: &Value) -> Option<i64> {
    let secs = |t: i64| if t > 100_000_000_000 { t / 1000 } else { t };
    s["last_accessed"].as_i64().map(secs).or_else(|| {
        let c = &s["conversation_created_at"];
        c.as_i64()
            .map(secs)
            .or_else(|| c.as_str().and_then(parse_time))
    })
}
/// Running sessions first (busy, then idle, recent access); inactive ones follow,
/// most recently opened first with missing times last, then by name and id.
fn sort_sessions(sessions: &mut [Value]) {
    let rank = |s: &Value| {
        if s["availability"] == "running" {
            if s["activity"] == "busy" { 0 } else { 1 }
        } else {
            2
        }
    };
    sessions.sort_by(|a, b| {
        let order = rank(a).cmp(&rank(b));
        if rank(a) == 2 && rank(b) == 2 {
            return order
                .then_with(|| last_opened(b).cmp(&last_opened(a)))
                .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
                .then_with(|| id(a).cmp(id(b)));
        }
        order
            .then_with(|| {
                b["last_accessed"]
                    .as_i64()
                    .cmp(&a["last_accessed"].as_i64())
            })
            .then_with(|| id(a).cmp(id(b)))
    });
}
/// Human byte size: `24 KB`, `1.2 MB`.
fn size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{} KB", bytes / 1024),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.),
    }
}
/// Usage details body. Missing windows and unknown state are stated, never 0%.
fn usage_lines(a: &Account, now: i64) -> Vec<String> {
    let usage = Usage::account_at(a, now);
    let mut facts: Vec<(&str, String)> = vec![
        (
            "Account",
            format!(
                "{} {}",
                crate::usage::provider_symbol(a.provider()),
                account_title(a)
            ),
        ),
        (
            "Provider",
            crate::client::provider_label(a.provider()).to_owned(),
        ),
        ("Status", usage.status.clone()),
    ];
    // Gemini sign-in mode and key presence exactly as the backend publishes them.
    if a.provider() == "gemini"
        && let Some(mode) = a.details.get("auth_mode").and_then(Value::as_str)
    {
        let api_key = mode == "api_key";
        facts.push((
            "Auth",
            if api_key {
                "API key".into()
            } else {
                "Google".into()
            },
        ));
        if api_key && let Some(present) = a.details.get("api_key_present").and_then(Value::as_bool)
        {
            facts.push(("API key", if present { "saved" } else { "missing" }.into()));
        }
    }
    // The account home exactly as the backend publishes it; never derived here.
    if let Some(home) = a
        .details
        .get("config_home")
        .or_else(|| a.details.get("home"))
        .and_then(Value::as_str)
        .map(clean)
        .filter(|h| !h.is_empty())
    {
        facts.insert(2, ("Directory", home));
    }
    let state = crate::usage::usage_state(a).map(clean);
    let source = a.usage["source"]
        .as_str()
        .or_else(|| a.details.get("usage_source").and_then(Value::as_str))
        .map(clean);
    let joined: Vec<String> = state.into_iter().chain(source).collect();
    if !joined.is_empty() {
        facts.push(("Usage", joined.join(" · ")));
    }
    if let Some(semantics) = a.usage["semantics"].as_str() {
        facts.push(("Semantics", clean(semantics)));
    }
    if let Some(then) = crate::usage::observed_at(a, now) {
        facts.push(("Observed", relative(now, Some(then))));
    }
    // Gemini and Antigravity have no usage windows to list, not even as unsupported.
    let windows: &[(&str, &str)] = if matches!(a.provider(), "gemini" | "antigravity") {
        &[]
    } else {
        &[("short", "5h window"), ("weekly", "Weekly")]
    };
    for &(window, label) in windows {
        let w = &a.usage[window];
        let used = w["used_percent"].as_f64().filter(|n| n.is_finite());
        facts.push((
            label,
            match (w.is_object(), used) {
                (true, Some(used)) => format!(
                    "{:.0}% left · {used:.0}% used · resets {}",
                    100. - used,
                    w["resets_at"]
                        .as_i64()
                        .map_or("at an unknown time".into(), crate::status::local_time)
                ),
                (_, None) if crate::usage::usage_state(a) == Some("unsupported") => {
                    "usage unsupported".into()
                }
                (true, None) => "usage unknown".into(),
                (false, _) => "unavailable".into(),
            },
        ));
    }
    let width = facts.iter().map(|(l, _)| l.len() + 1).max().unwrap_or(0);
    let mut body: Vec<String> = facts
        .into_iter()
        .map(|(label, value)| format!("{:<width$}  {value}", format!("{label}:")))
        .collect();
    body.push(String::new());
    let limits: Vec<String> = usage
        .windows
        .iter()
        .filter_map(|w| w.limit.map(|l| format!("{l:.0}% left {}", w.label)))
        .collect();
    if !limits.is_empty() {
        body.push(format!(
            "┿ marks the saved switch threshold ({})",
            limits.join(" · ")
        ));
    }
    body.push("Identifiers".into());
    body.push(format!("  {:<w$}  {}", "Account ID", a.id, w = width - 2));
    if let Some(name) = a
        .details
        .get("display_name")
        .and_then(Value::as_str)
        .map(clean)
        .filter(|n| !n.is_empty())
    {
        body.push(format!("  {:<w$}  {name}", "Display name", w = width - 2));
    }
    body
}
/// Printable, copy-pasteable command for a backend exec plan. Control characters
/// are removed so a plan cannot repaint the terminal.
fn login_command(argv: &Value) -> String {
    argv.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|arg| {
            let arg: String = arg.chars().filter(|c| !c.is_control()).collect();
            if !arg.is_empty()
                && arg
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_./:=@%+,-".contains(&c))
            {
                arg
            } else {
                format!("'{}'", arg.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
/// A launchable exec plan: a non-empty array of non-empty strings without NUL.
fn login_plan(argv: &Value) -> Option<Vec<String>> {
    let argv = argv
        .as_array()?
        .iter()
        .map(|a| {
            a.as_str()
                .filter(|a| !a.is_empty() && !a.contains('\0'))
                .map(str::to_owned)
        })
        .collect::<Option<Vec<_>>>()?;
    (!argv.is_empty()).then_some(argv)
}
/// Backend-provided reason/semantics text (a string or an array of strings) from the
/// first of these fields that is present, with control characters removed.
fn backend_text(value: &Value) -> Vec<String> {
    for key in ["semantics", "reason", "note"] {
        let lines: Vec<String> = match &value[key] {
            Value::String(text) => vec![clean(text)],
            Value::Array(items) => items.iter().filter_map(Value::as_str).map(clean).collect(),
            _ => vec![],
        };
        let lines: Vec<String> = lines.into_iter().filter(|l| !l.trim().is_empty()).collect();
        if !lines.is_empty() {
            return lines;
        }
    }
    // Account deletion previews describe their effects with structured fields.
    let mut derived = vec![];
    if value["home_disposition"] == "deleted" {
        derived.push("The managed account home is removed.".to_owned());
    }
    if value["preserves_history"] == true {
        derived.push("Conversation history is preserved.".to_owned());
    }
    if value["adopted_config_home"].is_string() {
        derived.push("The adopted config directory is left in place.".to_owned());
    }
    if !derived.is_empty() {
        return derived;
    }
    vec![]
}
/// The folder name differs from the display name (contract 1.30+).
fn name_mismatch(a: &Account) -> bool {
    a.details.get("name_mismatch").is_some_and(|v| v == true)
}
/// Short hover reason when the backend rename is unavailable for this account.
fn rename_blocked(a: &Account) -> Option<String> {
    if let Some(b) = a.details.get("backend_rename").filter(|b| b.is_object()) {
        if b["available"] != false {
            return None;
        }
        let code = b["reason"].as_str().unwrap_or("");
        return Some(match code {
            "ACCOUNT_SESSIONS_RUNNING" => "sessions running".to_owned(),
            "" => "unavailable".to_owned(),
            other => clean(
                &other
                    .trim_start_matches("ACCOUNT_")
                    .to_lowercase()
                    .replace('_', " "),
            ),
        });
    }
    a.details
        .get("running_sessions")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .map(|_| "sessions running".to_owned())
}
fn account_label(id: &str) -> String {
    id.replacen(':', "-", 1)
}
fn operation_status(record: &Value) -> &str {
    match record["phase"].as_str().unwrap_or("unknown") {
        "complete" => "complete",
        "failed" => "failed",
        "cancelled" => "cancelled",
        "recovered" => "recovered",
        "queued" => "queued",
        "unknown" | "uncertain" => "checking",
        p if p.starts_with("awaiting_") => "confirmation",
        _ => "running",
    }
}
fn launch_status(record: &Value) -> &str {
    match record["phase"].as_str().unwrap_or("unknown") {
        "running" => "complete",
        phase => phase,
    }
}
/// A finished operation's dialog: Dismiss (Undismiss once dismissed), then Close,
/// which keeps the focus. A failed operation not yet dismissed (`retry` is its
/// [`retry_state`]) offers Dismiss, Retry, Close with Dismiss focused; Retry is
/// disabled where the backend says the operation cannot be recovered. Repeated
/// polls leave a finished dialog and its focus alone.
fn finish_operation(d: &mut Dialog, dismissed: bool, retry: Option<bool>) {
    let label = if dismissed { "Undismiss" } else { "Dismiss" };
    if let Some(b) = d
        .buttons
        .iter_mut()
        .find(|b| b.label == "Dismiss" || b.label == "Undismiss")
    {
        b.label = label.into();
        return;
    }
    d.finish();
    let slots = d.selected;
    match retry.filter(|_| !dismissed) {
        Some(can) => {
            d.buttons.insert(0, Row::yes(label));
            d.buttons.insert(1, Row::gate("Retry", can));
            d.selected = slots;
        }
        None => {
            d.buttons.insert(0, Row::yes(label));
            d.selected += 1;
        }
    }
}
/// For a failed operation: whether the backend allows recovering (retrying) it,
/// from `can_recover`; the kind fallback applies only when the field is absent.
fn retry_state(o: &Value) -> Option<bool> {
    (o["phase"] == "failed").then(|| can_recover(o))
}
fn can_recover(o: &Value) -> bool {
    o["can_recover"].as_bool().unwrap_or(
        o["phase"] == "failed"
            && matches!(
                o["kind"].as_str(),
                Some(
                    "account_delete" | "account" | "restart" | "directory" | "pause" | "terminate"
                )
            ),
    )
}
const SYMLINKED: &str = "settings.json is a symlink";
/// Short row reason for a live-steering failure.
fn steering_reason(error: &str) -> String {
    if error.starts_with("SETTINGS_IS_SYMLINK") {
        SYMLINKED.into()
    } else {
        crate::messages::describe(error)
    }
}
fn terminal(phase: &str) -> bool {
    matches!(
        phase,
        "complete" | "completed" | "cancelled" | "failed" | "recovered"
    )
}
/// Plain-language name of an operation or launch kind.
pub fn kind_label(kind: &str) -> String {
    match kind {
        "move_space" => "Move to Space",
        "account" | "switch" => "Switch account",
        "account_delete" => "Delete account",
        "restart" => "Restart",
        "terminate" | "stop" => "End session",
        "directory" | "relocate" => "Change directory",
        "move_project" => "Move project folder",
        "pause" => "Pause work",
        "archive_session" => "Archive session",
        "restore_session" => "Restore session",
        "delete_session" => "Delete session",
        "launch" => "New session",
        "resume-launch" | "resume" => "Resume",
        other => return crate::messages::phase(other),
    }
    .into()
}
/// Badge text for work in progress, in the present tense.
fn active_text(kind: &str, record: &Value, spaces: &BTreeMap<String, String>) -> String {
    if record["phase"]
        .as_str()
        .is_some_and(|p| p.starts_with("awaiting_"))
    {
        return "needs your answer".into();
    }
    match kind {
        "move_space" => record["space_id"]
            .as_str()
            .and_then(|id| spaces.get(id))
            .map_or_else(|| "moving".into(), |name| format!("moving to {name}")),
        "account" | "switch" => record["target_account_id"].as_str().map_or_else(
            || "switching account".into(),
            |a| format!("switching to {}", crate::usage::short_name(a)),
        ),
        "restart" => "restarting".into(),
        "terminate" | "stop" => "ending".into(),
        "directory" | "relocate" | "move_project" => "changing directory".into(),
        "pause" => "pausing".into(),
        "archive_session" => "archiving".into(),
        "restore_session" => "restoring".into(),
        "delete_session" => "deleting".into(),
        "resume-launch" | "resume" => "resuming".into(),
        "launch" => "starting".into(),
        _ => "working".into(),
    }
}
/// Badge text for a failed operation.
fn failed_text(kind: &str) -> String {
    let noun = match kind {
        "move_space" => "move",
        "account" | "switch" => "switch",
        "restart" => "restart",
        "terminate" | "stop" => "end",
        "directory" | "relocate" | "move_project" => "directory change",
        "pause" => "pause",
        "archive_session" => "archive",
        "restore_session" => "restore",
        "delete_session" => "delete",
        "resume-launch" | "resume" => "resume",
        "launch" => "launch",
        _ => "operation",
    };
    format!("{noun} failed")
}
/// Short status for an operations row; failures carry their reason inline.
fn operation_row_status(kind: &str, record: &Value) -> String {
    let phase = if kind == "operation" {
        operation_status(record)
    } else {
        launch_status(record)
    };
    match phase {
        "complete" | "completed" => "done".into(),
        "failed" => {
            let reason = record["detail"]
                .as_str()
                .or_else(|| record["error"].as_str())
                .map(crate::messages::describe)
                .map(|r| clean(&r))
                .filter(|r| !r.trim().is_empty());
            match reason {
                Some(reason) => format!("failed · {}", reason.trim_end_matches('.')),
                None => "failed".into(),
            }
        }
        "running" => "in progress".into(),
        "confirmation" => "needs your answer".into(),
        "checking" => "checking".into(),
        other => other.into(),
    }
}
/// Which filter group an operations row belongs to.
fn operation_group(kind: &str, record: &Value) -> OperationFilter {
    let phase = if kind == "operation" {
        operation_status(record)
    } else {
        launch_status(record)
    };
    match phase {
        "failed" => OperationFilter::Failed,
        "complete" | "completed" | "cancelled" | "recovered" => OperationFilter::Completed,
        _ => OperationFilter::InProgress,
    }
}
/// Local calendar day (days since the epoch in local time) and its label.
fn local_day(seconds: i64) -> Option<(i64, String)> {
    let time = seconds as libc::time_t;
    let mut broken = std::mem::MaybeUninit::<libc::tm>::uninit();
    let mut buffer = [0i8; 32];
    // libc writes into fixed, valid storage and does not retain pointers.
    unsafe {
        if libc::localtime_r(&time, broken.as_mut_ptr()).is_null() {
            return None;
        }
        let tm = broken.assume_init();
        let n = libc::strftime(
            buffer.as_mut_ptr(),
            buffer.len(),
            c"%a %b %-d".as_ptr(),
            &tm,
        );
        let label =
            String::from_utf8_lossy(std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), n))
                .into_owned();
        let key = i64::from(tm.tm_year) * 400 + i64::from(tm.tm_yday);
        Some((key, label))
    }
}
/// `Today`, `Yesterday` or `Mon Oct 26`; undated legacy records are `Undated`.
fn day_heading(now: i64, then: Option<i64>) -> String {
    let Some(then) = then.map(|t| if t > 100_000_000_000 { t / 1000 } else { t }) else {
        return "Undated".into();
    };
    let (Some((today, _)), Some((day, label))) = (local_day(now), local_day(then)) else {
        return "Undated".into();
    };
    let yesterday = local_day(now - 86_400).map(|(k, _)| k);
    if day == today {
        "Today".into()
    } else if Some(day) == yesterday {
        "Yesterday".into()
    } else {
        label
    }
}
/// One entry of the Operations page layout.
#[derive(Clone, Debug, PartialEq)]
enum Slot {
    Filter,
    Heading(String),
    Entry(String, String),
}
impl Manager {
    fn request(&mut self, args: Vec<String>, purpose: Purpose) -> Vec<Intent> {
        self.sequence += 1;
        self.pending.insert(self.sequence, purpose);
        vec![Intent::Api(self.sequence, args)]
    }
    fn account(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }
    fn session(&self, key: &str) -> Option<&Value> {
        self.sessions
            .iter()
            .chain(self.archives.iter())
            .find(|s| id(s) == key)
    }
    /// Provider of an account ID (the published field when the account is known,
    /// else the qualified-ID prefix).
    fn provider_of(&self, key: &str) -> String {
        self.account(key).map_or_else(
            || {
                key.split_once(':')
                    .map_or("codex", |(provider, _)| provider)
                    .to_owned()
            },
            |a| a.provider().to_owned(),
        )
    }
    /// Title of every popup in this account's login flow ("Gemini login").
    fn login_title(&self, key: &str) -> String {
        crate::client::login_title(&self.provider_of(key))
    }
    /// Login title for the account the current mutation targets.
    fn mutation_login_title(&self) -> String {
        match &self.mutation[..] {
            [a, _, key, ..] if a == "account" => self.login_title(key),
            _ => "Provider login".into(),
        }
    }
    /// A provider-scoped capability flag; absent providers and flags are unsupported.
    fn provider_cap(&self, provider: &str, key: &str) -> bool {
        self.capabilities["providers"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p["id"] == provider && p[key] == true))
    }
    /// True only when the backend explicitly reports this provider operation as
    /// unsupported, so older backends that omit the flag keep their behaviour.
    fn provider_denies(&self, provider: &str, key: &str) -> bool {
        self.capabilities["providers"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p["id"] == provider && p[key] == false))
    }
    /// Backend text explaining an unsupported provider operation, when published.
    fn provider_reason(&self, provider: &str, key: &str) -> Option<String> {
        self.capabilities["providers"]
            .as_array()?
            .iter()
            .find(|p| p["id"] == provider)
            .and_then(|p| {
                p["operations"][key]["reason"]
                    .as_str()
                    .or_else(|| p[format!("{key}_reason")].as_str())
                    .map(clean)
            })
            .filter(|r| !r.trim().is_empty())
    }
    fn limits_account_row(&self, key: &str, removed: bool) -> Row {
        let provider = self.provider_of(key);
        let supported = self.provider_cap(&provider, "account_limits");
        let row = Row::gate("Limits", !removed && supported);
        if supported {
            row
        } else {
            let reason = self
                .provider_reason(&provider, "account_limits")
                .unwrap_or_else(|| "unsupported".into());
            row.with_hover_reason(reason)
        }
    }
    /// Live steering is offered for Claude accounts by a revision-16 backend.
    fn live_steering_supported(&self, key: &str) -> bool {
        self.provider_of(key) == "claude"
            && self.capabilities["claude_input_delivery"]["peer_message"].is_object()
    }
    /// The account's live-steering state: Some(on) when it can be changed.
    fn steering_state(&self, key: &str) -> Result<bool, String> {
        if let Some(reason) = self.steering_unavailable.get(key) {
            return Err(reason.clone());
        }
        let Some(v) = self.steering.get(key) else {
            return Err("checking…".into());
        };
        match v["value"].as_str() {
            None | Some("accept") => Ok(v["live_steering"] == true),
            // An operator's own hold/refuse is never overwritten from here.
            Some(other) => Err(format!("settings.json sets {other}")),
        }
    }
    fn live_steering_row(&self, key: &str) -> Row {
        match self.steering_state(key) {
            Ok(on) => {
                let on = self.steering_pick.unwrap_or(on);
                Row::yes("Live steering")
                    .with_picker(
                        if on { "On" } else { "Off" },
                        crate::preferences::choice_arrow(&[true, false], on),
                    )
                    .grabbed(self.steering_pick.is_some())
                    .with_hover_info("lets other sessions message this account's running sessions")
            }
            Err(reason) => Row::gate("Live steering", false).with_hover_reason(reason),
        }
    }
    /// The bypass-warning setting is offered for Claude accounts by a revision-22 backend.
    fn bypass_warning_supported(&self, key: &str) -> bool {
        self.provider_of(key) == "claude" && self.capabilities["claude_bypass_warning"].is_object()
    }
    /// The account's bypass-warning state: Some(skip) when it can be changed.
    fn bypass_state(&self, key: &str) -> Result<bool, String> {
        if let Some(reason) = self.bypass_unavailable.get(key) {
            return Err(reason.clone());
        }
        let Some(v) = self.bypass.get(key) else {
            return Err("checking…".into());
        };
        match v["bypass_warning"].as_str() {
            Some("skip") => Ok(true),
            Some("show") => Ok(false),
            _ => Err("unknown".into()),
        }
    }
    fn bypass_warning_row(&self, key: &str) -> Row {
        match self.bypass_state(key) {
            Ok(skip) => {
                let skip = self.bypass_pick.unwrap_or(skip);
                Row::yes("Bypass warning")
                    .with_picker(
                        if skip { "Skip" } else { "Show" },
                        crate::preferences::choice_arrow(&[true, false], skip),
                    )
                    .grabbed(self.bypass_pick.is_some())
                    .with_hover_info("skip Claude's YOLO-mode startup warning")
            }
            Err(reason) => Row::gate("Bypass warning", false).with_hover_reason(reason),
        }
    }
    fn logout_account_row(&self, key: &str, removed: bool) -> Row {
        let provider = self.provider_of(key);
        let supported = self.provider_cap(&provider, "account_logout");
        let row = Row::gate("Log out", !removed && supported);
        if supported {
            row
        } else {
            let reason = self
                .provider_reason(&provider, "account_logout")
                .unwrap_or_else(|| "unsupported".into());
            row.with_hover_reason(reason)
        }
    }
    /// The backend (contract 1.30+) advertises the backend rename; older ones keep the
    /// display-name-only rename.
    fn rename_backend_ui(&self) -> bool {
        self.capabilities["account_names"]["rename_backend"].is_object()
    }
    fn rekey_account(&mut self, old: &str, new: &str) {
        let swap = |page: &mut Page| {
            if matches!(page, Page::Account(k) if k == old) {
                *page = Page::Account(new.to_owned());
            }
        };
        swap(&mut self.page);
        for location in &mut self.history {
            swap(&mut location.page);
        }
        self.deletion_previews.remove(old);
        self.steering.remove(old);
        self.steering_unavailable.remove(old);
        self.bypass.remove(old);
        self.bypass_unavailable.remove(old);
    }
    fn sync_rename_dialog(&mut self) {
        let Form::AccountRenameBackend(key, current, mismatch) = self.form.clone() else {
            return;
        };
        let unavailable = self.account(&key).and_then(rename_blocked);
        let provider_ok = self.provider_cap(&self.provider_of(&key), "account_rename");
        let Some(d) = self.dialog.as_mut() else {
            return;
        };
        let text = d
            .fields
            .first()
            .map(|f| f.input.value.trim().to_owned())
            .unwrap_or_default();
        let same = text.is_empty() || text == current;
        let update = same && mismatch;
        let mut first = Row::gate(
            if update {
                "Update backend ⚠"
            } else {
                "Rename & update backend"
            },
            (!same || mismatch) && unavailable.is_none(),
        );
        if update {
            first = first.with_warning();
        }
        if let Some(reason) = unavailable {
            first = first.with_hover_reason(reason);
        }
        let only = Row::gate("Rename only", !same && provider_ok);
        for (label, row) in [("Rename only", only)] {
            if let Some(b) = d.buttons.iter_mut().find(|b| b.label == label) {
                *b = row;
            }
        }
        if let Some(b) = d
            .buttons
            .iter_mut()
            .find(|b| b.label == "Update backend ⚠" || b.label == "Rename & update backend")
        {
            *b = first;
        }
    }
    fn show_rename_backend(&mut self, a: &Account) {
        let current = crate::usage::account_title_full(a);
        let mismatch = name_mismatch(a);
        self.show(
            "Rename account",
            vec![format!(
                "Display name for {}. Update backend also renames the account folder and ID.",
                account_label(&a.id)
            )],
            &["Rename & update backend", "Rename only", "Cancel"],
            Form::AccountRenameBackend(a.id.clone(), current.clone(), mismatch),
        );
        let mut field = Field::new("Display name", current);
        field.input = field.input.limited(NAME_MAX_CHARS);
        self.dialog.as_mut().unwrap().fields.push(field);
        self.sync_rename_dialog();
    }
    fn rename_account_row(&self, key: &str) -> Row {
        let provider = self.provider_of(key);
        let row = Row::gate("Rename", self.provider_cap(&provider, "account_rename"));
        if self.provider_cap(&provider, "account_rename") {
            if self.rename_backend_ui() && self.account(key).is_some_and(name_mismatch) {
                return Row::yes("Rename ⚠")
                    .with_warning()
                    .with_hover_reason("name ≠ folder");
            }
            row
        } else {
            let reason = self
                .provider_reason(&provider, "account_rename")
                .unwrap_or_else(|| "unsupported".into());
            row.with_hover_reason(reason)
        }
    }
    fn remove_account_row(&self, key: &str, removed: bool) -> Row {
        let provider = self.provider_of(key);
        let label = if removed {
            "Restore account"
        } else {
            "Deregister account"
        };
        let supported = !self.provider_denies(&provider, "account_removal");
        let row = Row::gate(label, supported);
        if supported {
            row
        } else {
            let reason = self
                .provider_reason(&provider, "account_removal")
                .unwrap_or_else(|| "unsupported".into());
            row.with_hover_reason(reason)
        }
    }
    /// Providers the backend reports as implemented and that this UI has a flow for.
    fn add_providers(&self) -> Vec<&'static str> {
        crate::client::PROVIDERS
            .into_iter()
            .filter(|id| {
                self.capabilities["providers"].as_array().is_some_and(|ps| {
                    ps.iter()
                        .any(|p| p["id"] == *id && p["implemented"] == true)
                })
            })
            .collect()
    }
    fn delete_account_row(&self, key: &str) -> Row {
        let preview = self.deletion_previews.get(key);
        let reason = if !self.provider_cap(&self.provider_of(key), "account_deletion") {
            Some(
                self.provider_reason(&self.provider_of(key), "account_deletion")
                    .unwrap_or_else(|| "unsupported".into()),
            )
        } else if preview.is_some_and(|p| p["owner_upgrade_required"] == true) {
            Some("owner update needed".into())
        } else if preview.is_some_and(|p| p["can_delete"] == false) {
            Some(
                if preview.is_some_and(|p| {
                    p["affected_sessions"]
                        .as_array()
                        .is_some_and(|ss| ss.iter().any(|s| s["running"] == true))
                }) {
                    "active sessions".into()
                } else {
                    backend_text(preview.unwrap_or(&Value::Null))
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| "unavailable".into())
                },
            )
        } else {
            None
        };
        let row = Row::gate("Delete account", reason.is_none());
        if let Some(reason) = reason {
            row.with_hover_reason(reason)
        } else {
            row
        }
    }
    /// Archived name, shortened to `budget` cells; only colliding names gain a
    /// short ID suffix (`codex-root · 7792`).
    fn archive_label(&self, s: &Value, budget: usize) -> String {
        self.archive_row(s, budget).label
    }
    /// The archive row for `s`: its shortened name scrolls while selected.
    fn archive_row(&self, s: &Value, budget: usize) -> Row {
        let name = crate::session_list::full_name(s);
        let collides = self
            .archives
            .iter()
            .filter(|o| crate::session_list::full_name(o) == name)
            .count()
            > 1;
        let suffix = if collides {
            format!(" · {}", id(s).chars().take(4).collect::<String>())
        } else {
            String::new()
        };
        let short = crate::widgets::ellipsize(&name, budget.saturating_sub(suffix.chars().count()));
        let mut row = Row::yes(format!("{short}{suffix}"));
        row.shorten(
            0,
            unicode_width::UnicodeWidthStr::width(short.as_str()),
            &name,
        );
        row
    }
    fn selected_account(&self) -> Option<String> {
        match &self.page {
            Page::Home if self.options.selected >= 5 => self
                .accounts
                .get(self.options.selected - 5)
                .map(|a| a.id.clone()),
            Page::Account(id) => Some(id.clone()),
            _ => None,
        }
    }
    /// Put the cursor in the remembered home column, clamped to the columns the
    /// row has.
    fn apply_home_column(&mut self) {
        self.usage_focus = self.home_column > 0 && self.usage_selectable();
    }
    fn usage_selectable(&self) -> bool {
        self.page == Page::Home
            && self
                .selected_account()
                .as_deref()
                .and_then(|id| self.account(id))
                .is_some()
    }
    /// `m` is offered only with an account row focused and a backend that can save order.
    fn can_start_move(&self) -> bool {
        self.page == Page::Home
            && self.dialog.is_none()
            && self.moving.is_none()
            && !self.options.back
            && !self.usage_focus
            && self.capabilities["account_reorder"] == true
            && self.options.selected >= 5
            && self.options.selected - 5 < self.accounts.len()
    }
    /// Keys while an account is picked up. Returns None when not in move mode.
    fn move_key(&mut self, key: KeyEvent) -> Option<Vec<Intent>> {
        let original = self.moving.as_ref()?;
        let at = self.options.selected.saturating_sub(5);
        match key.code {
            KeyCode::Up | KeyCode::Down => {
                let to = if key.code == KeyCode::Up {
                    at.checked_sub(1)
                } else {
                    (at + 1 < self.accounts.len()).then_some(at + 1)
                };
                if let Some(to) = to {
                    self.accounts.swap(at, to);
                    self.options.selected = 5 + to;
                }
                Some(vec![])
            }
            KeyCode::Enter => {
                let id = self.accounts[at].id.clone();
                let unchanged = original
                    .iter()
                    .map(|a| &a.id)
                    .eq(self.accounts.iter().map(|a| &a.id));
                let original = self.moving.take().unwrap_or_default();
                if unchanged {
                    return Some(vec![]);
                }
                // Saved silently: no progress or acknowledgement, only failures.
                Some(self.request(
                    strings(&["account", "reorder", &id, "--position", &at.to_string()]),
                    Purpose::Reorder(original),
                ))
            }
            KeyCode::Esc => {
                let id = self.accounts.get(at).map(|a| a.id.clone());
                self.accounts = self.moving.take().unwrap_or_default();
                if let Some(i) = id.and_then(|id| self.accounts.iter().position(|a| a.id == id)) {
                    self.options.selected = 5 + i;
                }
                Some(vec![])
            }
            _ => Some(vec![]),
        }
    }
    fn show_usage(&mut self, a: &Account) {
        let now = crate::usage::clock();
        self.show(
            "Account details",
            usage_lines(a, now),
            &["Close"],
            Form::None,
        );
        if let Some(d) = &mut self.dialog {
            Self::details_layout(d);
        }
    }
    /// The backend's ordered Gemini sign-in options; `None` for an older backend
    /// that does not publish `auth_modes` (the plain login flow then applies).
    fn gemini_auth_modes(&self) -> Option<Vec<AuthChoice>> {
        let modes = self.capabilities["providers"]
            .as_array()?
            .iter()
            .find(|p| p["id"] == "gemini")?["auth_modes"]
            .as_array()?;
        let choices: Vec<AuthChoice> = modes
            .iter()
            .filter_map(|m| {
                Some(AuthChoice {
                    mode: m["mode"].as_str()?.to_owned(),
                    label: clean(m["label"].as_str()?),
                    available: m["available"] == true,
                    reason: m["reason"]
                        .as_str()
                        .map(clean)
                        .filter(|r| !r.trim().is_empty()),
                })
            })
            .collect();
        (!choices.is_empty()).then_some(choices)
    }
    /// Gemini sign-in choice. Unavailable options are struck through, cannot be
    /// chosen, and show the backend's reason only while the cursor is on them.
    fn show_gemini_auth(&mut self, key: &str, choices: Vec<AuthChoice>) {
        let mut buttons: Vec<&str> = choices.iter().map(|c| c.label.as_str()).collect();
        buttons.push("Cancel");
        self.show(
            &self.login_title(key),
            vec![format!("Choose how {} signs in.", account_label(key))],
            &buttons,
            Form::GeminiAuth(key.to_owned(), choices.clone()),
        );
        let d = self.dialog.as_mut().unwrap();
        for (row, choice) in d.buttons.iter_mut().zip(&choices) {
            let mut gated = Row::gate(choice.label.as_str(), choice.available);
            if let (false, Some(reason)) = (choice.available, &choice.reason) {
                gated = gated.with_preview(format!("({reason})"));
                gated.preview_fg = crate::widgets::MUTED;
            }
            *row = gated;
        }
        d.selected = choices.iter().position(|c| c.available).unwrap_or(0);
        d.separate_cancel();
    }
    fn show_gemini_key(&mut self, key: &str) {
        self.show(
            "Gemini API key",
            vec![format!(
                "Enter the API key for {}. It is sent to the CLI over stdin and is never shown or logged.",
                account_label(key)
            )],
            &["Save key", "Cancel"],
            Form::GeminiKey(key.to_owned()),
        );
        let d = self.dialog.as_mut().unwrap();
        d.separate_cancel();
        d.fields
            .push(Field::new("API key", "").validated(Rule::Required).secret());
    }
    /// Send the key through stdin only, then drop it from the dialog state.
    fn submit_gemini_key(&mut self, key: &str, secret: String) -> Vec<Intent> {
        let args = strings(&["account", "auth", key, "--mode", "api-key", "--key-stdin"]);
        let mut progress = Dialog::progress("Gemini API key");
        progress.body = vec![
            format!(
                "Saving and verifying the API key for {}…",
                account_label(key)
            ),
            "This can take up to 15 seconds.".into(),
        ];
        progress.size = self.dialog.as_ref().and_then(|d| d.size);
        // The entry dialog (and its field) is replaced; nothing keeps the clear text.
        self.dialog = Some(progress);
        self.form = Form::Progress;
        self.sequence += 1;
        self.pending
            .insert(self.sequence, Purpose::AuthKey(key.to_owned()));
        self.foreground = Some(self.sequence);
        vec![Intent::ApiInput(
            self.sequence,
            args,
            Secret(json!({ "api_key": secret })),
        )]
    }
    fn gemini_key_checked(&mut self, key: &str, value: &Value, shown: bool) {
        let validation = &value["validation"];
        let code = validation["code"]
            .as_str()
            .map(clean)
            .filter(|c| !c.is_empty());
        let reason = validation["reason"]
            .as_str()
            .map(clean)
            .filter(|r| !r.is_empty());
        let (title, body) = match validation["state"].as_str() {
            Some("valid") => (
                "Gemini API key",
                vec![format!(
                    "The Gemini API key for {} was saved and verified.",
                    account_label(key)
                )],
            ),
            Some("auth_failed") => (
                "Gemini API key",
                vec![
                    format!(
                        "Gemini did not accept the API key for {}.",
                        account_label(key)
                    ),
                    reason
                        .or(code)
                        .unwrap_or_else(|| "The key is invalid.".into()),
                ],
            ),
            _ => (
                "Gemini API key",
                vec![
                    format!(
                        "The API key for {} is saved but could not be verified{}.",
                        account_label(key),
                        code.map(|c| format!(" ({c})")).unwrap_or_default()
                    ),
                    reason.unwrap_or_else(|| "Try the account again later.".into()),
                ],
            ),
        };
        if shown {
            self.show(title, body, &["Close"], Form::None);
        } else {
            self.note = body.join(" ");
        }
    }
    fn show_login(&mut self, key: &str) {
        if self.provider_of(key) == "gemini"
            && let Some(choices) = self.gemini_auth_modes()
        {
            return self.show_gemini_auth(key, choices);
        }
        self.show(
            &self.login_title(key),
            vec![format!("Start the login for {}?", account_label(key))],
            &["Login", "Cancel"],
            Form::Login(key.into()),
        );
    }
    /// Log in: Claude, Gemini and Antigravity first look for a login already
    /// running in a HERDR pane, so its sign-in link can be shown full screen.
    fn begin_login(&mut self, key: &str) -> Vec<Intent> {
        if !crate::login_link::pane_provider(&self.provider_of(key)) {
            self.show_login(key);
            return vec![];
        }
        self.show(
            &self.login_title(key),
            vec!["Checking for a login in progress…".into()],
            &["Cancel"],
            Form::LoginLink(key.into()),
        );
        self.login_link = Some(LoginLink {
            key: key.into(),
            seen: false,
            in_flight: true,
            last: Some(std::time::Instant::now()),
            screen: None,
        });
        vec![Intent::ReadLoginPane(key.into())]
    }
    fn login_link_body(&self, key: &str, screen: Option<&str>) -> Vec<String> {
        let provider = self.provider_of(key);
        let label = crate::client::provider_label(&provider);
        match screen {
            None => vec!["The login pane has closed.".into()],
            Some(screen) => match crate::login_link::extract(screen) {
                Some(link) => vec![
                    "A login is already in progress.".into(),
                    format!("Sign-in link from the {label} login pane:"),
                    link,
                    "Finish signing in in the login pane.".into(),
                ],
                None => vec![
                    "A login is already in progress.".into(),
                    format!("Waiting for {label} to print its sign-in link…"),
                    "Finish signing in in the login pane.".into(),
                ],
            },
        }
    }
    /// The login pane's current screen. Shown again whenever it changes; the
    /// link option appears once a link is on it and follows a replaced link.
    pub fn login_screen(
        &mut self,
        key: String,
        result: Result<Option<String>, String>,
    ) -> Vec<Intent> {
        let Some(state) = self.login_link.as_mut() else {
            return vec![];
        };
        if state.key != key || !matches!(&self.form, Form::LoginLink(k) if *k == key) {
            return vec![];
        }
        state.in_flight = false;
        state.last = Some(std::time::Instant::now());
        let screen = match result {
            Ok(Some(screen)) => screen,
            Ok(None) if !state.seen => {
                self.login_link = None;
                self.show_login(&key);
                return vec![];
            }
            Ok(None) => {
                let body = self.login_link_body(&key, None);
                if let Some(d) = self.dialog.as_mut() {
                    d.body = body;
                }
                return vec![];
            }
            Err(e) => {
                if let Some(d) = self.dialog.as_mut()
                    && !state.seen
                {
                    d.body = vec![format!("Could not read the login pane: {}", clean(&e))];
                }
                return vec![];
            }
        };
        if state.screen.as_deref() == Some(screen.as_str()) {
            return vec![];
        }
        let first = !state.seen;
        state.seen = true;
        state.screen = Some(screen.clone());
        let body = self.login_link_body(&key, Some(&screen));
        if first {
            self.show(
                &self.login_title(&key),
                body,
                &["Start a new login", "Close"],
                Form::LoginLink(key),
            );
        } else if let Some(d) = self.dialog.as_mut() {
            d.body = body;
        }
        vec![]
    }
    fn login_link_poll(&mut self) -> Vec<Intent> {
        let Some(state) = self.login_link.as_mut() else {
            return vec![];
        };
        if !matches!(&self.form, Form::LoginLink(k) if *k == state.key) {
            self.login_link = None;
            return vec![];
        }
        if state.in_flight || state.last.is_some_and(|t| t.elapsed() < LOGIN_POLL) {
            return vec![];
        }
        state.in_flight = true;
        vec![Intent::ReadLoginPane(state.key.clone())]
    }
    fn activate_usage(&mut self) -> Vec<Intent> {
        let key = self.selected_account().unwrap();
        let a = self.account(&key).unwrap().clone();
        if let Some(operation) = a.details.get("deletion").filter(|v| v["id"].is_string()) {
            return self.deletion_operation(operation.clone());
        }
        let usage = Usage::account(&a);
        match usage.status.as_str() {
            "Disabled" => self.show(
                "Enable account",
                vec![format!("Enable {key} for session selection?")],
                &["Enable", "Cancel"],
                Form::Confirm(strings(&["account", "enable", &key])),
            ),
            "Login required" if self.provider_cap(&self.provider_of(&key), "enrollment") => {
                return self.begin_login(&key);
            }
            "Deregistered" => self.show(
                "Restore account",
                vec![format!("Restore {key} to account selection?")],
                &["Restore", "Cancel"],
                Form::Confirm(strings(&["account", "restore", &key])),
            ),
            _ if !usage.status_only()
                && self.provider_cap(&self.provider_of(&key), "account_limits") =>
            {
                return self.preview(
                    "Account limits",
                    strings(&["account", "limits", &key]),
                    Purpose::Limits(key),
                );
            }
            _ => self.show_usage(&a),
        }
        vec![]
    }
    fn deletion_operation(&mut self, operation: Value) -> Vec<Intent> {
        let key = id(&operation).to_owned();
        self.operations.retain(|o| id(o) != key);
        self.operations.push(operation);
        self.enter(Page::Operation(key));
        self.refresh()
    }
    pub fn inventory(&mut self, accounts: Vec<Account>) {
        if self.moving.is_some() {
            return;
        }
        let selected = self.selected_account();
        self.accounts = accounts;
        self.derive_session_fields();
        self.inventory_known = true;
        self.validate_dialog_names();
        if let Some(id) = selected {
            if let Some(index) = self.accounts.iter().position(|a| a.id == id) {
                if self.page == Page::Home {
                    self.options.selected = 5 + index;
                }
            } else if self.page == Page::Home {
                self.options.selected = 0;
            }
        }
    }
    fn validate_dialog_names(&mut self) {
        let excluded = match &self.form {
            // Names are unique per provider; other providers may reuse them.
            Form::Add(provider) => self
                .accounts
                .iter()
                .filter(|a| a.provider() == provider)
                .filter_map(|a| a.id.split_once(':').map(|(_, name)| name.to_owned()))
                .collect(),
            Form::Rename(target) | Form::Placement(target, ..) => self
                .sessions
                .iter()
                .filter(|s| s["id"] != target["id"])
                .filter_map(|s| s["name"].as_str().map(str::to_owned))
                .collect(),
            _ => return,
        };
        if let Some(field) = self.dialog.as_mut().and_then(|d| d.fields.first_mut()) {
            field.excluded = excluded;
        }
    }
    pub fn title(&self) -> String {
        match &self.page {
            Page::Queue => {
                return self.history.last().map_or_else(
                    || "Operations".into(),
                    |l| {
                        Manager {
                            page: l.page.clone(),
                            sessions: self.sessions.clone(),
                            archives: self.archives.clone(),
                            operations: self.operations.clone(),
                            ..Manager::default()
                        }
                        .title()
                    },
                );
            }
            Page::Home => "",
            Page::Account(key) => {
                let (provider, name) = key.split_once(':').unwrap_or(("codex", key.as_str()));
                return format!(
                    "Account: {} {name}",
                    crate::usage::provider_symbol(provider)
                );
            }
            Page::Sessions => "Manage sessions",
            Page::Archives => "Archived sessions",
            Page::Session(key) | Page::Spaces(key) => {
                return format!(
                    "Session: {}",
                    self.session(key)
                        .and_then(|s| s["name"].as_str())
                        .unwrap_or("Unnamed")
                );
            }
            Page::Operations => "Operations",
            Page::Launch(kind, _) => return kind_label(kind),
            Page::Operation(key) => {
                return self
                    .operations
                    .iter()
                    .find(|o| id(o) == key)
                    .and_then(|o| o["kind"].as_str())
                    .map_or_else(|| "Operation".into(), kind_label);
            }
        }
        .into()
    }
    fn go(&mut self, page: Page) {
        if page == Page::Home {
            self.history.clear();
        }
        self.dialog = None;
        self.form = Form::None;
        self.foreground = None;
        self.details_open = false;
        self.page = page;
        self.popup_size = None;
        self.usage_focus = false;
        self.home_column = 0;
        self.queue_focus = None;
        self.steering_pick = None;
        self.bypass_pick = None;
        self.filter_grab = None;
        self.options = Options::default();
        self.options.selected = self.rows().iter().position(|r| !r.separator).unwrap_or(0);
        self.note.clear();
    }
    fn focus(&self) -> Option<Focus> {
        let index = self.options.selected;
        match &self.page {
            Page::Home if self.selected_account().is_some() => {
                self.selected_account().map(Focus::Account)
            }
            Page::Sessions
                if crate::session_list::session_index(&self.sessions, index).is_some() =>
            {
                Some(Focus::Session(
                    id(&self.sessions
                        [crate::session_list::session_index(&self.sessions, index).unwrap()])
                    .into(),
                ))
            }
            Page::Archives if index < self.archives.len() => {
                Some(Focus::Session(id(&self.archives[index]).into()))
            }
            Page::Operations => self
                .operation_keys()
                .get(index)
                .cloned()
                .flatten()
                .map(|(kind, key)| Focus::Operation(kind, key)),
            _ => self
                .rows()
                .get(index)
                .map(|r| Focus::Label(r.label.clone())),
        }
    }
    fn enter(&mut self, page: Page) {
        self.history.push(Location {
            page: self.page.clone(),
            options: self.options.clone(),
            focus: self.focus(),
            usage_focus: self.usage_focus,
        });
        self.go(page);
    }
    fn return_previous(&mut self) -> bool {
        let Some(location) = self.history.pop() else {
            return false;
        };
        self.go(location.page);
        self.options = location.options;
        self.options.back = false;
        self.options.navigation = Default::default();
        self.options.preview_visible = false;
        let selected = match location.focus {
            Some(Focus::Account(key)) => self
                .accounts
                .iter()
                .position(|a| a.id == key)
                .map(|i| i + 5),
            Some(Focus::Session(key)) => match self.page {
                Page::Archives => self.archives.iter().position(|s| id(s) == key),
                _ => self
                    .sessions
                    .iter()
                    .position(|s| id(s) == key)
                    .map(|i| crate::session_list::session_row(&self.sessions, i)),
            },
            Some(Focus::Operation(kind, key)) => self
                .operation_keys()
                .iter()
                .position(|entry| entry.as_ref() == Some(&(kind.clone(), key.clone()))),
            Some(Focus::Label(label)) => self.rows().iter().position(|r| r.label == label),
            None => None,
        };
        let rows = self.rows();
        self.options.selected = selected
            .unwrap_or(self.options.selected)
            .min(rows.len().saturating_sub(1));
        if rows.get(self.options.selected).is_some_and(|r| r.separator) {
            self.options.selected = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| !r.separator)
                .min_by_key(|(index, _)| index.abs_diff(self.options.selected))
                .map(|(index, _)| index)
                .unwrap_or(0);
        }
        self.home_column = usize::from(location.usage_focus);
        self.usage_focus = location.usage_focus && self.usage_selectable();
        true
    }
    fn account_back(&mut self, key: &str) {
        self.go(Page::Home);
        self.options.selected = self
            .accounts
            .iter()
            .position(|a| a.id == key)
            .map_or(0, |i| i + 5);
    }
    pub fn rows(&self) -> Vec<Row> {
        match &self.page {
            Page::Home => {
                let mut rows = vec![
                    Row::yes("New session"),
                    Row::yes("Manage sessions"),
                    Row::yes("Add account"),
                    Row::separator(),
                    Row::yes("Settings"),
                ];
                rows.extend(self.accounts.iter().map(|a| {
                    Row::overflowing(
                        crate::usage::account_title_full(a),
                        crate::usage::ACCOUNT_TITLE_CELLS,
                        true,
                    )
                    .account()
                }));
                rows
            }
            Page::Account(key) => {
                let a = self.account(key);
                if a.is_some_and(|a| {
                    a.details
                        .get("deletion")
                        .is_some_and(|v| v["id"].is_string())
                }) {
                    return vec![Row::yes("Deletion operation")];
                }
                let removed =
                    a.is_some_and(|a| a.details.get("membership").is_some_and(|v| v == "removed"));
                let logged_in =
                    !a.is_some_and(|a| a.details.get("auth_required").is_some_and(|v| v == true));
                let provider = self.provider_of(key);
                let mut rows = vec![
                    Row::yes("Details"),
                    self.rename_account_row(key),
                    Row::separator(),
                ];
                if logged_in {
                    // Gemini and Antigravity publish no usage, so they have no Limits at all.
                    if !matches!(provider.as_str(), "gemini" | "antigravity") {
                        rows.push(self.limits_account_row(key, removed));
                    }
                    // Older backends omit the capability: hide Log out entirely.
                    if self.provider_cap(&provider, "account_logout")
                        || self.provider_denies(&provider, "account_logout")
                    {
                        rows.push(self.logout_account_row(key, removed));
                    }
                } else {
                    rows.push(Row::gate(
                        "Log in",
                        !removed && self.provider_cap(&provider, "enrollment"),
                    ));
                }
                if self.live_steering_supported(key) {
                    rows.extend([Row::separator(), self.live_steering_row(key)]);
                }
                if self.bypass_warning_supported(key) {
                    if !self.live_steering_supported(key) {
                        rows.push(Row::separator());
                    }
                    rows.push(self.bypass_warning_row(key));
                }
                rows.extend([
                    Row::separator(),
                    Row::gate(
                        if a.is_some_and(|a| a.details.get("enabled").is_some_and(|v| v == false)) {
                            "Enable"
                        } else {
                            "Disable"
                        },
                        !removed,
                    ),
                    self.remove_account_row(key, removed),
                    self.delete_account_row(key),
                ]);
                crate::widgets::align_reasons(&mut rows);
                // The selector glyph one space after the longest name, as in Settings.
                let longest = rows
                    .iter()
                    .map(|r| unicode_width::UnicodeWidthStr::width(r.label.as_str()))
                    .max()
                    .unwrap_or(0);
                for row in rows.iter_mut().filter(|r| r.preview_arrows) {
                    row.preview_column = Some(2 + longest + 1);
                }
                crate::widgets::apply_selector_mode(&mut rows);
                rows
            }
            Page::Sessions => crate::session_list::rows_with_archives(
                &self.sessions,
                self.sessions_known,
                !self.archives.is_empty(),
            ),
            Page::Archives => self
                .archives
                .iter()
                .map(|s| Row::yes(self.archive_label(s, usize::MAX)))
                .collect(),
            Page::Spaces(_) => crate::popup::space_rows(
                self.space_choices["spaces"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|space| {
                        (
                            space["name"].as_str().unwrap_or("Unnamed Space"),
                            space["id"].as_str().unwrap_or(""),
                            space["current"] == true,
                        )
                    }),
                &self.new_space_state(self.place),
            ),
            Page::Session(key) => crate::session_list::actions(
                self.session(key).unwrap_or(&Value::Null),
                self.capabilities["resume"]["version"] == 1,
            )
            .into_iter()
            .map(|(row, _)| row)
            .collect(),
            Page::Launch(kind, key) => {
                let receipt = self.launch_record(kind, key);
                vec![
                    Row::yes(
                        if receipt.is_some_and(|r| {
                            terminal(r["phase"].as_str().unwrap_or("")) || r["phase"] == "running"
                        }) {
                            "Details"
                        } else {
                            "Progress"
                        },
                    ),
                    Row::gate(
                        "Transfer operation",
                        receipt.is_some_and(|r| r["operation_id"].is_string()),
                    ),
                ]
            }
            Page::Operations => self.operation_rows(),
            Page::Queue => self.queue_rows(),
            Page::Operation(key) => {
                let o = self
                    .operations
                    .iter()
                    .find(|o| id(o) == key)
                    .cloned()
                    .unwrap_or(Value::Null);
                let phase = o["phase"].as_str().unwrap_or("");
                vec![
                    Row::yes(if terminal(phase) {
                        "Details"
                    } else {
                        "Progress"
                    }),
                    Row::gate(
                        "Respond yes",
                        o["can_respond"]
                            .as_bool()
                            .unwrap_or(phase.starts_with("awaiting")),
                    ),
                    Row::gate(
                        "Respond no",
                        o["can_respond"]
                            .as_bool()
                            .unwrap_or(phase.starts_with("awaiting")),
                    ),
                    Row::separator(),
                    Row::gate(
                        "Cancel operation",
                        !terminal(phase) && o["can_cancel"] != false,
                    ),
                    Row::gate("Recover", can_recover(&o)),
                ]
            }
        }
    }
    fn operation_entries(&self) -> Vec<(&'static str, &str, &str, &Value)> {
        let mut entries: Vec<_> = self
            .operations
            .iter()
            .map(|r| {
                (
                    "operation",
                    id(r),
                    r["kind"].as_str().unwrap_or("operation"),
                    r,
                )
            })
            .chain(self.launches.iter().flat_map(|(kind, rs)| {
                rs.iter().map(move |r| {
                    (
                        *kind,
                        r["request"]["request_id"].as_str().unwrap_or(""),
                        if *kind == "launch" {
                            "New session"
                        } else {
                            "Resume"
                        },
                        r,
                    )
                })
            }))
            .collect();
        entries.sort_by(|a, b| {
            b.3["created_at"]
                .as_i64()
                .cmp(&a.3["created_at"].as_i64())
                .then_with(|| a.0.cmp(b.0))
                .then_with(|| a.1.cmp(b.1))
        });
        entries
    }
    /// Operations page layout: the filter selector, then day headings and entries.
    fn operation_slots(&self) -> Vec<Slot> {
        let now = crate::usage::clock();
        let mut slots = vec![Slot::Filter];
        let mut last: Option<String> = None;
        for (kind, key, _, record) in self.operation_entries() {
            if self.operation_filter != OperationFilter::All
                && operation_group(kind, record) != self.operation_filter
            {
                continue;
            }
            let heading = day_heading(now, record["created_at"].as_i64());
            if last.as_ref() != Some(&heading) {
                slots.push(Slot::Heading(heading.clone()));
                last = Some(heading);
            }
            slots.push(Slot::Entry(kind.into(), key.into()));
        }
        if slots.len() == 1 {
            slots.push(Slot::Heading(
                if self.operation_filter == OperationFilter::All {
                    "No operations"
                } else {
                    "Nothing matches this filter"
                }
                .into(),
            ));
        }
        slots
    }
    /// The entry key at each Operations row (None for the filter and headings).
    fn operation_keys(&self) -> Vec<Option<(String, String)>> {
        self.operation_slots()
            .into_iter()
            .map(|slot| match slot {
                Slot::Entry(kind, key) => Some((kind, key)),
                _ => None,
            })
            .collect()
    }
    fn retain_operation_focus(&mut self, chosen: Option<(String, String)>) {
        if let Some(chosen) = chosen {
            self.options.selected = self
                .operation_keys()
                .iter()
                .position(|k| k.as_ref() == Some(&chosen))
                .unwrap_or(0);
        }
        let rows = self.rows();
        self.options.selected = self.options.selected.min(rows.len().saturating_sub(1));
        if rows.get(self.options.selected).is_some_and(|r| r.separator) {
            self.options.selected = 0;
        }
    }
    /// Provider symbol and the best known name for an operation's subject.
    fn operation_subject(&self, kind: &str, r: &Value) -> String {
        let (session, account) = if kind == "operation" {
            (
                r["source_session_id"]
                    .as_str()
                    .or_else(|| r["session_id"].as_str()),
                r["account_id"]
                    .as_str()
                    .or_else(|| r["source_account_id"].as_str()),
            )
        } else {
            (
                r["request"]["session_id"].as_str(),
                r["request"]["account_id"].as_str(),
            )
        };
        let symbol = |provider: &str| crate::usage::provider_symbol(provider);
        if let Some(s) = session.and_then(|id| self.session(id)) {
            let provider = s["provider"]
                .as_str()
                .or_else(|| {
                    s["account_id"]
                        .as_str()
                        .and_then(|a| a.split_once(':').map(|p| p.0))
                })
                .unwrap_or("codex");
            return format!("{} {}", symbol(provider), crate::session_list::full_name(s));
        }
        let provider_of = |a: &str| a.split_once(':').map_or("codex", |(p, _)| p).to_owned();
        let named = if kind == "operation" {
            r["name"].as_str()
        } else {
            r["request"]["name"].as_str()
        };
        if let Some(name) = named.filter(|n| !n.is_empty()) {
            return format!(
                "{} {}",
                symbol(&account.map(provider_of).unwrap_or_else(|| "codex".into())),
                clean(name)
            );
        }
        match account {
            Some(a) => format!(
                "{} {}",
                symbol(&provider_of(a)),
                self.account(a)
                    .map(account_title)
                    .unwrap_or_else(|| crate::usage::short_name(a))
            ),
            None => "unknown session".into(),
        }
    }
    /// Newest-first rows: relative time, subject, plain kind and an inline status.
    fn operation_rows(&self) -> Vec<Row> {
        use unicode_width::UnicodeWidthStr;
        let now = crate::usage::clock();
        let entries = self.operation_entries();
        let find = |kind: &str, key: &str| {
            entries
                .iter()
                .find(|e| e.0 == kind && e.1 == key)
                .map(|e| e.3)
        };
        let slots = self.operation_slots();
        /// Time, shortened subject, kind, status and full subject of an operation.
        type Described = (String, String, String, String, String);
        let describe = |kind: &str, key: &str| -> Option<Described> {
            find(kind, key).map(|r| {
                let code = if kind == "operation" {
                    r["kind"].as_str().unwrap_or("operation")
                } else {
                    kind
                };
                (
                    relative(now, r["created_at"].as_i64()).replace("unknown", "—"),
                    crate::widgets::ellipsize(&self.operation_subject(kind, r), 24),
                    kind_label(code),
                    operation_row_status(kind, r),
                    self.operation_subject(kind, r),
                )
            })
        };
        let described: Vec<Option<Described>> = slots
            .iter()
            .map(|slot| match slot {
                Slot::Entry(kind, key) => describe(kind, key),
                _ => None,
            })
            .collect();
        // Column widths come from every operation, not the filtered subset, so the
        // list stays put when the filter changes.
        let everything: Vec<_> = entries.iter().filter_map(|e| describe(e.0, e.1)).collect();
        let width = |f: fn(&Described) -> &String| {
            everything
                .iter()
                .map(|d| UnicodeWidthStr::width(f(d).as_str()))
                .max()
                .unwrap_or(0)
        };
        let (time_w, subject_w, kind_w) = (width(|d| &d.0), width(|d| &d.1), width(|d| &d.2));
        let row_width = time_w + subject_w + kind_w + 4;
        let pad = |text: &str, w: usize| {
            format!(
                "{text}{}",
                " ".repeat(w.saturating_sub(UnicodeWidthStr::width(text)))
            )
        };
        let mut rows: Vec<Row> = slots
            .iter()
            .zip(described)
            .map(|(slot, described)| match (slot, described) {
                (Slot::Filter, _) => {
                    let mut row = Row::yes("Show")
                        .with_picker(
                            self.operation_filter.label(),
                            crate::preferences::choice_arrow(
                                &OperationFilter::ALL,
                                self.operation_filter,
                            ),
                        )
                        .grabbed(self.filter_grab.is_some());
                    row.preview_column = Some(2 + 4 + 1);
                    row.min_width = row_width;
                    row
                }
                (Slot::Heading(label), _) => Row::heading(label.clone()),
                (Slot::Entry(..), Some((time, subject, kind, status, full_subject))) => {
                    let label = format!(
                        "{}{time}  {}  {}",
                        " ".repeat(time_w.saturating_sub(UnicodeWidthStr::width(time.as_str()))),
                        pad(&subject, subject_w),
                        pad(&kind, kind_w),
                    );
                    let mut row = Row::yes(label).with_reason(status);
                    row.shorten(
                        time_w + 2,
                        UnicodeWidthStr::width(subject.as_str()),
                        &full_subject,
                    );
                    row.preview = row.preview.map(|p| p.trim_start_matches("· ").to_owned());
                    row.preview_column = Some(2 + time_w + subject_w + kind_w + 6);
                    row
                }
                (Slot::Entry(..), None) => Row::yes("Unknown operation"),
            })
            .collect();
        crate::widgets::apply_selector_mode(&mut rows);
        rows
    }
    fn launch_record(&self, kind: &str, key: &str) -> Option<&Value> {
        self.launches
            .get(kind)?
            .iter()
            .find(|r| r["request"]["request_id"] == key)
    }
    pub fn refresh(&mut self) -> Vec<Intent> {
        self.refresh_with(false)
    }
    /// Periodic refresh: like `refresh`, but session state is throttled.
    pub fn refresh_poll(&mut self) -> Vec<Intent> {
        self.refresh_with(true)
    }
    fn refresh_with(&mut self, poll: bool) -> Vec<Intent> {
        let mut out = vec![];
        if let Page::Account(key) = self.page.clone()
            && self.live_steering_supported(&key)
            && !self
                .pending
                .values()
                .any(|p| matches!(p, Purpose::LiveSteering(_)))
        {
            out.extend(self.request(
                strings(&["account", "live-steering", &key]),
                Purpose::LiveSteering(key),
            ));
        }
        if let Page::Account(key) = self.page.clone()
            && self.bypass_warning_supported(&key)
            && !self
                .pending
                .values()
                .any(|p| matches!(p, Purpose::BypassWarning(_)))
        {
            out.extend(self.request(
                strings(&["account", "bypass-warning", &key]),
                Purpose::BypassWarning(key),
            ));
        }
        if let Page::Account(key) = self.page.clone()
            && self.provider_cap(&self.provider_of(&key), "account_deletion")
            && !self
                .pending
                .values()
                .any(|p| matches!(p, Purpose::DeleteAvailability(_)))
        {
            out.extend(self.request(
                strings(&["account", "delete", &key]),
                Purpose::DeleteAvailability(key),
            ));
        }
        if (matches!(self.page, Page::Archives | Page::Sessions)
            || matches!(&self.page, Page::Session(key) if self.archives.iter().any(|s|id(s)==key)))
            && !self
                .pending
                .values()
                .any(|p| matches!(p, Purpose::Archives))
        {
            out.extend(self.request(strings(&["archives"]), Purpose::Archives));
        }
        if matches!(self.page, Page::Operations | Page::Launch(..)) {
            for kind in ["launch", "resume-launch"] {
                let cap = if kind == "launch" { "launch" } else { "resume" };
                if self.capabilities[cap]["list"] == true
                    && !self
                        .pending
                        .values()
                        .any(|p| matches!(p,Purpose::Launches(k) if *k==kind))
                {
                    out.extend(self.request(strings(&[kind, "list"]), Purpose::Launches(kind)));
                }
            }
        }
        if let Page::Launch(kind, key) = self.page.clone()
            && !self
                .pending
                .values()
                .any(|p| matches!(p, Purpose::Launch(..)))
        {
            out.extend(self.request(
                strings(&[kind, "status", "--request-id", &key]),
                Purpose::Launch(kind, key),
            ));
        }
        if poll {
            out.extend(self.poll_sessions(false));
        } else {
            out.extend(self.refresh_sessions());
        }
        if (self.active_operation.is_some()
            || !self.queue.is_empty()
            || self
                .operations
                .iter()
                .any(|o| !terminal(o["phase"].as_str().unwrap_or("")))
            || matches!(self.form, Form::Operation(_) | Form::OperationApproval(..))
            || matches!(
                self.page,
                Page::Operations | Page::Operation(_) | Page::Sessions | Page::Queue
            ))
            && !self
                .pending
                .values()
                .any(|p| matches!(p, Purpose::Operations))
        {
            out.extend(self.request(strings(&["operations"]), Purpose::Operations));
        }
        if let Page::Session(key) = &self.page
            && self
                .observe_after
                .is_none_or(|deadline| std::time::Instant::now() >= deadline)
            && !self
                .pending
                .values()
                .any(|p| matches!(p, Purpose::Observe(_)))
            && let Some(s) = self
                .session(key)
                .filter(|s| {
                    s["availability"] == "running" && !crate::session_list::is_native_provider(s)
                })
                .cloned()
        {
            self.observe_after =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
            out.extend(self.request(Self::target(&s, "observe"), Purpose::Observe(key.clone())));
        }
        out
    }
    const STATE_POLL_TICKS: u32 = 5;
    const FULL_STATE_TICKS: u32 = 30;
    fn state_cached_supported(&self) -> bool {
        self.capabilities["state_cached"] == true
    }
    /// Single-flight full `state`; with a cached read available and nothing shown
    /// yet, a cheap `state --cached` rides along for the first paint.
    fn request_full_state(&mut self) -> Vec<Intent> {
        self.ticks_since_state = 0;
        self.ticks_since_full = 0;
        let mut out = vec![];
        if self.state_cached_supported()
            && (!self.sessions_known || self.cached_snapshot)
            && self.cached_tokens.is_empty()
        {
            let mut cached = self.request(strings(&["state", "--cached"]), Purpose::State);
            if let Some(Intent::Api(token, _)) = cached.first() {
                self.cached_tokens.insert(*token);
            }
            out.append(&mut cached);
        }
        if !self
            .pending
            .iter()
            .any(|(t, p)| matches!(p, Purpose::State) && !self.cached_tokens.contains(t))
        {
            out.extend(self.request(strings(&["state"]), Purpose::State));
        }
        out
    }
    /// Explicit read (page entry, mutation, focus): always the full state.
    pub fn read_sessions(&mut self) -> Vec<Intent> {
        self.request_full_state()
    }
    pub fn refresh_sessions(&mut self) -> Vec<Intent> {
        if matches!(self.page, Page::Sessions | Page::Session(_)) {
            self.request_full_state()
        } else {
            vec![]
        }
    }
    /// Timer-driven read: every 5 ticks (seconds), cheap when the backend offers a
    /// cached read, full every 30 s, and nothing while the UI is hidden.
    pub fn poll_sessions(&mut self, any_page: bool) -> Vec<Intent> {
        self.ticks_since_state += 1;
        self.ticks_since_full += 1;
        let eligible = any_page || matches!(self.page, Page::Sessions | Page::Session(_));
        if !self.visible
            || !eligible
            || self.ticks_since_state < Self::STATE_POLL_TICKS
            || self.pending.values().any(|p| matches!(p, Purpose::State))
        {
            return vec![];
        }
        if self.state_cached_supported() && self.ticks_since_full < Self::FULL_STATE_TICKS {
            self.ticks_since_state = 0;
            let out = self.request(strings(&["state", "--cached"]), Purpose::State);
            if let Some(Intent::Api(token, _)) = out.first() {
                self.cached_tokens.insert(*token);
            }
            out
        } else {
            self.request_full_state()
        }
    }
    /// Paint the saved account list on the first frame. Usage is marked stale so
    /// the existing age styling shows when it was observed, and nothing is
    /// selectable; the first real inventory replaces it.
    pub fn show_cached_accounts(&mut self, accounts: &Value) {
        if self.inventory_known {
            return;
        }
        let accounts: Vec<Account> = accounts
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| serde_json::from_value::<Account>(a.clone()).ok())
            .map(|mut a| {
                a.selectable = false;
                a.prefill = Value::Null;
                a.details.insert("usage_fresh".into(), json!(false));
                if matches!(crate::usage::usage_state(&a), Some("fresh" | "partial")) {
                    a.details.insert("usage_state".into(), json!("stale"));
                    if a.usage.is_object() {
                        a.usage["state"] = json!("stale");
                    }
                }
                a
            })
            .collect();
        self.inventory(accounts);
        self.inventory_known = false;
    }
    /// Paint the saved session list immediately. Capabilities and generations are
    /// dropped, so no session action can be enabled from it.
    pub fn show_cached(&mut self, sessions: &Value) {
        if self.sessions_known {
            return;
        }
        let sessions: Vec<Value> = sessions
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| {
                let mut s = s.clone();
                if let Some(o) = s.as_object_mut() {
                    o.remove("generation");
                    o.insert("capabilities".into(), json!({}));
                    o.insert("activity".into(), json!("unknown"));
                }
                s
            })
            .collect();
        self.sequence += 1;
        let token = self.sequence;
        self.pending.insert(token, Purpose::State);
        let capabilities = self.capabilities.clone();
        self.response(
            token,
            Ok(json!({"sessions":sessions,"capabilities":capabilities})),
        );
        self.cached_snapshot = true;
        self.cache_out = None;
    }
    /// Result of opening the login pane. Only a failure is surfaced, with the
    /// command a person can run themselves.
    pub fn login_launched(&mut self, result: Result<(), String>) -> Vec<Intent> {
        if result.is_ok() {
            // Get out of the way: the login pane is focused and its sign-in link must
            // not sit under this popup. The pane reports success or failure itself.
            if self.login_wait.take().is_some() {
                self.dialog = None;
                self.form = Form::None;
                return vec![Intent::Exit];
            }
            return vec![];
        }
        if let Err(e) = result
            && let Some(wait) = self.login_wait.take()
            && matches!(&self.form, Form::LoginWait(k) if *k == wait.key)
        {
            self.dialog = None;
            let title = self.login_title(&wait.key);
            self.show(
                &title,
                vec![
                    clean(&e),
                    "Run this command in a terminal:".into(),
                    login_command(&json!(wait.argv)),
                ],
                &["Close"],
                Form::None,
            );
        }
        vec![]
    }
    /// Fields the UI derives from published data: Claude sessions skip the observe
    /// round-trip, so their activity comes from availability and native status;
    /// session account tags use the account display name when one is set.
    fn derive_session_fields(&mut self) {
        let titles: BTreeMap<String, String> = self
            .accounts
            .iter()
            .filter(|a| {
                a.details
                    .get("display_name")
                    .and_then(Value::as_str)
                    .is_some_and(|n| !n.trim().is_empty())
            })
            .map(|a| (a.id.clone(), account_title(a)))
            .collect();
        for s in self.sessions.iter_mut().chain(self.archives.iter_mut()) {
            if crate::session_list::is_native_provider(s) && s["availability"] == "running" {
                match s["native_status"].as_str() {
                    Some("busy") => s["activity"] = json!("busy"),
                    Some("idle") => s["activity"] = json!("idle"),
                    _ => {}
                }
            }
            // Without `active_since`, the verified attachment process start on this
            // host is the same measure; otherwise no counter is shown.
            let since = (s["availability"] == "running" && s["active_since"].is_null())
                .then(|| crate::session_list::attachment_started(s))
                .flatten();
            match (since, s.as_object_mut()) {
                (Some(t), _) => s["attachment_since"] = json!(t),
                (None, Some(o)) => {
                    o.remove("attachment_since");
                }
                (None, None) => {}
            }
            match s["account_id"].as_str().and_then(|id| titles.get(id)) {
                Some(t) => s["account_label"] = json!(t),
                None => {
                    if let Some(o) = s.as_object_mut() {
                        o.remove("account_label");
                    }
                }
            }
        }
    }
    fn login_poll(&mut self) -> Vec<Intent> {
        let Some(wait) = &mut self.login_wait else {
            return vec![];
        };
        if !matches!(&self.form, Form::LoginWait(k) if *k == wait.key) {
            self.login_wait = None;
            return vec![];
        }
        if wait.started.elapsed() >= LOGIN_LIMIT {
            let key = wait.key.clone();
            let title = self.login_title(&key);
            self.login_wait = None;
            self.dialog = None;
            self.show(
                &title,
                vec![
                    "Login not completed: timed out waiting for sign-in. Choose Log in on the account page to try again."
                        .into(),
                ],
                &["Close"],
                Form::None,
            );
            return vec![];
        }
        if wait.last_poll.is_some_and(|t| t.elapsed() < LOGIN_POLL)
            || self
                .pending
                .values()
                .any(|p| matches!(p, Purpose::LoginPoll(_)))
        {
            return vec![];
        }
        wait.last_poll = Some(std::time::Instant::now());
        let key = wait.key.clone();
        self.request(strings(&["account", "list"]), Purpose::LoginPoll(key))
    }
    fn login_polled(&mut self, key: String, value: Value) {
        let Some(wait) = &self.login_wait else { return };
        if wait.key != key || !matches!(&self.form, Form::LoginWait(k) if *k == key) {
            return;
        }
        let Some(found) = value
            .as_array()
            .and_then(|a| a.iter().find(|a| id(a) == key))
            .filter(|a| a["auth_required"] == false)
        else {
            return;
        };
        match self.accounts.iter_mut().find(|a| a.id == key) {
            Some(a) => {
                a.details.insert("auth_required".into(), json!(false));
            }
            None => {
                if let Ok(a) = serde_json::from_value(found.clone()) {
                    self.accounts.push(a);
                }
            }
        }
        self.login_wait = None;
        self.dialog = None;
        let label = crate::client::provider_label(&self.provider_of(&key)).to_owned();
        if self.page != Page::Account(key.clone()) && self.account(&key).is_some() {
            self.enter(Page::Account(key.clone()));
        }
        self.show(
            &self.login_title(&key),
            vec![format!("{label} is signed in for {}.", account_label(&key))],
            &["Close"],
            Form::None,
        );
    }
    pub fn refresh_live(&mut self) -> Vec<Intent> {
        let login = self.login_poll();
        let link = self.login_link_poll();
        if !link.is_empty() {
            return link;
        }
        if !login.is_empty() {
            return login;
        }
        if matches!(
            self.page,
            Page::Operations | Page::Operation(_) | Page::Launch(..)
        ) || matches!(self.form, Form::Operation(_) | Form::OperationApproval(..))
        {
            self.refresh_with(true)
        } else {
            let mut out = self.poll_sessions(false);
            // Session badges follow operations: every second while one is active,
            // otherwise every five seconds.
            self.ticks_since_operations += 1;
            let active = self.badges().values().any(|b| b.active);
            if self.page == Page::Sessions
                && self.visible
                && self.ticks_since_operations >= if active { 1 } else { 5 }
                && !self
                    .pending
                    .values()
                    .any(|p| matches!(p, Purpose::Operations))
            {
                self.ticks_since_operations = 0;
                out.extend(self.request(strings(&["operations"]), Purpose::Operations));
            }
            out
        }
    }
    /// Session details share one layout for every provider: the facts people use
    /// first, then a dim Identifiers group.
    fn session_detail_lines(value: &Value) -> Vec<String> {
        let text = |v: &Value| v.as_str().map(clean).filter(|s| !s.is_empty());
        let now = crate::usage::clock();
        let mut status = match value["availability"].as_str() {
            Some("running") => match value["native_status"]
                .as_str()
                .or_else(|| value["activity"].as_str())
            {
                Some("busy") => "running · busy".to_owned(),
                Some("idle") => "running · idle".to_owned(),
                _ => "running".to_owned(),
            },
            Some("stopped" | "saved") => "saved".into(),
            Some("archived") => "archived".into(),
            Some("stale") => "stale".into(),
            Some(other) if !other.is_empty() => clean(other),
            _ => "unknown".into(),
        };
        if matches!(
            text(&value["native"]["kind"]).as_deref(),
            Some("bg" | "background")
        ) {
            status.push_str(" · background");
        }
        let created = value["conversation_created_at"].as_i64().or_else(|| {
            value["conversation_created_at"]
                .as_str()
                .and_then(parse_time)
        });
        let mut facts = vec![
            ("Account", crate::session_list::account_tag(value)),
            ("Status", status),
            (
                "Directory",
                text(&value["directory"]).unwrap_or_else(|| "unknown".into()),
            ),
            (
                "Last active",
                relative(now, value["last_accessed"].as_i64()),
            ),
            ("Created", relative(now, created)),
        ];
        if let Some(bytes) = value["history_file_bytes"].as_u64() {
            facts.push(("History", size(bytes)));
        }
        let mut ids = vec![("Session", id(value).to_owned())];
        if let Some(native) = text(&value["conversation_id"]) {
            ids.push(("Native session", native));
        }
        if let Some(pid) = value["attachment"]["pid"]
            .as_u64()
            .or_else(|| value["pid"].as_u64())
            .filter(|_| value["availability"] == "running")
        {
            ids.push(("Process", pid.to_string()));
        }
        if let Some(job) = text(&value["native"]["job_id"]) {
            ids.push(("Job", job));
        }
        if let Some(earlier) = value["previous_conversation_ids"].as_array() {
            let earlier: Vec<_> = earlier.iter().filter_map(&text).collect();
            if !earlier.is_empty() {
                ids.push(("Earlier IDs", earlier.join(" · ")));
            }
        }
        if let Some(parent) = text(&value["forked_from_conversation_id"]) {
            ids.push(("Forked from", parent));
        }
        let width = facts
            .iter()
            .map(|(l, _)| l.len() + 1)
            .chain(ids.iter().map(|(l, _)| l.len() + 2))
            .max()
            .unwrap_or(0);
        let mut lines: Vec<String> = facts
            .into_iter()
            .map(|(label, value)| format!("{:<width$}  {value}", format!("{label}:")))
            .collect();
        lines.push(String::new());
        lines.push("Identifiers".into());
        lines.extend(
            ids.into_iter()
                .map(|(label, value)| format!("  {label:<w$}  {value}", w = width - 2)),
        );
        lines
    }
    /// Details layout: wrapped values hang under their column, and everything from
    /// the Identifiers group on is dim.
    fn details_layout(dialog: &mut Dialog) {
        dialog.hanging = true;
        dialog.dim_from = dialog
            .body
            .iter()
            .position(|l| l == "Identifiers" || l.starts_with("┿ marks"));
    }
    fn target(s: &Value, command: &str) -> Vec<String> {
        strings(&[
            command,
            "--session",
            id(s),
            "--expected-generation",
            s["generation"].as_str().unwrap_or(""),
        ])
    }
    fn show(&mut self, title: &str, body: Vec<String>, buttons: &[&str], form: Form) {
        let mut dialog = Dialog::new(title, body, buttons);
        dialog.compact =
            buttons == ["Close"] && matches!(form, Form::None | Form::SessionDetails(_));
        // A replacing dialog keeps at least the frame it replaces, and grows when its
        // own content needs more room (a small "Checking…" frame must not clip the
        // options that follow it). An info-only dialog fits its own content.
        if let Some(previous) = (!dialog.compact)
            .then(|| self.dialog.as_ref().and_then(|d| d.size))
            .flatten()
        {
            dialog.minimum_width = previous.width;
            dialog.minimum_height = previous.height;
        }
        if matches!(form, Form::Loading) {
            dialog.minimum_height = dialog.minimum_height.max(16);
            dialog.minimum_width = dialog.minimum_width.max(76);
        }
        self.dialog = Some(dialog);
        self.form = form;
    }
    fn mutate(&mut self, args: Vec<String>) -> Vec<Intent> {
        if self.queue_or_register(&args) {
            // Accepted into the queue: back to the list with the target selected.
            if matches!(self.page, Page::Session(_) | Page::Spaces(_)) {
                self.session_return = args
                    .windows(2)
                    .find(|w| w[0] == "--session")
                    .map(|w| w[1].clone());
                self.leave_to_list();
            }
            return vec![];
        }
        self.mutation = args.clone();
        if args.first().is_none_or(|arg| arg != "respond") {
            self.session_return = args.windows(2).find(|w| w[0] == "--session").map(|w| {
                if matches!(
                    args.first().map(String::as_str),
                    Some("archive" | "delete-session")
                ) {
                    String::new()
                } else {
                    w[1].clone()
                }
            });
            self.details_open = false;
        }
        let mut progress = Dialog::progress("Operation progress");
        progress.size = self.dialog.as_ref().and_then(|d| d.size);
        self.dialog = Some(progress);
        self.form = Form::Progress;
        let request = self.request(args, Purpose::Mutation);
        self.foreground = Some(self.sequence);
        request
    }
    fn preview(&mut self, title: &str, args: Vec<String>, purpose: Purpose) -> Vec<Intent> {
        self.show(title, vec!["Loading…".into()], &["Cancel"], Form::Loading);
        self.dialog.as_mut().unwrap().busy = true;
        let request = self.request(args, purpose);
        self.foreground = Some(self.sequence);
        request
    }
    pub fn switch_confirmation(&mut self, s: Value, account: &str) {
        // Account selection may outlive several state polls. Confirm the current
        // attachment explicitly, then retain that generation through submission.
        let Some(s) = self.session(id(&s)).cloned() else {
            self.note = "SESSION_NOT_FOUND".into();
            return;
        };
        if s["capabilities"]["switch_account"] != true && !s["pending_operation_id"].is_string() {
            self.note = "The session is not currently available for account switching. Review its state in Manage sessions.".into();
            return;
        }
        let mut args = Self::target(&s, "switch");
        args.extend(strings(&["--to", account]));
        self.confirm("Switch account",vec![format!("Move {} to {} while retaining conversation history? The backend waits for safe idle; it never steers.",s["name"].as_str().unwrap_or("session"),account)],args);
    }
    fn dismiss(&mut self) {
        self.login_link = None;
        if matches!(self.form, Form::Progress | Form::Operation(_))
            && !self.usage_focus
            && !self.details_open
        {
            if let Some(key) = self.session_return.take() {
                if let Some(index) = self
                    .history
                    .iter()
                    .rposition(|location| location.page == Page::Sessions)
                {
                    self.history.truncate(index);
                }
                self.go(Page::Sessions);
                self.options.selected = self
                    .sessions
                    .iter()
                    .position(|s| id(s) == key)
                    .map(|i| crate::session_list::session_row(&self.sessions, i))
                    .unwrap_or(0);
            } else {
                self.go(Page::Home);
            }
        }
        self.dialog = None;
        self.form = Form::None;
        self.foreground = None;
        self.details_open = false;
        if self.login_wait.take().is_some() {
            self.note = "Stopped waiting for sign-in. A login pane that is already open keeps running; finish or close it there.".into();
        }
    }
    fn confirm(&mut self, title: &str, body: Vec<String>, args: Vec<String>) {
        self.show(title, body, &["Confirm", "Cancel"], Form::Confirm(args));
    }
    pub fn key(&mut self, key: KeyEvent) -> Vec<Intent> {
        self.key_with_wrap(key, key.kind != crossterm::event::KeyEventKind::Repeat)
    }
    pub fn key_with_wrap(&mut self, key: KeyEvent, wrap: bool) -> Vec<Intent> {
        self.validate_dialog_names();
        if let Some(d) = &mut self.dialog {
            if d.link_open() {
                // Esc in the link view only goes back; it never arms closing.
                self.close_armed = None;
            }
            match d.key_with_wrap(key, wrap) {
                dialog::Event::None => {
                    self.sync_rename_dialog();
                    return vec![];
                }
                dialog::Event::Dismiss => {
                    if matches!(self.form, Form::OperationApproval(..)) {
                        return self.submit(1, vec![]);
                    }
                    self.dismiss();
                    return vec![];
                }
                dialog::Event::Choose(button, values) => return self.submit(button, values),
            }
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return vec![];
        }
        if let Some(out) = self.move_key(key) {
            return out;
        }
        if self.steering_pick.is_some() {
            return self.steering_key(key.code);
        }
        if self.bypass_pick.is_some() {
            return self.bypass_key(key.code);
        }
        if matches!(
            key.code,
            KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ')
        ) && let Some((_, skip)) = self.bypass_selected()
        {
            self.bypass_pick = Some(skip);
            return vec![];
        }
        if matches!(
            key.code,
            KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ')
        ) && let Some((_, on)) = self.steering_selected()
        {
            self.steering_pick = Some(on);
            return vec![];
        }
        if self.filter_grab.is_some() {
            return self.filter_key(key.code);
        }
        if matches!(
            key.code,
            KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ')
        ) && self.page == Page::Operations
            && self.options.selected == 0
            && !self.options.back
        {
            self.filter_grab = Some(self.operation_filter);
            return vec![];
        }
        if key.code == KeyCode::Char('m') && self.can_start_move() {
            self.moving = Some(self.accounts.clone());
            return vec![];
        }
        if self.page == Page::Queue {
            return match key.code {
                KeyCode::Esc | KeyCode::Char('o') => self.back(),
                // Left only moves the cursor to Back; it never leaves the page.
                KeyCode::Left => {
                    self.options.back = true;
                    vec![]
                }
                KeyCode::Right => {
                    self.options.back = false;
                    vec![]
                }
                KeyCode::Enter | KeyCode::Char(' ') if self.options.back => self.back(),
                KeyCode::Enter | KeyCode::Char(' ') => self.activate_queue(),
                KeyCode::Up | KeyCode::Down => {
                    self.options
                        .step_with_wrap(&self.rows(), false, key.code == KeyCode::Up, wrap);
                    self.note_queue_focus();
                    vec![]
                }
                KeyCode::Home | KeyCode::End => {
                    self.options
                        .boundary(&self.rows(), false, key.code == KeyCode::End);
                    self.note_queue_focus();
                    vec![]
                }
                KeyCode::PageUp | KeyCode::PageDown => {
                    self.options.page(&self.rows(), key.code == KeyCode::PageUp);
                    self.note_queue_focus();
                    vec![]
                }
                _ => vec![],
            };
        }
        if key.code == KeyCode::Char('o') && self.moving.is_none() {
            return self.open_queue();
        }
        let armed = self.close_armed.take();
        match key.code {
            KeyCode::Home | KeyCode::End => {
                self.options
                    .boundary(&self.rows(), false, key.code == KeyCode::End);
                self.badge_focus = false;
                self.apply_home_column();
                vec![]
            }
            KeyCode::PageUp | KeyCode::PageDown => {
                if self.page != Page::Home {
                    self.options.page(&self.rows(), key.code == KeyCode::PageUp);
                    self.badge_focus = false;
                }
                vec![]
            }
            KeyCode::Up | KeyCode::Down => {
                let before = self.options.selected;
                let up = key.code == KeyCode::Up;
                self.options.step_with_wrap(&self.rows(), false, up, wrap);
                // Wrapping jumps the Home account list to the matching end at once.
                let selected = self.options.selected;
                if self.page == Page::Home && selected != before && (selected > before) == up {
                    self.account_scroll = if up { self.accounts.len() } else { 0 };
                }
                self.badge_focus = false;
                self.apply_home_column();
                vec![]
            }
            KeyCode::Left if matches!(self.page, Page::Spaces(_)) => vec![],
            KeyCode::Left => {
                if self.usage_focus {
                    self.usage_focus = false;
                } else if self.badge_focus {
                    self.badge_focus = false;
                } else {
                    self.options.back = true;
                }
                // Back, or the name column: return on the left from now on.
                self.home_column = 0;
                vec![]
            }
            KeyCode::Right => {
                if !self.options.back && self.usage_selectable() {
                    self.usage_focus = true;
                    self.home_column = 1;
                }
                if !self.options.back && self.selected_badge().is_some() {
                    self.badge_focus = true;
                }
                self.options.back = false;
                vec![]
            }
            KeyCode::Esc if self.page == Page::Home && self.history.is_empty() => {
                // Closing the popup from Home needs a second Esc within one second.
                if armed.is_some_and(|t| t.elapsed() <= CLOSE_WINDOW) {
                    vec![Intent::Exit]
                } else {
                    let now = std::time::Instant::now();
                    self.close_armed = Some(now);
                    if self
                        .close_band
                        .is_none_or(|t| t.elapsed() >= crate::close_band::TOTAL)
                    {
                        self.close_band = Some(now);
                    }
                    vec![]
                }
            }
            KeyCode::Esc => self.back(),
            KeyCode::Enter | KeyCode::Char(' ') => {
                if self.options.back {
                    self.back()
                } else if self.badge_focus {
                    self.open_badge()
                } else {
                    self.activate()
                }
            }
            _ => vec![],
        }
    }
    /// Hide a finished operation's badge for good; running ones stay.
    fn dismiss_operation(&mut self, key: &str) {
        if self
            .operations
            .iter()
            .any(|o| id(o) == key && terminal(o["phase"].as_str().unwrap_or("")))
            && self.dismissed.insert(key.to_owned())
        {
            self.dismissed_out = Some(self.dismissed.iter().cloned().collect());
            self.badge_focus = false;
        }
    }
    /// Show a dismissed operation's badge again, under the usual badge rules.
    fn undismiss_operation(&mut self, key: &str) {
        if self.dismissed.remove(key) {
            self.dismissed_out = Some(self.dismissed.iter().cloned().collect());
        }
    }
    /// Keys while the Operations filter is grabbed: Up/Down choose (no wrap), Left,
    /// Enter and Space commit, Esc restores the value held when it was grabbed.
    fn filter_key(&mut self, code: KeyCode) -> Vec<Intent> {
        match code {
            KeyCode::Up | KeyCode::Down => {
                self.operation_filter = crate::preferences::step(
                    &OperationFilter::ALL,
                    self.operation_filter,
                    code == KeyCode::Up,
                );
            }
            KeyCode::Left | KeyCode::Enter | KeyCode::Char(' ') => self.filter_grab = None,
            KeyCode::Esc => {
                if let Some(saved) = self.filter_grab.take() {
                    self.operation_filter = saved;
                }
            }
            _ => {}
        }
        vec![]
    }
    /// The selected row is an available Live steering selector: its account and state.
    fn steering_selected(&self) -> Option<(String, bool)> {
        let Page::Account(key) = &self.page else {
            return None;
        };
        if self.options.back || self.dialog.is_some() || !self.live_steering_supported(key) {
            return None;
        }
        let on = self.steering_state(key).ok()?;
        (self.rows().get(self.options.selected)?.label == "Live steering")
            .then(|| (key.clone(), on))
    }
    /// Keys while Live steering is grabbed: Up/Down choose, Esc reverts, Enter sets.
    /// The value shown afterwards is the state the backend reports.
    fn steering_key(&mut self, code: KeyCode) -> Vec<Intent> {
        match code {
            KeyCode::Up | KeyCode::Down => self.steering_pick = Some(code == KeyCode::Up),
            KeyCode::Esc => self.steering_pick = None,
            KeyCode::Left | KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(pick) = self.steering_pick.take()
                    && let Some((key, on)) = self.steering_selected()
                    && pick != on
                {
                    let flag = if pick { "--enable" } else { "--disable" };
                    return self.mutate(strings(&["account", "live-steering", &key, flag]));
                }
            }
            _ => {}
        }
        vec![]
    }
    /// The selected row is an available Bypass warning selector: its account and state.
    fn bypass_selected(&self) -> Option<(String, bool)> {
        let Page::Account(key) = &self.page else {
            return None;
        };
        if self.options.back || self.dialog.is_some() || !self.bypass_warning_supported(key) {
            return None;
        }
        let skip = self.bypass_state(key).ok()?;
        (self.rows().get(self.options.selected)?.label == "Bypass warning")
            .then(|| (key.clone(), skip))
    }
    /// Keys while Bypass warning is grabbed: Up/Down choose, Esc reverts, Enter sets.
    /// The value shown afterwards is the state the backend reports.
    fn bypass_key(&mut self, code: KeyCode) -> Vec<Intent> {
        match code {
            KeyCode::Up | KeyCode::Down => self.bypass_pick = Some(code == KeyCode::Up),
            KeyCode::Esc => self.bypass_pick = None,
            KeyCode::Left | KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(pick) = self.bypass_pick.take()
                    && let Some((key, skip)) = self.bypass_selected()
                    && pick != skip
                {
                    let flag = if pick { "--skip" } else { "--show" };
                    return self.mutate(strings(&["account", "bypass-warning", &key, flag]));
                }
            }
            _ => {}
        }
        vec![]
    }
    /// The selected session row's operation badge, when it has one.
    fn selected_badge(&self) -> Option<crate::session_list::Badge> {
        if self.page != Page::Sessions {
            return None;
        }
        let index = crate::session_list::session_index(&self.sessions, self.options.selected)?;
        let key = id(&self.sessions[index]);
        self.badges().remove(key)
    }
    /// True while a first Esc on Home waits for its confirming second press.
    pub fn close_pending(&self) -> bool {
        self.close_armed
            .is_some_and(|t| t.elapsed() <= CLOSE_WINDOW)
    }
    /// Pending and recently failed operations by session ID.
    pub fn badges(&self) -> BTreeMap<String, crate::session_list::Badge> {
        let now = crate::usage::clock();
        let mut out = BTreeMap::new();
        for s in &self.sessions {
            let key = id(s);
            let newest = self
                .operations
                .iter()
                .filter(|o| {
                    o["source_session_id"] == key
                        || o["session_id"] == key
                        || s["pending_operation_id"].as_str() == Some(id(o))
                })
                .max_by_key(|o| o["created_at"].as_i64());
            let running = match newest {
                Some(o) if !terminal(o["phase"].as_str().unwrap_or("unknown")) => Some((
                    active_text(
                        o["kind"].as_str().unwrap_or("operation"),
                        o,
                        &self.space_names,
                    ),
                    id(o),
                )),
                Some(_) => None,
                None => s["pending_operation_id"]
                    .as_str()
                    .map(|op| ("operation running".to_owned(), op)),
            };
            // Running backend work first, then the queue (starting or queued),
            // then the session's most recent outcome when it is a recent failure.
            let badge = match running {
                Some((text, op)) => Some(crate::session_list::Badge {
                    kind: "operation".into(),
                    id: op.to_owned(),
                    text,
                    active: true,
                    queued: false,
                }),
                None => self
                    .queue_badge(key)
                    .or_else(|| self.outcome_badge(key, s, now)),
            };
            if let Some(badge) = badge {
                out.insert(key.to_owned(), badge);
            }
        }
        out
    }
    /// Enter on a badge opens that operation's progress or details over the list.
    fn open_badge(&mut self) -> Vec<Intent> {
        let Some(badge) = self.selected_badge() else {
            self.badge_focus = false;
            return vec![];
        };
        if badge.kind == "queue" {
            self.open_queue_entry(&badge.id);
            return vec![];
        }
        self.open_operation(badge.id)
    }
    /// An operation's progress or details dialog over the current page.
    fn open_operation(&mut self, key: String) -> Vec<Intent> {
        let badge = crate::session_list::Badge {
            kind: "operation".into(),
            id: key,
            text: String::new(),
            active: false,
            queued: false,
        };
        self.details_open = true;
        let o = self
            .operations
            .iter()
            .find(|o| id(o) == badge.id)
            .cloned()
            .unwrap_or(Value::Null);
        let phase = o["phase"].as_str().unwrap_or("unknown");
        let finished = terminal(phase);
        let dismissed = self.dismissed.contains(&badge.id);
        self.show(
            &kind_label(o["kind"].as_str().unwrap_or("operation")),
            Self::operation_body(&o, &badge.id),
            &["Continue in background"],
            Form::Operation(badge.id.clone()),
        );
        if let Some(d) = &mut self.dialog {
            d.dim_from = Some(d.body.len().saturating_sub(2));
            d.hanging = true;
            if finished {
                finish_operation(d, dismissed, retry_state(&o));
            } else {
                d.busy = true;
            }
        }
        if !self
            .pending
            .values()
            .any(|p| matches!(p, Purpose::Operations))
        {
            return self.request(strings(&["operations"]), Purpose::Operations);
        }
        vec![]
    }
    /// Operation dialog body: what is happening, the backend detail, and its ID.
    fn operation_body(o: &Value, key: &str) -> Vec<String> {
        let mut body = vec![if o.is_null() {
            "Loading…".to_owned()
        } else {
            crate::messages::operation_phase(
                o["kind"].as_str().unwrap_or(""),
                o["phase"].as_str().unwrap_or("unknown"),
            )
        }];
        if let Some(detail) = o["detail"].as_str().map(clean).filter(|d| !d.is_empty()) {
            body.push(detail);
        }
        body.push(String::new());
        body.push(format!("Operation ID  {key}"));
        body
    }
    pub fn paste(&mut self, text: &str) {
        if let Some(d) = &mut self.dialog {
            d.paste(text);
        }
        self.sync_rename_dialog();
    }
    fn back(&mut self) -> Vec<Intent> {
        if self.page == Page::Home {
            return vec![Intent::Exit];
        }
        if self.return_previous() {
            return self.refresh();
        }
        match self.page.clone() {
            Page::Home => return vec![Intent::Exit],
            Page::Account(key) => self.account_back(&key),
            Page::Session(key) => {
                if self.archives.iter().any(|s| id(s) == key) {
                    self.go(Page::Archives);
                    self.options.selected =
                        self.archives.iter().position(|s| id(s) == key).unwrap_or(0);
                    return self.refresh();
                }
                self.go(Page::Sessions);
                self.options.selected = self
                    .sessions
                    .iter()
                    .position(|s| id(s) == key)
                    .map(|i| crate::session_list::session_row(&self.sessions, i))
                    .unwrap_or(0)
            }
            Page::Spaces(key) => self.go(Page::Session(key)),
            Page::Operation(_) | Page::Launch(..) => self.go(Page::Operations),
            Page::Operations => self.go(Page::Sessions),
            Page::Archives => self.go(Page::Sessions),
            _ => self.go(Page::Home),
        }
        vec![]
    }
    fn add(&mut self) {
        let providers = self.add_providers();
        if providers.len() > 1 {
            let mut buttons: Vec<&str> = providers
                .iter()
                .map(|p| crate::client::provider_label(p))
                .collect();
            buttons.push("Cancel");
            self.show(
                "Add account",
                vec!["Choose the provider for the new account.".into()],
                &buttons,
                Form::AddProvider(providers),
            );
            self.dialog.as_mut().unwrap().separate_cancel();
        } else {
            self.add_for(providers.first().copied().unwrap_or("codex"));
        }
    }
    fn add_for(&mut self, provider: &str) {
        let label = crate::client::provider_label(provider);
        let mut body = vec![format!(
            "{label} account name: 1–{NAME_MAX_CHARS} letters, digits, hyphens or underscores."
        )];
        if matches!(provider, "claude" | "gemini" | "antigravity") {
            body.push(format!(
                "Login runs {label}'s own interactive sign-in in a terminal."
            ));
        }
        self.show(
            "Add account",
            body,
            &["Add account", "Cancel"],
            Form::Add(provider.to_owned()),
        );
        self.dialog.as_mut().unwrap().separate_cancel();
        self.dialog
            .as_mut()
            .unwrap()
            .fields
            .push(Field::new("Name", "").validated(Rule::AccountName));
        self.validate_dialog_names();
    }
    /// The one place that decides whether `[create new space]` works for an action
    /// (`None` is a move between Spaces). Only the backend's per-operation capability
    /// enables it; until it is advertised the row stays visible but unselectable.
    /// Source: `capabilities.new_space.{resume,add_to_herdr,move_space}` from `api state`
    /// (backend contract revision 20).
    fn new_space_state(&self, place: Option<Place>) -> crate::popup::NewSpace {
        let (operation, action) = match place {
            Some(Place::Add) => ("add_to_herdr", "adding"),
            Some(Place::Start) => ("resume", "starting"),
            None => ("move_space", "moving"),
        };
        if self.capabilities["new_space"][operation] == true {
            crate::popup::NewSpace::Enabled
        } else {
            crate::popup::NewSpace::Disabled(format!("not available for {action} yet"))
        }
    }
    /// Open on the preferred Space, else the first selectable row (never a disabled
    /// `[create new space]`). Index 0 is `[create new space]`, Spaces follow it.
    fn preselect_space(&mut self, preferred: Option<String>) {
        let index = preferred
            .and_then(|want| {
                self.space_choices["spaces"]
                    .as_array()?
                    .iter()
                    .position(|space| space["id"] == want.as_str())
            })
            .map(|i| i + 1);
        self.options.selected = index
            .filter(|&i| self.rows().get(i).is_some_and(|r| r.enabled))
            .unwrap_or_else(|| {
                self.rows()
                    .iter()
                    .position(|r| r.enabled)
                    .unwrap_or_default()
            });
    }
    fn new_space_dialog(&mut self) -> Vec<Intent> {
        let session = self.space_session.clone();
        self.show(
            "New Space name",
            vec!["Name the Space to create for this session.".into()],
            &["Continue", "Cancel"],
            Form::NewSpace(session, self.place),
        );
        let taken: Vec<String> = self.space_choices["spaces"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["name"].as_str().map(str::to_owned))
            .collect();
        let mut field = Field::new("Space name", "").validated(Rule::SpaceName);
        field.excluded = taken;
        self.dialog.as_mut().unwrap().fields.push(field);
        vec![]
    }
    /// Confirm placing the session in an existing or a new Space (Start, Add, Move).
    fn choose_space(&mut self, target: SpaceTarget, label: &str) -> Vec<Intent> {
        let session = self.space_session.clone();
        let where_ = match &target {
            SpaceTarget::Existing(_) => format!("a new tab in {label}"),
            SpaceTarget::New(_) => format!("a new tab in a new Space named {label}"),
        };
        let Some(place) = self.place else {
            let mut args = Self::target(&session, "move-space");
            args.extend(match &target {
                SpaceTarget::Existing(id) => strings(&["--space", id]),
                SpaceTarget::New(label) => strings(&["--new-space", label]),
            });
            args.extend(strings(&[
                "--request-id",
                &uuid::Uuid::new_v4().to_string(),
            ]));
            self.confirm("Move to Space", vec![format!("Move this terminal to {where_}? Its running process, account, directory and history stay unchanged. Split siblings stay in place; an empty source tab or Space may close.")], args);
            return vec![];
        };
        let name = session["name"].as_str().unwrap_or("").to_owned();
        let (title, body, button) = match place {
            Place::Add => (
                "Add to HERDR",
                format!(
                    "Open this running session in {where_}. HERDR only views the existing session: no agent is started and no input is sent."
                ),
                "Add to HERDR",
            ),
            Place::Start => (
                "Start session",
                format!(
                    "Start this saved session in {where_}. The conversation is resumed; no message is sent."
                ),
                "Start session",
            ),
        };
        self.show(
            title,
            vec![body],
            &[button, "Cancel"],
            Form::Placement(session, target, place),
        );
        self.dialog
            .as_mut()
            .unwrap()
            .fields
            .push(Field::new("Name", name.clone()).validated(Rule::Rename(name)));
        self.validate_dialog_names();
        vec![]
    }
    fn activate(&mut self) -> Vec<Intent> {
        if matches!(self.page, Page::Spaces(_)) && self.options.selected + 1 == self.rows().len() {
            return self.back();
        }
        if !self
            .rows()
            .get(self.options.selected)
            .is_some_and(|r| r.enabled)
        {
            return vec![];
        }
        if self.usage_focus && self.usage_selectable() {
            return self.activate_usage();
        }
        let index = self.options.selected;
        match self.page.clone() {
            Page::Queue => return self.activate_queue(),
            Page::Home => match index {
                0 => return vec![Intent::New],
                1 => {
                    self.enter(Page::Sessions);
                    return self.refresh();
                }
                2 => self.add(),
                4 => return vec![Intent::Settings],
                i if i >= 5 => {
                    if let Some(a) = self.accounts.get(i - 5) {
                        self.enter(Page::Account(a.id.clone()));
                    }
                }
                _ => {}
            },
            Page::Account(key) => {
                let Some(a) = self.account(&key).cloned() else {
                    self.note = "Account is no longer available".into();
                    return vec![];
                };
                if let Some(operation) = a.details.get("deletion").filter(|v| v["id"].is_string()) {
                    return self.deletion_operation(operation.clone());
                }
                let label = self
                    .rows()
                    .get(index)
                    .map(|r| r.label.clone())
                    .unwrap_or_default();
                match label.as_str() {
                    "Details" => self.show_usage(&a),
                    "Limits" => {
                        return self.preview(
                            "Account limits",
                            strings(&["account", "limits", &key]),
                            Purpose::Limits(key),
                        );
                    }
                    "Rename" | "Rename ⚠"
                        if self.provider_cap(&self.provider_of(&key), "account_rename") =>
                    {
                        if self.rename_backend_ui()
                            && a.details
                                .get("backend_rename")
                                .is_some_and(Value::is_object)
                        {
                            self.show_rename_backend(&a);
                            return vec![];
                        }
                        let current = a
                            .details
                            .get("display_name")
                            .and_then(Value::as_str)
                            .map(clean)
                            .unwrap_or_default();
                        self.show(
                            "Rename account",
                            vec![format!(
                                "Display name for {}. Leave empty to use the account name.",
                                account_label(&key)
                            )],
                            &["Rename", "Cancel"],
                            Form::AccountRename(key.clone()),
                        );
                        self.dialog
                            .as_mut()
                            .unwrap()
                            .fields
                            .push(Field::new("Display name", current));
                    }
                    "Log in" => return self.begin_login(&key),
                    "Log out" if self.provider_cap(&self.provider_of(&key), "account_logout") => {
                        self.confirm(
                            "Log out",
                            vec![format!(
                                "Sign out of {}? Sessions keep their history; you can log in again later.",
                                account_label(&key)
                            )],
                            strings(&["account", "logout", &key]),
                        );
                    }
                    "Enable" | "Disable" => {
                        if a.details.get("enabled").is_some_and(|v| v == false) {
                            return self.mutate(strings(&["account", "enable", &key]));
                        } else {
                            self.confirm("Disable account",vec![format!("Disable {key} for future selection? Existing agents keep running.")],strings(&["account","disable",&key]));
                        }
                    }
                    "Deregister account" | "Restore account" => {
                        if a.details.get("membership").is_some_and(|v| v == "removed") {
                            return self.mutate(strings(&["account", "restore", &key]));
                        } else {
                            return self.preview(
                                "Deregister account",
                                strings(&["account", "remove", &key]),
                                Purpose::Remove(key),
                            );
                        }
                    }
                    "Delete account" => {
                        return self.preview(
                            "Delete account",
                            strings(&["account", "delete", &key]),
                            Purpose::Delete(key),
                        );
                    }
                    _ => {}
                }
            }
            Page::Sessions => {
                if !self.archives.is_empty() && index == self.rows().len().saturating_sub(2) {
                    self.enter(Page::Archives);
                    return self.refresh();
                }
                if let Some(session) = crate::session_list::session_index(&self.sessions, index)
                    .and_then(|i| self.sessions.get(i))
                {
                    self.enter(Page::Session(id(session).into()));
                } else {
                    return self.open_queue();
                }
                return self.refresh();
            }
            Page::Archives => {
                if let Some(s) = self.archives.get(index) {
                    self.enter(Page::Session(id(s).to_owned()));
                }
            }
            Page::Spaces(_) => {
                if index == 0 {
                    return self.new_space_dialog();
                }
                if let Some(space) = self.space_choices["spaces"]
                    .as_array()
                    .and_then(|spaces| spaces.get(index - 1))
                {
                    if space["current"] == true {
                        return vec![];
                    }
                    let target =
                        SpaceTarget::Existing(space["id"].as_str().unwrap_or("").to_owned());
                    let label = space["name"]
                        .as_str()
                        .unwrap_or("the selected Space")
                        .to_owned();
                    return self.choose_space(target, &label);
                }
            }
            Page::Session(key) => {
                let Some(s) = self.session(&key).cloned() else {
                    return vec![];
                };
                let action =
                    crate::session_list::actions(&s, self.capabilities["resume"]["version"] == 1)
                        .get(index)
                        .map(|(_, a)| *a);
                let action = action.unwrap_or(crate::session_list::Action::Unavailable);
                // A busy session queues the action; the backend checks it when it starts.
                if s["availability"] == "running"
                    && !crate::session_list::is_native_provider(&s)
                    && !s["pending_operation_id"].is_string()
                    && !matches!(
                        action,
                        crate::session_list::Action::Focus
                            | crate::session_list::Action::AddToHerdr
                            | crate::session_list::Action::Details
                            | crate::session_list::Action::Unavailable
                    )
                {
                    self.show(
                        "Checking session",
                        vec!["Checking the current session state…".into()],
                        &["Cancel"],
                        Form::Loading,
                    );
                    let intents =
                        self.request(Self::target(&s, "observe"), Purpose::ActionCheck(s, action));
                    self.foreground = Some(self.sequence);
                    return intents;
                }
                return self.session_action(s, action);
            }

            Page::Operations => {
                if index == 0 {
                    self.filter_grab = Some(self.operation_filter);
                    return vec![];
                }
                if let Some((kind, key)) = self.operation_keys().get(index).cloned().flatten() {
                    if kind == "operation" {
                        self.enter(Page::Operation(key));
                    } else {
                        self.enter(Page::Launch(
                            if kind == "launch" {
                                "launch"
                            } else {
                                "resume-launch"
                            },
                            key,
                        ));
                    }
                }
                return self.refresh();
            }
            Page::Launch(kind, key) => {
                if index == 0 {
                    self.details_open = true;
                    return self.request(
                        strings(&[kind, "status", "--request-id", &key]),
                        Purpose::Launch(kind, key),
                    );
                }
                if let Some(op) = self
                    .launch_record(kind, &key)
                    .and_then(|r| r["operation_id"].as_str())
                    .map(str::to_owned)
                {
                    self.enter(Page::Operation(op));
                    return self.refresh();
                }
            }
            Page::Operation(key) => {
                let Some(o) = self.operations.iter().find(|o| id(o) == key).cloned() else {
                    return vec![];
                };
                match index {
                    0 => {
                        self.details_open = true;
                        let dismissed = self.dismissed.contains(&key);
                        self.show(
                            &kind_label(o["kind"].as_str().unwrap_or("operation")),
                            Self::operation_body(&o, &key),
                            &["Continue in background"],
                            Form::Operation(key.clone()),
                        );
                        if let Some(d) = &mut self.dialog {
                            d.dim_from = Some(d.body.len().saturating_sub(2));
                            d.hanging = true;
                            if terminal(o["phase"].as_str().unwrap_or("")) {
                                finish_operation(d, dismissed, retry_state(&o));
                            }
                        }
                    }
                    1 | 2 => self.confirm(
                        "Operation confirmation",
                        vec![
                            o["detail"]
                                .as_str()
                                .unwrap_or("Review the pending operation")
                                .into(),
                        ],
                        strings(&[
                            "respond",
                            &key,
                            if index == 1 { "yes" } else { "no" },
                            "--sequence",
                            &o["confirmation_seq"].to_string(),
                        ]),
                    ),
                    4 | 5 => self.confirm(
                        if index == 4 {
                            "Cancel operation"
                        } else {
                            "Recover operation"
                        },
                        vec![
                            "The backend rechecks whether this operation can safely proceed."
                                .into(),
                        ],
                        strings(&[
                            if index == 4 {
                                "cancel-operation"
                            } else {
                                "recover"
                            },
                            &key,
                        ]),
                    ),
                    _ => {}
                }
            }
        }
        vec![]
    }
    /// `--herdr-session`/`--herdr-socket` for the UI's own HERDR server.
    fn host_args(&self, socket: bool) -> Option<Vec<String>> {
        let session = self.host["session"].as_str().filter(|v| !v.is_empty())?;
        let mut args = strings(&["--herdr-session", session]);
        if let Some(path) = self.host["socket"]
            .as_str()
            .filter(|v| socket && !v.is_empty())
        {
            args.extend(strings(&["--herdr-socket", path]));
        }
        Some(args)
    }
    /// Spaces on the UI's own HERDR server for a session that is not in HERDR.
    fn place_spaces(&mut self, s: Value, place: Place) -> Vec<Intent> {
        let Some(host) = self.host_args(true) else {
            self.note = "The HERDR instance could not be identified.".into();
            return vec![];
        };
        let mut args = Self::target(&s, "spaces");
        args.extend(host);
        self.preview("Choose Space", args, Purpose::Placement(s, place))
    }
    /// Stopped Claude sessions choose a Space and name when the backend can place them.
    fn start_route(&mut self, value: Value) -> Vec<Intent> {
        if self.capabilities["resume_placement"]["version"] == 1
            && crate::session_list::is_native_provider(&value)
        {
            return self.place_spaces(value, Place::Start);
        }
        vec![Intent::Resume(value)]
    }
    fn progress(&mut self, args: Vec<String>, purpose: Purpose) -> Vec<Intent> {
        self.session_return = args
            .windows(2)
            .find(|w| w[0] == "--session")
            .map(|w| w[1].clone());
        self.details_open = false;
        let mut progress = Dialog::progress("Operation progress");
        progress.size = self.dialog.as_ref().and_then(|d| d.size);
        self.dialog = Some(progress);
        self.form = Form::Progress;
        let request = self.request(args, purpose);
        self.foreground = Some(self.sequence);
        request
    }
    /// Return to the session list, keeping the session that was acted on selected.
    fn leave_to_list(&mut self) {
        let key = self.session_return.take();
        if let Some(index) = self
            .history
            .iter()
            .rposition(|location| location.page == Page::Sessions)
        {
            self.history.truncate(index);
        }
        self.go(Page::Sessions);
        if let Some(key) = key {
            self.options.selected = self
                .sessions
                .iter()
                .position(|s| id(s) == key)
                .map(|i| crate::session_list::session_row(&self.sessions, i))
                .unwrap_or(0);
        }
    }
    /// A merged entry is dead: refresh and select the session it merged into.
    fn merged(&mut self, survivor: String) -> Vec<Intent> {
        self.dialog = None;
        self.form = Form::None;
        self.foreground = None;
        self.details_open = false;
        if matches!(
            self.page,
            Page::Sessions | Page::Session(_) | Page::Spaces(_)
        ) {
            if let Some(index) = self
                .history
                .iter()
                .rposition(|location| location.page == Page::Sessions)
            {
                self.history.truncate(index);
            }
            self.go(Page::Sessions);
        }
        self.merge_target = Some((survivor, 2));
        self.note = "That session was merged into another entry; refreshing the list.".into();
        self.refresh()
    }
    /// Report a failure that happened outside a tracked request (for example focus).
    pub fn with_host(host: Value) -> Self {
        Self {
            host,
            ..Self::default()
        }
    }
    pub fn failure(&mut self, error: String) -> Vec<Intent> {
        match merged_survivor(&error) {
            Some(survivor) => self.merged(survivor),
            None => {
                self.note = error;
                vec![]
            }
        }
    }
    fn session_action(&mut self, s: Value, action: crate::session_list::Action) -> Vec<Intent> {
        let key = id(&s).to_owned();
        match action {
            crate::session_list::Action::Archive
            | crate::session_list::Action::Delete
            | crate::session_list::Action::Restore => {
                let (command, title, text) = match action {
                    crate::session_list::Action::Archive
                        if crate::session_list::is_native_provider(&s) =>
                    {
                        (
                            "archive",
                            "Archive session",
                            "Move this conversation's saved files to sessionsation-managed Archived Sessions; Restore brings them back. No agent is started; project files and account credentials stay in place.",
                        )
                    }
                    crate::session_list::Action::Archive => (
                        "archive",
                        "Archive session",
                        "Compress and move saved conversation history to sessionsation-managed Archived Sessions. No agent is started; project files and account credentials stay in place.",
                    ),
                    crate::session_list::Action::Restore => (
                        "restore-session",
                        "Restore session",
                        "Restore the saved conversation for later resume. This does not start an agent or send a message.",
                    ),
                    _ => (
                        "delete-session",
                        "Delete session",
                        "Permanently delete this conversation and its archived data. This cannot be undone. Project files and account credentials are unaffected.",
                    ),
                };
                let name = s["name"].as_str().unwrap_or("").to_owned();
                let mut body = vec![text.into()];
                if command == "delete-session" {
                    body.push(format!("Type {name} to confirm."));
                }
                self.show(
                    title,
                    body,
                    &["Confirm", "Cancel"],
                    Form::SessionMutation(s, command),
                );
                if command == "delete-session" {
                    self.dialog
                        .as_mut()
                        .unwrap()
                        .fields
                        .push(Field::new("Session name", "").validated(Rule::Exact(name)));
                }
            }
            crate::session_list::Action::Focus => return vec![Intent::Focus(s)],
            crate::session_list::Action::Details => {
                self.show(
                    "Session details",
                    Self::session_detail_lines(&s),
                    &["Close"],
                    Form::SessionDetails(key),
                );
                if let Some(d) = &mut self.dialog {
                    Self::details_layout(d);
                }
                return self.refresh_sessions();
            }
            crate::session_list::Action::Rename => {
                let claude = crate::session_list::is_native_provider(&s);
                self.show(
                    "Rename session",
                    vec![if claude {
                        if crate::session_list::is_gemini(&s) {
                            "Changes the Sessionsation name only. Gemini CLI has no native rename."
                        } else if crate::session_list::is_antigravity(&s) {
                            "Changes the Sessionsation name only."
                        } else {
                            "Changes the Sessionsation name only. Claude Code's own title is applied the next time this session is launched or resumed."
                        }
                    } else {
                        "Keep the same session and conversation identities."
                    }
                    .into()],
                    &["Rename", "Cancel"],
                    Form::Rename(s.clone()),
                );
                self.dialog.as_mut().unwrap().fields.push(
                    Field::new(
                        if claude { "Sessionsation name" } else { "Name" },
                        s["name"].as_str().unwrap_or(""),
                    )
                    .validated(Rule::Rename(s["name"].as_str().unwrap_or("").into())),
                );
                self.validate_dialog_names();
            }

            crate::session_list::Action::MoveProject
            | crate::session_list::Action::ChangeDirectory => {
                let moving = action == crate::session_list::Action::MoveProject;
                self.show(if moving { "Move project folder" } else { "Change working directory" }, vec![if moving { "Enter a new destination for the project folder. Review the plan before moving files." } else { "Enter an existing directory. Only this session changes directory; project files stay in place." }.into()], &["Review", "Cancel"], Form::Relocation(s.clone(), moving));
                self.dialog.as_mut().unwrap().fields.push(Field::new(
                    "Destination",
                    s["directory"].as_str().unwrap_or(""),
                ));
            }
            crate::session_list::Action::MoveSpace => {
                return self.preview(
                    "Choose Space",
                    Self::target(&s, "spaces"),
                    Purpose::Spaces(s),
                );
            }
            crate::session_list::Action::AddToHerdr => return self.place_spaces(s, Place::Add),
            crate::session_list::Action::Switch => return vec![Intent::Switch(s)],
            crate::session_list::Action::Resume => {
                return self.preview(
                    "Resume session",
                    Self::target(&s, "session"),
                    Purpose::Resume(key),
                );
            }
            crate::session_list::Action::Restart | crate::session_list::Action::Stop => {
                let command = if action == crate::session_list::Action::Restart {
                    "restart"
                } else {
                    "stop"
                };
                let name = s["name"]
                    .as_str()
                    .map(clean)
                    .unwrap_or_else(|| "this session".into());
                self.confirm(
                    if command == "restart" {
                        "Restart Codex"
                    } else {
                        "End session"
                    },
                    vec![if command == "restart" {
                        format!("Restart Codex for {name}? Conversation history is retained.")
                    } else {
                        format!("End {name}? Conversation history is retained.")
                    }],
                    Self::target(&s, command),
                );
            }
            _ => {}
        }
        vec![]
    }
    fn action_checked(
        &mut self,
        s: Value,
        action: crate::session_list::Action,
        observation: Option<Value>,
    ) -> Vec<Intent> {
        if self.page != Page::Session(id(&s).into()) {
            return vec![];
        }
        if self
            .session(id(&s))
            .is_none_or(|current| current["generation"] != s["generation"])
        {
            self.dismiss();
            self.note = "The session changed while checking it. Choose the action again.".into();
            return vec![];
        }
        self.dialog = None;
        self.foreground = None;
        self.form = Form::None;
        let verified = observation
            .as_ref()
            .filter(|o| o["generation"] == s["generation"] && o["freshness"] == "fresh");
        let state = verified.and_then(|o| o["runtime_state"].as_str().or(o["state"].as_str()));
        let warning = match state {
            Some("errored") => Some(
                "The last turn ended with a provider error, such as a usage limit or unavailable model. Continue with this session action? Saved history is retained; the backend will recheck that it is safe before changing the session.",
            ),
            Some("interrupted" | "stopped") => Some(
                "This session stopped before normal completion. Continue with this action? Review any unfinished work; the backend will recheck the session before proceeding.",
            ),
            Some("idle" | "working") => None,
            _ => Some(
                "The current provider state could not be verified. Continue to configure this action? The backend must verify a safe state before stopping or switching the session; this warning does not bypass that check.",
            ),
        };
        if let Some(message) = warning {
            self.show(
                "Session needs attention",
                vec![message.into()],
                &["Confirm", "Cancel"],
                Form::ActionWarning(s, action),
            );
            vec![]
        } else {
            self.session_action(s, action)
        }
    }
    fn submit(&mut self, button: usize, values: Vec<String>) -> Vec<Intent> {
        if let Form::OperationApproval(key, sequence) = self.form.clone() {
            return self.mutate(strings(&[
                "respond",
                &key,
                if button == 0 { "yes" } else { "no" },
                "--sequence",
                &sequence.to_string(),
            ]));
        }
        if let Form::LoginLink(key) = self.form.clone() {
            let seen = self.login_link.as_ref().is_some_and(|s| s.seen);
            self.login_link = None;
            if seen && button == 0 {
                self.show_login(&key);
            } else {
                self.dismiss();
            }
            return vec![];
        }
        if let Form::Queued(key) = self.form.clone() {
            self.dismiss();
            if button == 0 {
                self.cancel_queued(&key);
            }
            return vec![];
        }
        if let Form::QueuedFailed(key) = self.form.clone() {
            self.dismiss();
            if button == 0 {
                self.dismiss_queued(&key);
            }
            return vec![];
        }
        if let Form::Operation(key) = self.form.clone() {
            match self
                .dialog
                .as_ref()
                .and_then(|d| d.buttons.get(button))
                .map(|b| b.label.as_str())
            {
                Some("Dismiss") => self.dismiss_operation(&key),
                Some("Undismiss") => self.undismiss_operation(&key),
                // Retry is the backend's own recover, confirmed first; never automatic.
                Some("Retry") => {
                    self.dismiss();
                    self.confirm(
                        "Retry operation",
                        vec![
                            "The backend rechecks whether this operation can safely proceed."
                                .into(),
                        ],
                        strings(&["recover", &key]),
                    );
                    return vec![];
                }
                _ => {}
            }
            self.dismiss();
            return vec![];
        }
        if let Form::GeminiAuth(key, choices) = self.form.clone() {
            self.dialog = None;
            self.form = Form::None;
            match choices.get(button) {
                Some(c) if c.available && c.mode == "google" => {
                    return self.mutate(strings(&["account", "auth", &key, "--mode", "google"]));
                }
                Some(c) if c.available && c.mode == "api_key" => self.show_gemini_key(&key),
                _ => {}
            }
            return vec![];
        }
        if let Form::GeminiKey(key) = self.form.clone() {
            if button != 0 {
                self.dialog = None;
                self.form = Form::None;
                return vec![];
            }
            let secret = std::mem::take(&mut self.dialog.as_mut().unwrap().fields[0].input.value);
            return self.submit_gemini_key(&key, secret);
        }
        if let Form::AddProvider(providers) = self.form.clone() {
            self.dialog = None;
            self.form = Form::None;
            if let Some(provider) = providers.get(button) {
                self.add_for(provider);
            }
            return vec![];
        }
        if let Form::AccountRenameBackend(key, current, _) = self.form.clone() {
            let enabled = self
                .dialog
                .as_ref()
                .and_then(|d| d.buttons.get(button))
                .is_some_and(|b| b.enabled);
            if button > 1 || !enabled {
                if button > 1 {
                    self.dismiss();
                }
                return vec![];
            }
            let text: String = values
                .first()
                .map_or("", String::as_str)
                .trim()
                .chars()
                .take(64)
                .collect();
            let generation = self
                .account(&key)
                .and_then(|a| a.details.get("generation"))
                .map(|g| g.as_str().map_or_else(|| g.to_string(), str::to_owned))
                .unwrap_or_default();
            let mut args = strings(&[
                "account",
                if button == 0 {
                    "rename-backend"
                } else {
                    "rename"
                },
                &key,
            ]);
            if button == 0 {
                if !text.is_empty() && text != current {
                    args.push(text);
                }
            } else {
                args.extend(strings(&["--display-name", &text]));
            }
            args.extend(strings(&["--expected-generation", &generation]));
            return self.mutate(args);
        }
        if button != 0 {
            self.dialog = None;
            self.form = Form::None;
            return vec![];
        }
        let form = self.form.clone();
        let value = |i: usize| values.get(i).map(String::as_str).unwrap_or("");
        let args = match form {
            Form::None
            | Form::Progress
            | Form::Operation(_)
            | Form::OperationApproval(..)
            | Form::Queued(_)
            | Form::QueuedFailed(_)
            | Form::LoginWait(_)
            | Form::LoginLink(_)
            | Form::SessionDetails(_) => {
                self.dismiss();
                return vec![];
            }
            Form::Loading => {
                self.dismiss();
                return vec![];
            }
            Form::ActionWarning(s, action) => {
                self.dialog = None;
                self.form = Form::None;
                if self
                    .session(id(&s))
                    .is_none_or(|current| current["generation"] != s["generation"])
                {
                    self.note = "The session changed. Choose the action again.".into();
                    return vec![];
                }
                return self.session_action(s, action);
            }
            Form::NewSpace(s, place) => {
                let label = value(0).to_owned();
                let taken = |s: &Value| s["name"] == label.as_str();
                if !crate::draft::valid_name(&label)
                    || self.space_choices["spaces"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(taken))
                {
                    return vec![];
                }
                // Same session and generation as when the Spaces were loaded.
                self.space_session = s;
                self.place = place;
                return self.choose_space(SpaceTarget::New(label.clone()), &format!("“{label}”"));
            }
            Form::Placement(s, space, place) => {
                let mut args = match place {
                    Place::Add => strings(&["session", "add-to-herdr"]),
                    Place::Start => strings(&["resume"]),
                };
                args.extend(strings(&[
                    "--session",
                    id(&s),
                    "--expected-generation",
                    s["generation"].as_str().unwrap_or(""),
                ]));
                let Some(host) = self.host_args(place == Place::Add) else {
                    self.dialog.as_mut().unwrap().status =
                        Some("The HERDR instance could not be identified.".into());
                    return vec![];
                };
                args.extend(host);
                args.extend(match &space {
                    SpaceTarget::Existing(id) => strings(&["--space", id]),
                    SpaceTarget::New(label) => strings(&["--new-space", label]),
                });
                args.extend(strings(&["--name", value(0)]));
                let purpose = match place {
                    Place::Add => Purpose::AddToHerdr(s),
                    Place::Start => Purpose::Start(s),
                };
                return self.progress(args, purpose);
            }
            Form::Added(done) => {
                self.dismiss();
                return vec![Intent::Focus(
                    json!({"id": done["session_id"], "generation": done["generation"]}),
                )];
            }
            Form::Confirm(args) => args,
            Form::AddProvider(_) | Form::GeminiAuth(..) | Form::GeminiKey(_) => return vec![],
            Form::Add(provider) => {
                let name = value(0);
                if name.is_empty()
                    || name.len() > NAME_MAX_CHARS
                    || !name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                {
                    self.dialog.as_mut().unwrap().body = vec![format!(
                        "Use 1–{NAME_MAX_CHARS} letters, digits, hyphens or underscores."
                    )];
                    return vec![];
                }
                strings(&["account", "add", &format!("{provider}:{name}")])
            }
            Form::Login(key) => strings(&["account", "login", &key]),
            Form::Limits(key) => {
                let parsed = values
                    .iter()
                    .map(|v| {
                        v.parse::<f64>()
                            .ok()
                            .filter(|n| n.is_finite() && (0. ..=100.).contains(n))
                    })
                    .collect::<Option<Vec<_>>>();
                let Some(p) = parsed.filter(|p| p.len() == 2) else {
                    self.dialog.as_mut().unwrap().body =
                        vec!["Enter remaining percentages from 0 through 100.".into()];
                    return vec![];
                };
                strings(&[
                    "account",
                    "limits",
                    &key,
                    "--short-used",
                    &(100. - p[0]).to_string(),
                    "--weekly-used",
                    &(100. - p[1]).to_string(),
                ])
            }
            Form::ResumeWarning(mut session) => {
                session["confirm_unfinished"] = json!(true);
                self.dismiss();
                return self.start_route(session);
            }
            Form::Delete(preview) => {
                let key = preview["account_id"].as_str().unwrap_or("");
                if value(0) != account_label(key) {
                    self.dialog.as_mut().unwrap().status =
                        Some(format!("Type {} to confirm.", account_label(key)));
                    return vec![];
                }
                strings(&[
                    "account",
                    "delete",
                    key,
                    "--confirm",
                    key,
                    "--request-id",
                    &uuid::Uuid::new_v4().to_string(),
                ])
            }
            Form::Remove(preview) => {
                let key = preview["account_id"].as_str().unwrap_or("");
                if value(0) != account_label(key) {
                    self.dialog.as_mut().unwrap().status =
                        Some(format!("Type {} to confirm.", account_label(key)));
                    return vec![];
                }
                strings(&[
                    "account",
                    "remove",
                    key,
                    "--confirm",
                    key,
                    "--revision",
                    preview["revision"].as_str().unwrap_or(""),
                ])
            }
            Form::Relocation(s, moving) => {
                let mut args = Self::target(&s, "relocate");
                args.extend(strings(&["--to", value(0)]));
                if moving {
                    args.push("--move-files".into());
                }
                let mut preview = args.clone();
                preview.push("--dry-run".into());
                return self.preview(
                    "Review directory change",
                    preview,
                    Purpose::Relocation(args),
                );
            }
            Form::AccountRenameBackend(..) => return vec![],
            Form::AccountRename(key) => {
                let name: String = value(0).trim().chars().take(64).collect();
                strings(&["account", "rename", &key, "--display-name", &name])
            }
            Form::Rename(s) => {
                let mut args = Self::target(&s, "rename");
                args.extend(strings(&["--name", value(0)]));
                args
            }
            Form::SessionMutation(s, command) => {
                if command == "delete-session" && value(0) != s["name"].as_str().unwrap_or("") {
                    return vec![];
                }
                let mut args = Self::target(&s, command);
                args.extend(strings(&[
                    "--request-id",
                    &uuid::Uuid::new_v4().to_string(),
                    "--confirm",
                    id(&s),
                ]));
                args
            }
        };
        self.mutate(args)
    }
    pub fn response(&mut self, token: u64, result: Result<Value, String>) -> Vec<Intent> {
        let Some(purpose) = self.pending.remove(&token) else {
            return vec![];
        };
        if matches!(purpose, Purpose::Mutation) {
            self.direct_answered(token, &result);
        }
        let cached_req = self.cached_tokens.remove(&token);
        if let Purpose::LoginPoll(key) = purpose {
            if let Ok(value) = result {
                self.login_polled(key, value);
            }
            return vec![];
        }
        if matches!(
            purpose,
            Purpose::Remove(_)
                | Purpose::Delete(_)
                | Purpose::Limits(_)
                | Purpose::Resume(_)
                | Purpose::Spaces(_)
                | Purpose::Placement(..)
                | Purpose::Relocation(_)
                | Purpose::ActionCheck(..)
        ) && (self.foreground != Some(token) || !matches!(self.form, Form::Loading))
        {
            return vec![];
        }
        if let Err(error) = &result
            && let Some(survivor) = merged_survivor(error)
        {
            return self.merged(survivor);
        }
        if let Purpose::ActionCheck(session, action) = purpose {
            return self.action_checked(session, action, result.ok());
        }
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                if let Purpose::Reorder(original) = &purpose {
                    // Back to the backend's order, the moved account still selected.
                    let moved = self.selected_account();
                    self.accounts = original.clone();
                    if let Some(i) =
                        moved.and_then(|key| self.accounts.iter().position(|a| a.id == key))
                    {
                        self.options.selected = 5 + i;
                    }
                    self.note = e.clone();
                    return vec![];
                }
                if let Purpose::LiveSteering(key) = &purpose {
                    // A failed read is shown on the row, not as a page note.
                    self.steering.remove(key);
                    self.steering_unavailable
                        .insert(key.clone(), steering_reason(&e));
                    return vec![];
                }
                if let Purpose::BypassWarning(key) = &purpose {
                    self.bypass.remove(key);
                    self.bypass_unavailable
                        .insert(key.clone(), steering_reason(&e));
                    return vec![];
                }
                if let [a, b, key, ..] = &self.mutation[..]
                    && a == "account"
                    && b == "bypass-warning"
                    && e.starts_with("SETTINGS_IS_SYMLINK")
                {
                    self.bypass_unavailable
                        .insert(key.clone(), steering_reason(&e));
                }
                if let [a, b, key, ..] = &self.mutation[..]
                    && a == "account"
                    && b == "live-steering"
                    && e.starts_with("SETTINGS_IS_SYMLINK")
                {
                    self.steering_unavailable
                        .insert(key.clone(), steering_reason(&e));
                }
                if matches!(purpose, Purpose::State) {
                    for session in &mut self.sessions {
                        session["activity"] = json!("unknown");
                    }
                }
                // A background read on an outdated generation is not the user's
                // action going stale: read the session again instead of a notice.
                if matches!(purpose, Purpose::Observe(_)) && stale_error(&e) {
                    return self.refresh_sessions();
                }
                self.note = e.clone();
                if self.foreground == Some(token)
                    && matches!(self.form, Form::Loading | Form::Progress)
                {
                    self.dialog = None;
                    self.form = Form::None;
                    self.foreground = None;
                    self.details_open = false;
                }
                return vec![];
            }
        };
        match purpose {
            Purpose::ActionCheck(..) | Purpose::LoginPoll(_) => unreachable!(),
            Purpose::AuthKey(key) => {
                let shown = matches!(self.form, Form::Progress) && self.foreground == Some(token);
                if shown {
                    self.foreground = None;
                }
                self.gemini_key_checked(&key, &value, shown);
                return self.refresh();
            }
            Purpose::Reorder(_) => {}
            Purpose::DeleteAvailability(key) => {
                self.deletion_previews.insert(key, value);
            }
            Purpose::LiveSteering(key) => {
                // A symlinked settings file is only reported by a change attempt.
                if self
                    .steering_unavailable
                    .get(&key)
                    .is_some_and(|r| r != SYMLINKED)
                {
                    self.steering_unavailable.remove(&key);
                }
                self.steering.insert(key, value);
            }
            Purpose::BypassWarning(key) => {
                if self
                    .bypass_unavailable
                    .get(&key)
                    .is_some_and(|r| r != SYMLINKED)
                {
                    self.bypass_unavailable.remove(&key);
                }
                self.bypass.insert(key, value);
            }
            Purpose::Archives => {
                self.archives = value.as_array().cloned().unwrap_or_default();
                self.derive_session_fields();
                self.options.selected = self
                    .options
                    .selected
                    .min(self.rows().len().saturating_sub(1));
                if self.page == Page::Archives && self.archives.is_empty() {
                    self.note = "No archived sessions".into();
                }
            }
            Purpose::State => {
                let mut value = value;
                // The cursor still on the first row of a cached list was never
                // moved: keep it on the first row when the live order differs.
                let untouched = self.cached_snapshot
                    && self.page == Page::Sessions
                    && self.options.selected
                        == self.rows().iter().position(|r| !r.separator).unwrap_or(0);
                if cached_req {
                    // A late cheap read must never replace newer full state.
                    if self.sessions_known && !self.cached_snapshot && token < self.last_full_token
                    {
                        return vec![];
                    }
                    if value["snapshot"]["fresh"] == false && !self.sessions_known {
                        if let Some(list) = value["sessions"].as_array_mut() {
                            for s in list {
                                if let Some(o) = s.as_object_mut() {
                                    o.remove("generation");
                                    o.insert("capabilities".into(), json!({}));
                                }
                            }
                        }
                        self.cached_snapshot = true;
                    }
                } else {
                    self.last_full_token = token;
                    self.cached_snapshot = false;
                    self.cache_out = Some(value["sessions"].clone());
                }
                let initial_sessions = self.page == Page::Sessions && !self.sessions_known;
                let archives_selected = self.sessions_known
                    && self.page == Page::Sessions
                    && !self.archives.is_empty()
                    && self.options.selected == self.rows().len().saturating_sub(2);
                let operations_selected = self.sessions_known
                    && self.page == Page::Sessions
                    && self.options.selected == self.rows().len().saturating_sub(1);
                let chosen = if self.page == Page::Sessions && self.sessions_known {
                    crate::session_list::session_index(&self.sessions, self.options.selected)
                        .and_then(|i| self.sessions.get(i))
                        .map(|s| id(s).to_owned())
                } else {
                    None
                };
                self.sessions = value["sessions"].as_array().cloned().unwrap_or_default();
                self.sessions_known = true;
                self.derive_session_fields();
                self.validate_dialog_names();
                sort_sessions(&mut self.sessions);
                if initial_sessions {
                    self.options = Options::default();
                }
                let chosen = chosen.filter(|_| !untouched);
                if untouched {
                    self.options.selected =
                        self.rows().iter().position(|r| !r.separator).unwrap_or(0);
                }
                if let Some(key) = chosen {
                    self.options.selected = self
                        .sessions
                        .iter()
                        .position(|s| id(s) == key)
                        .map(|i| crate::session_list::session_row(&self.sessions, i))
                        .unwrap_or(0);
                }
                if archives_selected {
                    self.options.selected = self.rows().len().saturating_sub(2);
                }
                if operations_selected {
                    self.options.selected = self.rows().len().saturating_sub(1);
                }
                if let Some((target, tries)) = self.merge_target.take() {
                    if let Some(index) = self.sessions.iter().position(|s| id(s) == target) {
                        if self.page == Page::Sessions {
                            self.options.selected =
                                crate::session_list::session_row(&self.sessions, index);
                            self.note = "Showing the session it was merged into.".into();
                        }
                    } else if tries > 1 {
                        self.merge_target = Some((target, tries - 1));
                    }
                }
                self.capabilities = value["capabilities"].clone();
                if let Form::SessionDetails(key) = &self.form
                    && let Some(session) = self.session(key)
                {
                    let body = Self::session_detail_lines(session);
                    if let Some(d) = &mut self.dialog {
                        d.body = body;
                        Self::details_layout(d);
                    }
                }
            }
            Purpose::Launches(kind) => {
                let chosen = (self.page == Page::Operations)
                    .then(|| self.operation_keys().get(self.options.selected).cloned())
                    .flatten()
                    .flatten();
                self.launches
                    .insert(kind, value.as_array().cloned().unwrap_or_default());
                self.retain_operation_focus(chosen);
            }
            Purpose::Launch(kind, key) => {
                if self.page == Page::Launch(kind, key.clone()) && self.details_open {
                    let body = vec![
                        format!("Request: {key}"),
                        format!("Status: {}", launch_status(&value)),
                        value["detail"].as_str().unwrap_or("").into(),
                        "Closing this view does not stop or resend the request.".into(),
                    ];
                    if self.dialog.is_none() {
                        self.show(
                            "Launch progress",
                            body,
                            &["Continue in background"],
                            Form::Progress,
                        );
                    } else if matches!(self.form, Form::Progress)
                        && let Some(d) = &mut self.dialog
                    {
                        d.body = body;
                    }
                    if (terminal(value["phase"].as_str().unwrap_or(""))
                        || value["phase"] == "running")
                        && let Some(d) = &mut self.dialog
                    {
                        d.finish();
                    }
                }
            }
            Purpose::Operations => {
                let chosen = (self.page == Page::Operations)
                    .then(|| self.operation_keys().get(self.options.selected).cloned())
                    .flatten()
                    .flatten();
                let was_running = crate::op_queue::backend_running(&self.operations);
                self.operations = value.as_array().cloned().unwrap_or_default();
                // An operation that finished since the last read changed its
                // session's generation: read the sessions again.
                let finished =
                    !was_running.is_subset(&crate::op_queue::backend_running(&self.operations));
                // Forget dismissals of operations the backend no longer lists.
                let known = self.dismissed.len();
                self.dismissed
                    .retain(|k| self.operations.iter().any(|o| id(o) == k));
                if self.dismissed.len() != known {
                    self.dismissed_out = Some(self.dismissed.iter().cloned().collect());
                }
                self.retain_operation_focus(chosen);
                let viewed = match &self.form {
                    Form::Operation(key) | Form::OperationApproval(key, _) => Some(key.clone()),
                    _ => None,
                };
                if let Some(key) = viewed
                    && let Some(o) = self.operations.iter().find(|o| id(o) == key).cloned()
                {
                    if o["phase"] == "awaiting_background_approval"
                        && o["can_respond"] != false
                        && let Some(sequence) = o["confirmation_seq"].as_u64()
                    {
                        // Preserve button focus while polling the same consent. New
                        // generations of consent are bound to their own sequence.
                        if !matches!(&self.form, Form::OperationApproval(old, seq) if old == &key && *seq == sequence)
                        {
                            self.show("Stop background terminals?", vec![o["detail"].as_str().unwrap_or("This operation needs permission to stop background work.").into()], &["Stop and continue", "Cancel operation"], Form::OperationApproval(key, sequence));
                        }
                    } else if matches!(self.form, Form::OperationApproval(..)) {
                        self.show(
                            "Operation progress",
                            vec![],
                            &["Continue in background"],
                            Form::Operation(key),
                        );
                    }
                }
                if let Form::Operation(key) = &self.form
                    && let Some(o) = self.operations.iter().find(|o| id(o) == key)
                    && let Some(d) = &mut self.dialog
                {
                    d.body = Self::operation_body(o, key);
                    d.dim_from = Some(d.body.len().saturating_sub(2));
                    d.hanging = true;
                    d.title = kind_label(o["kind"].as_str().unwrap_or("operation"));
                    if terminal(o["phase"].as_str().unwrap_or("")) {
                        finish_operation(d, self.dismissed.contains(key), retry_state(o));
                    }
                }
                if let Some(key) = &self.active_operation
                    && let Some(o) = self.operations.iter().find(|o| id(o) == key)
                    && terminal(o["phase"].as_str().unwrap_or(""))
                {
                    self.active_operation = None;
                }
                if finished && !self.pending.values().any(|p| matches!(p, Purpose::State)) {
                    return self.refresh_sessions();
                }
            }
            Purpose::Observe(key) => {
                self.readiness.insert(key, value);
            }
            Purpose::Relocation(args) => {
                let mut body = vec![
                    value["detail"]
                        .as_str()
                        .unwrap_or("Review the directory change.")
                        .into(),
                    format!("From: {}", value["from"].as_str().unwrap_or("")),
                    format!("To: {}", value["to"].as_str().unwrap_or("")),
                ];
                if let Some(others) = value["other_sessions"].as_array().filter(|v| !v.is_empty()) {
                    body.push(format!(
                        "Other sessions using this folder: {}",
                        others
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                self.confirm("Confirm directory change", body, args);
            }
            Purpose::Spaces(session) => {
                if self.page == Page::Session(id(&session).into()) {
                    if value["session_id"] != session["id"]
                        || value["generation"] != session["generation"]
                    {
                        self.dismiss();
                        self.note = "Session changed while its Spaces were loading. Refresh and choose again.".into();
                        return vec![];
                    }
                    self.space_choices = value;
                    self.space_session = session.clone();
                    self.enter(Page::Spaces(id(&session).into()));
                    self.place = None;
                    self.preselect_space(None);
                }
            }
            Purpose::Placement(session, place) => {
                if self.page == Page::Session(id(&session).into()) {
                    if value["session_id"] != session["id"]
                        || value["generation"] != session["generation"]
                        || value["herdr_session"].as_str() != self.host["session"].as_str()
                    {
                        self.dismiss();
                        self.note = "Session changed while its Spaces were loading. Refresh and choose again.".into();
                        return vec![];
                    }
                    let original = &session["original_host"];
                    let remembered = (original["session"] == self.host["session"]
                        && original["socket"] == self.host["socket"])
                        .then(|| original["workspace_id"].as_str())
                        .flatten();
                    let preferred = remembered
                        .map(str::to_owned)
                        .or_else(|| self.focused_space.clone());
                    self.space_choices = value;
                    self.space_session = session.clone();
                    self.enter(Page::Spaces(id(&session).into()));
                    self.place = Some(place);
                    self.preselect_space(preferred);
                }
            }
            Purpose::AddToHerdr(session) => {
                let shown = matches!(self.form, Form::Progress) && self.foreground == Some(token);
                let attached = value["state"] == "attached";
                if shown {
                    self.leave_to_list();
                    let name = value["name"]
                        .as_str()
                        .or(session["name"].as_str())
                        .map(clean)
                        .unwrap_or_default();
                    if attached {
                        let focusable = value["generation"].is_string();
                        let buttons: &[&str] = if focusable {
                            &["Focus tab", "Close"]
                        } else {
                            &["Close"]
                        };
                        self.show(
                            "Added to HERDR",
                            vec![
                                format!("{name} is open in a new HERDR tab."),
                                "The existing session is only viewed there: no agent was started and no input was sent.".into(),
                            ],
                            buttons,
                            if focusable { Form::Added(value) } else { Form::None },
                        );
                    } else {
                        self.show(
                            "Attachment not confirmed",
                            vec![
                                format!("HERDR opened a tab for {name}, but the attachment could not be confirmed."),
                                "Nothing was retried and nothing will be retried automatically. Check the HERDR tab and the session list before trying again.".into(),
                            ],
                            &["Close"],
                            Form::None,
                        );
                    }
                } else {
                    self.note = if attached {
                        "Added to HERDR."
                    } else {
                        "Attachment not confirmed; nothing was retried. Check the session list."
                    }
                    .into();
                }
                return self.refresh();
            }
            Purpose::Start(session) => {
                if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                    self.leave_to_list();
                    self.show(
                        "Session start requested",
                        vec![
                            format!("{} was opened in a new HERDR tab.", session["name"].as_str().map(clean).unwrap_or_default()),
                            "Its confirmed state appears in Manage sessions after a refresh. Nothing is retried automatically.".into(),
                        ],
                        &["Close"],
                        Form::None,
                    );
                } else {
                    self.note = "Start requested; check the session list.".into();
                }
                return self.refresh();
            }
            Purpose::Resume(key) => {
                if self.page == Page::Session(key) {
                    self.dialog = None;
                    self.form = Form::None;
                    self.foreground = None;
                    if value["resume_block_reason"] == "unfinished_history" {
                        self.show("Unfinished conversation", vec!["This conversation ended during a turn. Start from its saved history? Review unfinished work before continuing; the interrupted request will not be replayed automatically.".into()], &["Start session", "Cancel"], Form::ResumeWarning(value));
                    } else {
                        return self.start_route(value);
                    }
                }
            }
            Purpose::Limits(key) => {
                if self.page != Page::Account(key.clone())
                    && !(self.usage_focus
                        && self.usage_selectable()
                        && self.selected_account().as_deref() == Some(&key))
                {
                    return vec![];
                }
                let limits = &value["limits"];
                self.show(
                    "Account limits",
                    vec![
                        "Minimum allowance to retain. Saving does not enable automatic switching."
                            .into(),
                    ],
                    &["Save limits", "Cancel"],
                    Form::Limits(key),
                );
                let d = self.dialog.as_mut().unwrap();
                d.fields = vec![
                    Field::new(
                        "5h minimum remaining (%)",
                        (100. - limits["short_percent"].as_f64().unwrap_or(90.)).to_string(),
                    )
                    .validated(Rule::Percentage),
                    Field::new(
                        "Weekly minimum remaining (%)",
                        (100. - limits["weekly_percent"].as_f64().unwrap_or(90.)).to_string(),
                    )
                    .validated(Rule::Percentage),
                ];
            }
            Purpose::Delete(key) => {
                if self.page != Page::Account(key.clone()) {
                    return vec![];
                }
                let expected = account_label(&key);
                let mut body = vec![format!(
                    "Delete {expected}'s local account files and credentials?"
                )];
                let semantics = backend_text(&value);
                if !semantics.is_empty() {
                    body.extend(semantics);
                    body.push("The upstream provider account is unchanged.".into());
                } else if matches!(
                    self.provider_of(&key).as_str(),
                    "claude" | "gemini" | "antigravity"
                ) {
                    body.push("A managed account home is removed and its conversation history is preserved. An adopted external directory is left in place. The upstream provider account is unchanged.".into());
                } else {
                    body.push("All saved and archived conversations will be preserved outside the account directory. The upstream provider account is unchanged.".into());
                }
                body.push("After confirmation, the CLI checks active sessions, preserves history and deletes local account data. Progress can continue in the background.".into());
                if value["can_delete"] != true {
                    body.push(if value["owner_upgrade_required"]==true{"The existing account owner needs an update before deletion is available.".into()}else{"This account has active sessions. Exit or stop them before deleting the account.".into()});
                    self.show("Account cannot be deleted", body, &["Close"], Form::None);
                } else {
                    self.show(
                        "Delete account",
                        body,
                        &["Delete account", "Cancel"],
                        Form::Delete(value),
                    );
                    self.dialog.as_mut().unwrap().fields.push(
                        Field::new(&format!("Type {expected} to confirm"), "")
                            .validated(Rule::Exact(expected)),
                    );
                }
            }
            Purpose::Remove(key) => {
                if self.page != Page::Account(key.clone()) {
                    return vec![];
                }
                let mut body = vec![
                    format!(
                        "Deregister {} from local selection? Credentials and history remain recoverable. The provider account is not deleted.",
                        account_label(&key)
                    ),
                    format!(
                        "Stored files: {}; bytes: {}; history files: {}",
                        value["stored_data"]["files"],
                        value["stored_data"]["bytes"],
                        value["stored_data"]["history_files"]
                    ),
                ];
                body.extend(backend_text(&value));
                if let Some(sessions) = value["affected_sessions"].as_array() {
                    for s in sessions {
                        body.push(format!(
                            "Affected: {} ({})",
                            s["name"].as_str().unwrap_or(""),
                            id(s)
                        ));
                    }
                }
                if value["can_remove"] != true {
                    body.push("Blocked by active/loaded conversations or pending operations. Stop or move them explicitly first.".into());
                    self.show("Deregistration blocked", body, &["Close"], Form::None);
                } else {
                    let expected = account_label(&key);
                    self.show(
                        "Deregister account",
                        body,
                        &["Deregister account", "Cancel"],
                        Form::Remove(value),
                    );
                    self.dialog.as_mut().unwrap().fields.push(
                        Field::new(&format!("Type {expected} to confirm"), "")
                            .validated(Rule::Exact(expected)),
                    );
                }
            }
            Purpose::Mutation => {
                if let (Some(new), Some(old)) = (
                    value["account_id"].as_str(),
                    value["previous_account_id"].as_str(),
                ) && new != old
                {
                    // The account id changed: follow it, drop per-id caches of the old id and
                    // re-read accounts and sessions (their generations changed).
                    self.rekey_account(old, new);
                    if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                        self.show(
                            "Saved",
                            vec![format!("Account renamed to {}.", account_label(new))],
                            &["Close"],
                            Form::None,
                        );
                    }
                    return self.refresh();
                }
                if value["setting"] == "skipDangerousModePermissionPrompt"
                    && let Some(key) = value["account_id"].as_str()
                {
                    // The state the backend reports after the change, never the request.
                    self.bypass_unavailable.remove(key);
                    self.bypass.insert(key.to_owned(), value.clone());
                    if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                        self.dialog = None;
                        self.form = Form::None;
                        self.foreground = None;
                    }
                    return vec![];
                }
                if value["setting"] == "crossSessionInbound"
                    && let Some(key) = value["account_id"].as_str()
                {
                    // The state the backend reports after the change, never the request.
                    self.steering_unavailable.remove(key);
                    self.steering.insert(key.to_owned(), value.clone());
                    if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                        self.dialog = None;
                        self.form = Form::None;
                        self.foreground = None;
                    }
                    return vec![];
                }
                // A new Gemini account chooses its sign-in mode before any login plan runs.
                if let [a, b, key] = &self.mutation[..]
                    && a == "account"
                    && b == "add"
                    && self.provider_of(key) == "gemini"
                    && matches!(self.form, Form::Progress)
                    && self.foreground == Some(token)
                    && let Some(choices) = self.gemini_auth_modes()
                {
                    let key = key.clone();
                    self.foreground = None;
                    self.show_gemini_auth(&key, choices);
                    return self.refresh();
                }
                if let Some(argv) = login_plan(&value["login"]["argv"]) {
                    if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                        // The plan is a backend-owned exec argv; the UI only hands it
                        // to a HERDR pane and polls the account. It handles no tokens.
                        let key = match &self.mutation[..] {
                            [a, b, key] if a == "account" && (b == "add" || b == "login") => {
                                Some(key.clone())
                            }
                            [a, b, key, m, g]
                                if a == "account"
                                    && b == "auth"
                                    && m == "--mode"
                                    && g == "google" =>
                            {
                                Some(key.clone())
                            }
                            _ => None,
                        };
                        if let Some(key) = key {
                            let label =
                                crate::client::provider_label(&self.provider_of(&key)).to_owned();
                            self.dialog = None;
                            let title = self.login_title(&key);
                            self.show(
                                &title,
                                vec![
                                    format!("Complete the {label} login in the opened pane…"),
                                    "This popup closes once the login pane opens; the pane reports the result. Cancel only stops waiting; it does not cancel the login.".into(),
                                ],
                                &["Cancel"],
                                Form::LoginWait(key.clone()),
                            );
                            self.login_wait = Some(LoginWait {
                                key,
                                argv: argv.clone(),
                                started: std::time::Instant::now(),
                                last_poll: None,
                            });
                            return vec![Intent::LaunchLogin(argv)];
                        }
                        self.dialog = None;
                        let title = self.mutation_login_title();
                        self.show(
                            &title,
                            vec![
                                "Run this command in a terminal:".into(),
                                login_command(&value["login"]["argv"]),
                                "Closing this dialog does not cancel enrollment.".into(),
                            ],
                            &["Close"],
                            Form::None,
                        );
                    }
                } else if value["login"].is_object() {
                    if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                        let login = &value["login"];
                        let url = login["verificationUri"]
                            .as_str()
                            .or_else(|| login["verificationUrl"].as_str())
                            .unwrap_or("See provider login response");
                        self.dialog = None;
                        let title = self.mutation_login_title();
                        self.show(&title,vec![url.into(),format!("Code: {}",login["userCode"].as_str().unwrap_or("unavailable")),"Complete login in your browser. Closing this dialog does not cancel enrollment.".into()],&["Continue in background"],Form::None);
                    }
                } else if value["phase"].is_string() {
                    self.active_operation = Some(id(&value).into());
                    if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                        self.form = Form::Operation(id(&value).into());
                        if terminal(value["phase"].as_str().unwrap_or(""))
                            && let Some(d) = &mut self.dialog
                        {
                            finish_operation(
                                d,
                                self.dismissed.contains(id(&value)),
                                retry_state(&value),
                            );
                        }
                    }
                    return self.refresh();
                } else if let (Some(key), Some(generation)) =
                    (value["id"].as_str(), value["generation"].as_str())
                    && let Some(cached) = self.sessions.iter_mut().find(|s| id(s) == key)
                {
                    // An instant session edit (rename) returns the session at its new
                    // generation: adopt it so later reads and actions are not stale.
                    cached["generation"] = json!(generation);
                    if let Some(name) = value["name"].as_str() {
                        cached["name"] = json!(name);
                    }
                    if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                        self.show(
                            "Saved",
                            vec!["The backend acknowledged this change.".into()],
                            &["Close"],
                            Form::None,
                        );
                    }
                    return self.refresh_sessions();
                } else if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                    let note = if value["existing_home"] == true {
                        "An existing Claude home was registered unchanged."
                    } else {
                        "The backend acknowledged this change."
                    };
                    let mut body = vec![note.to_owned()];
                    if value["auth"]["logged_in"] == false {
                        body.push("Not signed in yet. Choose Login on the account page.".into());
                    }
                    self.show("Saved", body, &["Close"], Form::None);
                }
            }
        }
        vec![]
    }
    pub fn paint(&mut self, canvas: &mut Canvas, footer: usize) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.paint_at(canvas, footer, now);
    }
    pub fn paint_at(&mut self, canvas: &mut Canvas, footer: usize, now: i64) {
        let _ = footer; // retained for callers; the layout derives rows from the canvas
        if self.page == Page::Queue {
            self.paint_queue(canvas, footer, now);
            return;
        }
        if let Page::Spaces(key) = self.page.clone() {
            let options = self.options.clone();
            let dialog = self.dialog.take();
            let note = std::mem::take(&mut self.note);
            self.page = Page::Session(key.clone());
            self.options = self
                .history
                .last()
                .map(|p| p.options.clone())
                .unwrap_or_default();
            self.paint_at(canvas, footer, now);
            self.page = Page::Spaces(key);
            self.options = options;
            self.dialog = dialog;
            self.note = note;
            let rows = self.rows();
            let palette = self.palette.clone();
            let mut view = crate::popup::Choices {
                title: crate::popup::SPACE_TITLE,
                description: crate::popup::SPACE_DESCRIPTION,
                rows: &rows,
                editor: None,
                note: &self.note,
                note_accent: false,
                large: false,
                fit_description: true,
                hints: &["▲/▼: Choose", "Enter: Select", "Esc: Cancel"],
                sections: None,
            };
            if self
                .popup_size
                .is_none_or(|s| s.viewport != (canvas.width, canvas.height))
            {
                self.popup_size = Some(view.measure((canvas.width, canvas.height)));
            }
            self.popup_size =
                Some(view.grow_for_selection(self.popup_size.unwrap(), self.options.selected));
            // The popup draws its own controls; the footer shows none of the page's.
            let start = canvas.height.saturating_sub(2);
            for cell in &mut canvas.cells[start * canvas.width..] {
                *cell = crate::render::Cell::default();
            }
            crate::popup::paint_choices(
                canvas,
                &palette,
                &view,
                &mut self.options,
                self.popup_size.unwrap(),
            );
            if let Some(dialog) = &mut self.dialog {
                dialog.paint(canvas, &self.palette);
            }
            return;
        }
        let hints = if self.moving.is_some() {
            hint_lines(&["▲/▼: Move", "Enter: Save", "Esc: Cancel"], canvas.width)
        } else if self.steering_pick.is_some()
            || self.bypass_pick.is_some()
            || self.filter_grab.is_some()
        {
            hint_lines(&["▲/▼: Choose", "Enter: Set", "Esc: Cancel"], canvas.width)
        } else {
            let mut tokens = vec!["▲/▼/◀/▶: Move", "Enter/Space: Select"];
            if self.can_start_move() {
                tokens.push("m: Move");
            }
            tokens.push(if self.page == Page::Home && self.history.is_empty() {
                "Esc: Close"
            } else {
                "Esc: Back"
            });
            let lines = hint_lines(&tokens, canvas.width);
            // The `o` hint joins only when it needs no extra hint row.
            if self.operation_summary().is_some() {
                tokens.insert(tokens.len() - 1, "o: Operations");
                let with = hint_lines(&tokens, canvas.width);
                if with.len() == lines.len() {
                    with
                } else {
                    lines
                }
            } else {
                lines
            }
        };
        // A dialog draws its own controls inside its frame; the footer keeps its rows
        // (and the Esc close band) but shows no controls for the dialog or the page.
        let hints = if self.dialog.is_some() {
            vec![String::new(); hints.len()]
        } else {
            hints
        };
        // Saved-session notice: only on the session list, only until fresh state.
        let note = if self.note.is_empty() && self.page == Page::Sessions && self.cached_snapshot {
            "Showing saved sessions; refreshing…".to_owned()
        } else if self.dialog.as_ref().is_some_and(|d| d.shows(&self.note)) {
            // The open dialog already says this; the bottom lane does not repeat it.
            String::new()
        } else {
            self.note.clone()
        };
        // With no notice, the lane shows the live operation counts (whole or not at all).
        let controls_start = canvas.height.saturating_sub(hints.len());
        // Shown whole or not at all, and never at the cost of the list itself.
        let summary = if note.is_empty() && controls_start >= SUMMARY_MIN_CONTROLS {
            self.fitted_summary(canvas.width)
        } else {
            None
        };
        let list_controls = if note.is_empty() && summary.is_none() {
            controls_start
        } else {
            controls_start.saturating_sub(2)
        };
        let layout = AccountsLayout::fit(&self.accounts, canvas.width);
        if self.usage_focus && !self.usage_selectable() {
            self.usage_focus = false;
        }
        let rows = self.rows();
        if self.page == Page::Home {
            let l = &layout;
            let longest = rows
                .iter()
                .take(5)
                .map(|r| unicode_width::UnicodeWidthStr::width(r.label.as_str()))
                .max()
                .unwrap_or(0);
            let top_left = Options::centered_left(canvas.width, longest + 2);
            for (i, row) in rows.iter().take(5).enumerate() {
                row.paint(
                    canvas,
                    top_left,
                    3 + i,
                    self.options.selected == i && !self.options.back,
                    self.options.back || self.options.selected >= 5,
                );
            }
            let bottom = list_controls.saturating_sub(1).max(10);
            let selected = self.options.selected.checked_sub(5);
            self.account_scroll = l.scroll(
                &self.accounts,
                self.account_scroll,
                selected,
                bottom.saturating_sub(9),
            );
            let moving = self.moving.is_some().then_some(selected).flatten();
            l.paint(
                canvas,
                &self.accounts,
                9,
                bottom,
                self.account_scroll,
                selected,
                self.options.back,
                self.usage_focus,
                moving,
            );
            self.options.paint_back(canvas, top_left, 3, "Close");
        } else {
            let menu_controls = list_controls;
            let bottom = menu_controls.saturating_sub(1);
            let (left, _) = if self.page == Page::Sessions {
                let badges = self.badges();
                if self.badge_focus && self.selected_badge().is_none() {
                    self.badge_focus = false;
                }
                crate::session_list::paint_with(
                    canvas,
                    &mut self.options,
                    &self.sessions,
                    self.sessions_known,
                    bottom,
                    now,
                    menu_controls,
                    crate::session_list::Extras {
                        archives: !self.archives.is_empty(),
                        badges: Some(&badges),
                        badge_focus: self.badge_focus,
                        spinners: self.spinners,
                    },
                )
            } else {
                let mut rows = rows;
                let budget = crate::session_list::name_budget(canvas.width);
                if self.page == Page::Archives {
                    for (row, s) in rows.iter_mut().zip(&self.archives) {
                        let short = self.archive_row(s, budget);
                        row.label = short.label;
                        row.overflow = short.overflow;
                    }
                }
                // Inline statuses and reasons shorten to the space they have; when
                // too narrow, the selected one moves below the list (standard wrap).
                for row in rows.iter_mut() {
                    // Hover notes are never shortened: all beside their rows or all below.
                    if let (Some(column), Some(_)) = (row.preview_column, &row.preview)
                        && !row.preview_choice
                        && !row.is_hover_note()
                    {
                        let room = canvas.width.saturating_sub(column + 6);
                        if room >= 6 {
                            row.shorten_preview(room, crate::widgets::ellipsize);
                        } else {
                            row.preview_selected_only = true;
                        }
                    }
                }
                self.options.right_mode = self.steering_pick.is_some()
                    || self.bypass_pick.is_some()
                    || self.filter_grab.is_some();
                self.options
                    .paint_menu(canvas, &rows, 3, bottom, menu_controls)
            };
            self.options.paint_back(
                canvas,
                left,
                3,
                if self.page == Page::Home {
                    "Close"
                } else {
                    "Back"
                },
            );
        }
        if !note.is_empty() {
            crate::widgets::paint_status(canvas, controls_start, &note);
        } else if let Some(summary) = &summary {
            self.paint_summary(canvas, controls_start, summary);
        }
        crate::widgets::paint_hints(canvas, &hints);
        if let Some(phase) = self
            .close_band
            .and_then(|t| crate::close_band::phase(t.elapsed()))
        {
            let band = canvas.theme.as_ref().map_or([40; 3], |t| t.bar(false));
            crate::close_band::paint(canvas, hints.len(), phase, self.band_base, band, [238; 3]);
        } else {
            self.close_band = None;
        }
        if let Some(d) = &mut self.dialog {
            d.paint(canvas, &self.palette);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn relocation_requires_preview_then_confirmation_and_preserves_target() {
        let mut m = Manager::default();
        let session = json!({"id":"session-1","generation":"generation-1","name":"Test","directory":"/project"});
        m.session_action(session.clone(), crate::session_list::Action::MoveProject);
        assert!(matches!(m.form, Form::Relocation(_, true)));
        assert_eq!(m.dialog.as_ref().unwrap().fields[0].input.value, "/project");
        let intents = m.submit(0, vec!["/destination".into()]);
        let Intent::Api(token, args) = &intents[0] else {
            panic!("preview required")
        };
        assert!(args.contains(&"--dry-run".into()));
        assert!(args.contains(&"--move-files".into()));
        m.response(*token, Ok(json!({"from":"/project","to":"/destination","detail":"Moves files","other_sessions":[]})));
        assert!(matches!(m.form, Form::Confirm(_)));
        let intents = m.submit(0, vec![]);
        let Intent::Api(_, args) = &intents[0] else {
            panic!("confirmed request required")
        };
        assert!(!args.contains(&"--dry-run".into()));
        assert!(args.contains(&"session-1".into()));
        assert!(args.contains(&"generation-1".into()));
    }

    #[test]
    fn provider_error_warning_precedes_configuration_and_cancel_never_mutates() {
        for state in ["errored", "interrupted", "stopped", "unknown"] {
            for label in [
                "Switch account",
                "End session",
                "Restart Codex",
                "Move to Space",
            ] {
                let session = json!({"id":"s","name":"Example","generation":"g","availability":"running","capabilities":{"switch_account":true,"stop":true,"restart":true,"move_space":true}});
                let mut m = Manager {
                    page: Page::Session("s".into()),
                    sessions: vec![session],
                    ..Default::default()
                };
                m.options.selected = m.rows().iter().position(|r| r.label == label).unwrap();
                let requests = m.activate();
                let Intent::Api(token, args) = &requests[0] else {
                    panic!()
                };
                assert_eq!(args[0], "observe");
                assert!(
                    m.response(
                        *token,
                        Ok(json!({"generation":"g","freshness":"fresh","runtime_state":state}))
                    )
                    .is_empty()
                );
                assert_eq!(m.dialog.as_ref().unwrap().title, "Session needs attention");
                assert!(m.submit(1, vec![]).is_empty());
                assert!(m.dialog.is_none());
                let requests = m.activate();
                let Intent::Api(token, _) = &requests[0] else {
                    panic!()
                };
                m.response(
                    *token,
                    Ok(json!({"generation":"g","freshness":"fresh","runtime_state":state})),
                );
                let intents = m.submit(0, vec![]);
                if label == "Switch account" {
                    assert!(matches!(&intents[0], Intent::Switch(_)));
                } else if label == "Move to Space" {
                    assert!(matches!(&intents[0], Intent::Api(_, a) if a[0]=="spaces"));
                } else {
                    assert!(matches!(
                        m.dialog.as_ref().unwrap().title.as_str(),
                        "End session" | "Restart Codex"
                    ));
                    assert!(intents.is_empty());
                }
            }
        }
    }
    #[test]
    fn cancelled_or_replaced_action_checks_never_reopen_dialog_or_retarget() {
        let s = json!({"id":"s","generation":"g","availability":"running","capabilities":{"switch_account":true}});
        let mut m = Manager {
            page: Page::Session("s".into()),
            sessions: vec![s],
            ..Default::default()
        };
        m.options.selected = m
            .rows()
            .iter()
            .position(|r| r.label == "Switch account")
            .unwrap();
        let requests = m.activate();
        let Intent::Api(token, _) = &requests[0] else {
            panic!()
        };
        m.dismiss();
        assert!(
            m.response(
                *token,
                Ok(json!({"generation":"g","freshness":"fresh","state":"errored"}))
            )
            .is_empty()
        );
        assert!(m.dialog.is_none());
        let requests = m.activate();
        let Intent::Api(token, _) = &requests[0] else {
            panic!()
        };
        m.response(*token, Err("provider unavailable".into()));
        assert_eq!(m.dialog.as_ref().unwrap().title, "Session needs attention");
        m.sessions[0]["generation"] = json!("replacement");
        assert!(m.submit(0, vec![]).is_empty());
        assert!(m.dialog.is_none());
    }
    #[test]
    fn archived_names_gain_a_short_id_only_when_they_collide() {
        use super::*;
        let mut m = setup();
        m.page = Page::Archives;
        m.archives = vec![
            json!({"id":"7792aaaa","name":"codex-root"}),
            json!({"id":"1234bbbb","name":"codex-root"}),
            json!({"id":"5555cccc","name":"unique"}),
        ];
        let labels: Vec<_> = m.rows().into_iter().map(|r| r.label).collect();
        assert_eq!(labels, ["codex-root · 7792", "codex-root · 1234", "unique"]);
        assert!(m.rows().iter().all(|r| r.preview.is_none()));
        // Narrow lists shorten the name, never the suffix.
        m.archives[0]["name"] = json!("a-very-long-archived-session-name");
        m.archives[1]["name"] = json!("a-very-long-archived-session-name");
        let mut c = Canvas::new(40, 24);
        m.paint_at(&mut c, 23, 100);
        let line = |y: usize| -> String {
            c.cells[y * 40..(y + 1) * 40]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect()
        };
        assert!(line(4).contains("… · 1234"), "{}", line(4));
        // The selected name scrolls (from its start) instead of ending in `…`.
        assert!(line(3).contains("a-ver · 7792"), "{}", line(3));
        let mut c = Canvas::new(40, 24);
        c.clock = crate::widgets::MARQUEE_PAUSE_MS;
        m.paint_at(&mut c, 23, 100);
        let line: String = c.cells[3 * 40..4 * 40]
            .iter()
            .map(|c| c.glyph.as_str())
            .collect();
        assert!(line.contains("-very · 7792"), "{line}");
    }
    #[test]
    fn loading_confirmation_and_operation_share_the_opened_frame() {
        use super::*;
        let mut m = setup();
        let palette = m.palette.clone();
        m.show(
            "Preview",
            vec!["Loading…".into()],
            &["Cancel"],
            Form::Loading,
        );
        m.dialog
            .as_mut()
            .unwrap()
            .paint(&mut Canvas::new(80, 24), &palette);
        let size = m.dialog.as_ref().unwrap().size.unwrap();
        m.show(
            "Confirm",
            vec!["Review this action.".into()],
            &["Confirm", "Cancel"],
            Form::None,
        );
        m.dialog
            .as_mut()
            .unwrap()
            .fields
            .push(Field::new("Name", "example"));
        let mut c = Canvas::new(80, 24);
        m.dialog.as_mut().unwrap().paint(&mut c, &palette);
        assert_eq!(m.dialog.as_ref().unwrap().size.unwrap().height, size.height);
        assert!(
            c.cells
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
                .contains("Confirm")
        );
        m.mutate(strings(&["fixture-only"]));
        m.dialog
            .as_mut()
            .unwrap()
            .paint(&mut Canvas::new(80, 24), &palette);
        assert_eq!(m.dialog.as_ref().unwrap().size.unwrap().height, size.height);
        m.dialog.as_mut().unwrap().finish();
        m.dialog
            .as_mut()
            .unwrap()
            .paint(&mut Canvas::new(80, 24), &palette);
        assert_eq!(m.dialog.as_ref().unwrap().size.unwrap().height, size.height);
    }
    #[test]
    fn grouped_sessions_keep_enter_and_back_bound_to_the_same_session() {
        use super::*;
        let mut m = setup();
        m.sessions_known = true;
        m.sessions = vec![
            json!({"id":"active","name":"Active","availability":"running"}),
            json!({"id":"saved","name":"Saved","availability":"saved"}),
        ];
        m.options.selected = 1;
        m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(m.page, Page::Sessions);
        assert!(m.rows()[1].separator);
        m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(m.options.selected, 2);
        m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(m.page, Page::Session("saved".into()));
        m.sessions
            .insert(0, json!({"id":"new","name":"New","availability":"running"}));
        m.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(m.page, Page::Sessions);
        assert_eq!(m.options.selected, 3);
        assert_eq!(
            crate::session_list::session_index(&m.sessions, m.options.selected),
            Some(2)
        );
    }
    #[test]
    fn back_restores_actual_parent_and_stable_selection_after_reordering() {
        use super::*;
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let escape = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let mut m = setup();
        m.options.selected = 6;
        m.options.scroll = 1;
        m.key(enter);
        assert_eq!(m.page, Page::Account("codex:account2".into()));
        m.accounts.reverse();
        m.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        m.key(enter);
        // Back restores the parent and follows the account to its new row.
        assert_eq!(m.page, Page::Home);
        assert_eq!(m.options.selected, 5);
        assert!(!m.options.back);

        m.sessions = vec![json!({"id":"a","name":"A"}), json!({"id":"b","name":"B"})];
        m.sessions_known = true;
        m.options.selected = 1;
        m.key(enter);
        assert_eq!(m.page, Page::Sessions);
        m.options.selected = 1;
        m.key(enter);
        assert_eq!(m.page, Page::Session("b".into()));
        m.sessions.reverse();
        m.key(escape);
        assert_eq!(m.page, Page::Sessions);
        assert_eq!(m.options.selected, 0);
        m.operations = vec![
            json!({"id":"o1","kind":"switch","created_at":1}),
            json!({"id":"o2","kind":"restart","created_at":2}),
        ];
        m.options.selected = m.rows().len() - 1;
        m.key(enter);
        all_operations(&mut m);
        assert_eq!(m.page, Page::Operations);
        m.options.selected = 3;
        m.key(enter);
        assert_eq!(m.page, Page::Operation("o1".into()));
        m.operations
            .push(json!({"id":"o3","kind":"switch","created_at":3}));
        m.key(escape);
        assert_eq!(m.page, Page::Operations);
        assert_eq!(
            m.operation_keys()[m.options.selected],
            Some(("operation".into(), "o1".into()))
        );
        m.key(escape);
        assert_eq!(m.page, Page::Queue);
        m.key(escape);
        assert_eq!(m.page, Page::Sessions);
        assert_eq!(m.options.selected, m.rows().len() - 1);
        m.key(escape);
        assert_eq!(m.page, Page::Home);
        assert_eq!(m.options.selected, 1);
    }
    #[test]
    fn back_keeps_archive_and_nested_operation_origins() {
        use super::*;
        let mut m = setup();
        m.sessions_known = true;
        m.archives = vec![json!({"id":"a","name":"Archived"})];
        m.enter(Page::Sessions);
        m.options.selected = m.rows().len() - 2;
        m.activate();
        assert_eq!(m.page, Page::Archives);
        m.activate();
        assert_eq!(m.page, Page::Session("a".into()));
        m.back();
        assert_eq!(m.page, Page::Archives);
        m.back();
        assert_eq!(m.page, Page::Sessions);
        assert_eq!(m.options.selected, m.rows().len() - 2);
        m.go(Page::Operations);
        m.enter(Page::Launch("launch", "request".into()));
        m.options.selected = 1;
        m.enter(Page::Operation("nested".into()));
        m.back();
        assert_eq!(m.page, Page::Launch("launch", "request".into()));
        assert_eq!(m.options.selected, 1);
        m.back();
        assert_eq!(m.page, Page::Operations);
    }
    #[test]
    fn space_picker_uses_bound_backend_choices_and_captured_generation() {
        use super::*;
        let session = json!({"id":"stable","name":"Example","generation":"g1","availability":"running","attachment":{"host":"herdr"},"capabilities":{"move_space":true}});
        let mut m = Manager {
            page: Page::Session("stable".into()),
            sessions: vec![session],
            ..Default::default()
        };
        m.options.selected = m
            .rows()
            .iter()
            .position(|r| r.label == "Move to Space")
            .unwrap();
        let checked = m.activate();
        let Intent::Api(check, _) = &checked[0] else {
            panic!()
        };
        let sent = m.response(
            *check,
            Ok(json!({"generation":"g1","freshness":"fresh","runtime_state":"idle"})),
        );
        let Intent::Api(token, args) = &sent[0] else {
            panic!()
        };
        assert_eq!(
            args,
            &strings(&[
                "spaces",
                "--session",
                "stable",
                "--expected-generation",
                "g1"
            ])
        );
        assert!(
            m.dialog
                .as_ref()
                .unwrap()
                .buttons
                .iter()
                .any(|s| s.label == "Cancel")
        );
        m.response(*token, Ok(json!({"session_id":"stable","generation":"g1","current_space_id":"w1","spaces":[{"id":"w1","name":"Origin","current":true},{"id":"w2","name":"Work","current":false}]})));
        assert_eq!(m.page, Page::Spaces("stable".into()));
        assert_eq!(m.options.selected, 2, "opens on the first selectable Space");
        assert!(!m.rows()[0].enabled, "no new Space for a move yet");
        assert!(!m.rows()[1].enabled, "current Space");
        m.options.selected = 1;
        assert!(m.activate().is_empty());
        m.options.selected = 2;
        // A later state refresh must not silently rebind the confirmed move.
        m.sessions[0]["generation"] = json!("g2");
        assert!(m.activate().is_empty());
        assert!(m.submit(1, vec![]).is_empty(), "cancel is read-only");
        m.activate();
        let sent = m.submit(0, vec![]);
        let Intent::Api(_, args) = &sent[0] else {
            panic!()
        };
        assert_eq!(
            &args[..7],
            &strings(&[
                "move-space",
                "--session",
                "stable",
                "--expected-generation",
                "g1",
                "--space",
                "w2"
            ])
        );
        assert_eq!(args[7], "--request-id");
        assert!(uuid::Uuid::parse_str(&args[8]).is_ok());
        m.dismiss();
        assert_eq!(m.page, Page::Sessions);
        assert_eq!(m.options.selected, 0);
    }

    #[test]
    fn cancelled_space_lookup_cannot_reopen_or_replace_a_new_picker() {
        use super::*;
        let session = json!({"id":"stable","generation":"g1"});
        let mut m = Manager {
            page: Page::Session("stable".into()),
            ..Default::default()
        };
        let sent = m.preview("Choose Space", vec![], Purpose::Spaces(session.clone()));
        let Intent::Api(old, _) = &sent[0] else {
            panic!()
        };
        m.submit(0, vec![]);
        let sent = m.preview("Choose Space", vec![], Purpose::Spaces(session));
        let Intent::Api(new, _) = &sent[0] else {
            panic!()
        };
        m.response(
            *old,
            Ok(json!({"session_id":"stable","generation":"g1","spaces":[]})),
        );
        assert_eq!(m.page, Page::Session("stable".into()));
        assert!(matches!(m.form, Form::Loading));
        m.response(
            *new,
            Ok(json!({"session_id":"other","generation":"g1","spaces":[]})),
        );
        assert_ne!(m.page, Page::Spaces("stable".into()));
        assert!(m.note.contains("changed"));
    }

    #[test]
    fn space_move_is_gated_by_public_capability_and_preserves_picker_back() {
        use super::*;
        let session = json!({"id":"stable","generation":"g1","availability":"running","attachment":{"host":"tmux"},"capabilities":{}});
        let mut m = Manager {
            page: Page::Session("stable".into()),
            sessions: vec![session],
            ..Default::default()
        };
        m.options.selected = m
            .rows()
            .iter()
            .position(|r| r.label == "Move to Space")
            .unwrap();
        assert!(!m.rows()[m.options.selected].enabled);
        assert!(m.activate().is_empty());
        m.go(Page::Spaces("stable".into()));
        m.back();
        assert_eq!(m.page, Page::Session("stable".into()));
    }

    #[test]
    fn archive_restore_delete_delegate_confirmed_identity_to_cli_only() {
        let saved = serde_json::json!({"id":"stable","name":"Saved","generation":"g1","availability":"saved","capabilities":{"archive":true,"delete":true}});
        let mut m = super::Manager {
            page: super::Page::Session("stable".into()),
            sessions: vec![saved.clone()],
            ..Default::default()
        };
        m.options.selected = 4;
        assert!(m.activate().is_empty());
        assert!(m.submit(1, vec![]).is_empty(), "cancel emits no mutation");
        m.activate();
        let calls = m.submit(0, vec![]);
        let super::Intent::Api(_, args) = &calls[0] else {
            panic!("CLI action required")
        };
        assert_eq!(args[0], "archive");
        assert!(
            args.windows(2)
                .any(|w| w == ["--expected-generation", "g1"])
        );
        assert!(args.windows(2).any(|w| w == ["--confirm", "stable"]));
        m.dialog = None;
        m.queue.clear(); // the archive operation finished
        m.options.selected = 5;
        m.activate();
        assert!(m.submit(0, vec!["wrong".into()]).is_empty());
        let calls = m.submit(0, vec!["Saved".into()]);
        assert!(matches!(&calls[0], super::Intent::Api(_,args) if args[0]=="delete-session"));
        let mut archived = saved;
        archived["availability"] = serde_json::json!("archived");
        archived["capabilities"]["restore"] = serde_json::json!(true);
        m.sessions.clear();
        m.archives = vec![archived];
        m.dialog = None;
        m.options.selected = 0;
        m.activate();
        let calls = m.submit(0, vec![]);
        assert!(matches!(&calls[0], super::Intent::Api(_,args) if args[0]=="restore-session"));
    }
    #[test]
    fn deletion_reservation_routes_to_original_operation_and_recovery_capabilities() {
        let mut m = super::Manager { accounts: vec![serde_json::from_value(serde_json::json!({"id":"codex:fixture","selectable":false,"enabled":false,"usage":{},"deletion":{"id":"original","kind":"account_delete","phase":"failed","can_recover":true,"can_cancel":false}})).unwrap()], page: super::Page::Account("codex:fixture".into()), ..Default::default() };
        assert_eq!(
            m.rows()
                .iter()
                .map(|r| r.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Deletion operation"]
        );
        m.activate();
        assert_eq!(m.page, super::Page::Operation("original".into()));
        assert!(m.rows()[5].enabled);
        assert!(!m.rows()[4].enabled);
        m.operations[0]["can_recover"] = serde_json::json!(false);
        assert!(!m.rows()[5].enabled);
        m.operations[0] =
            serde_json::json!({"id":"original","kind":"archive_session","phase":"failed"});
        assert!(
            !m.rows()[5].enabled,
            "unsupported recovery must not be guessed"
        );
    }
    use super::*;
    #[test]
    fn unfinished_resume_warning_requires_confirmation_and_cancel_is_side_effect_free() {
        for confirm in [false, true] {
            let mut m = setup();
            m.go(Page::Session("saved".into()));
            let sent = m.preview("Start session", vec![], Purpose::Resume("saved".into()));
            let Intent::Api(token, _) = sent[0] else {
                panic!()
            };
            assert!(
                m.response(
                    token,
                    Ok(json!({"id":"saved","resume_block_reason":"unfinished_history"}))
                )
                .is_empty()
            );
            assert!(matches!(m.form, Form::ResumeWarning(_)));
            let intents = m.submit(usize::from(!confirm), vec![]);
            if confirm {
                assert!(
                    matches!(&intents[0], Intent::Resume(s) if s["confirm_unfinished"] == true)
                );
            } else {
                assert!(intents.is_empty());
            }
            assert!(m.dialog.is_none());
        }
    }
    #[test]
    fn session_menu_initially_focuses_unavailable_first_action() {
        let mut m = setup();
        for availability in ["running", "unavailable"] {
            m.sessions = vec![
                json!({"id":"blocked","availability":availability,"attachment":{"host":"tmux"},"capabilities":{"resume":false}}),
            ];
            m.go(Page::Session("blocked".into()));
            assert_eq!(m.options.selected, 0);
            assert!(!m.rows()[0].enabled);
            assert!(m.activate().is_empty());
            m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
            assert_eq!(m.options.selected, 1);
        }
    }
    #[test]
    fn operations_are_newest_first_grouped_by_day_filterable_and_plain() {
        let mut m = setup();
        m.page = Page::Operations;
        m.operations = vec![
            json!({"id":"old","kind":"switch","phase":"complete","created_at":10,"updated_at":1000}),
            json!({"id":"new","kind":"account_delete","account_id":"codex:fixture0","phase":"running","created_at":20,"updated_at":1}),
            json!({"id":"legacy","kind":"move_space","phase":"failed","detail":"Destination Space is gone.","updated_at":2000}),
        ];
        m.launches.insert("launch",vec![json!({"request":{"request_id":"12345678-1234-4234-8234-123456789abc","name":"fresh","account_id":"codex:fixture0"},"phase":"running","created_at":30})]);
        m.launches.insert(
            "resume-launch",
            vec![json!({"request":{"request_id":"resume-id"},"phase":"running","created_at":15})],
        );
        let entries = |m: &Manager| -> Vec<String> {
            m.operation_keys()
                .into_iter()
                .flatten()
                .map(|(_, key)| key)
                .collect()
        };
        assert_eq!(
            entries(&m),
            vec![
                "12345678-1234-4234-8234-123456789abc",
                "new",
                "resume-id",
                "old",
                "legacy"
            ]
        );
        let rows = m.rows();
        assert_eq!(rows[0].label, "Show");
        assert!(rows[0].preview_choice);
        assert_eq!(rows[0].preview.as_deref(), Some("↓ All"));
        // The filter row sits directly above the first day heading.
        assert!(rows[1].separator);
        assert_eq!(rows[1].label, "Today");
        let undated = rows.iter().position(|r| r.label == "Undated").unwrap();
        assert!(rows[undated].separator);
        let labels: Vec<_> = rows.iter().map(|r| r.label.clone()).collect();
        assert!(labels[2].contains("⏣ fresh") && labels[2].contains("New session"));
        assert!(labels[3].contains("Delete account"));
        assert!(labels[4].contains("Resume"));
        assert!(labels[5].contains("Switch account"));
        assert!(labels[undated + 1].contains("Move to Space"));
        assert_eq!(rows[3].preview.as_deref(), Some("in progress"));
        assert_eq!(rows[5].preview.as_deref(), Some("done"));
        assert_eq!(
            rows[undated + 1].preview.as_deref(),
            Some("failed · Destination Space is gone")
        );
        // Kinds, times and statuses line up in columns.
        let column = rows[2].preview_column;
        assert!(
            rows[2..]
                .iter()
                .filter(|r| !r.separator)
                .all(|r| r.preview_column == column)
        );
        // Grab the filter, choose; the list follows live.
        let key = |m: &mut Manager, code| {
            m.key(KeyEvent::new(code, KeyModifiers::NONE));
        };
        m.options.selected = 0;
        key(&mut m, KeyCode::Right);
        assert!(m.filter_grab.is_some());
        key(&mut m, KeyCode::Down);
        assert_eq!(m.operation_filter, OperationFilter::InProgress);
        assert_eq!(entries(&m), vec!["new"]);
        key(&mut m, KeyCode::Down);
        assert_eq!(entries(&m), vec!["legacy"]);
        // Esc reverts to the value held at the grab.
        key(&mut m, KeyCode::Esc);
        assert_eq!(m.operation_filter, OperationFilter::All);
        assert!(m.filter_grab.is_none());
        assert!(!m.options.back);
        m.options.selected = 2;
        m.activate();
        assert_eq!(
            m.page,
            Page::Launch("launch", "12345678-1234-4234-8234-123456789abc".into())
        );
        assert_eq!(m.title(), "New session");
    }
    #[test]
    fn unchanged_rename_is_valid_but_other_names_and_generation_stay_checked() {
        let mut m = setup();
        let target = json!({"id":"one","name":"Existing name","generation":"captured","capabilities":{"rename":true}});
        m.sessions = vec![target.clone(), json!({"id":"two","name":"Other name"})];
        m.go(Page::Session("one".into()));
        m.options.selected = 2;
        m.activate();
        assert!(m.dialog.as_ref().unwrap().fields[0].valid());
        m.dialog.as_mut().unwrap().fields[0].input =
            crate::widgets::TextInput::new("Other name".into());
        assert!(!m.dialog.as_ref().unwrap().fields[0].valid());
        m.dialog.as_mut().unwrap().fields[0].input =
            crate::widgets::TextInput::new("Existing name".into());
        assert!(m.dialog.as_ref().unwrap().fields[0].valid());
        let sent = m.submit(0, vec!["Existing name".into()]);
        let Intent::Api(_, args) = &sent[0] else {
            panic!()
        };
        assert_eq!(
            args,
            &strings(&[
                "rename",
                "--session",
                "one",
                "--expected-generation",
                "captured",
                "--name",
                "Existing name"
            ])
        );
    }
    #[test]
    fn cached_accounts_show_at_once_as_stale_and_never_selectable() {
        let mut m = Manager::default();
        let cached = json!([{"id":"claude:a","selectable":true,"prefill":{"eligible":true},
            "usage":{"state":"fresh","short_percent":40},"usage_fresh":true}]);
        m.show_cached_accounts(&cached);
        assert!(
            !m.inventory_known,
            "still waiting for the authoritative list"
        );
        assert_eq!(m.accounts.len(), 1);
        assert!(!m.accounts[0].selectable && m.accounts[0].prefill.is_null());
        assert_eq!(crate::usage::usage_state(&m.accounts[0]), Some("stale"));
        assert_eq!(m.accounts[0].details["usage_fresh"], json!(false));
        m.inventory(vec![]);
        assert!(m.inventory_known && m.accounts.is_empty());
        m.show_cached_accounts(&cached);
        assert!(m.accounts.is_empty(), "a real inventory is never replaced");
    }
    #[test]
    fn session_details_open_cached_and_update_without_generation_bound_reads() {
        let mut m = setup();
        m.sessions =
            vec![json!({"id":"one","name":"Cached","generation":"old","directory":"/cached"})];
        m.go(Page::Session("one".into()));
        m.options.selected = 1;
        let sent = m.activate();
        assert!(
            m.dialog
                .as_ref()
                .unwrap()
                .body
                .iter()
                .any(|s| s.starts_with("Directory:") && s.ends_with("/cached"))
        );
        let Intent::Api(token, args) = &sent[0] else {
            panic!()
        };
        assert_eq!(args, &strings(&["state"]));
        m.response(*token,Ok(json!({"sessions":[{"id":"one","name":"Fresh","generation":"new","directory":"/fresh"}]})));
        assert!(
            m.dialog
                .as_ref()
                .unwrap()
                .body
                .iter()
                .any(|s| s.starts_with("Directory:") && s.ends_with("/fresh"))
        );
        assert!(
            m.dialog
                .as_ref()
                .unwrap()
                .body
                .iter()
                .all(|s| !s.contains("Generation") && !s.contains("new"))
        );
        // State polls are throttled to one per five ticks.
        let sent = (0..5).flat_map(|_| m.refresh_live()).collect::<Vec<_>>();
        assert_eq!(sent.len(), 1);
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.response(token, Err("DISCOVERY_UNAVAILABLE".into()));
        assert!(
            m.dialog
                .as_ref()
                .unwrap()
                .body
                .iter()
                .any(|s| s.starts_with("Directory:") && s.ends_with("/fresh"))
        );
        m.submit(0, vec![]);
        assert_eq!(m.page, Page::Session("one".into()));
    }
    #[test]
    fn live_sessions_sort_by_busy_idle_inactive_then_recent_access_and_keep_identity() {
        let mut m = setup();
        m.page = Page::Sessions;
        m.sessions_known = true;
        m.sessions = vec![json!({"id":"saved","name":"Saved","availability":"saved"})];
        m.options.selected = 0;
        let sent = m.refresh_sessions();
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        let entries = json!({"sessions":[
            {"id":"saved","name":"Saved","availability":"saved","last_accessed":900},
            {"id":"idle-old","name":"Idle old","availability":"running","activity":"idle","last_accessed":10},
            {"id":"busy-old","name":"Busy old","availability":"running","activity":"busy","last_accessed":2},
            {"id":"idle-new","name":"Idle new","availability":"running","activity":"idle","last_accessed":20},
            {"id":"busy-new","name":"Busy new","availability":"running","activity":"busy","last_accessed":3},
            {"id":"saved-old","name":"Saved old","availability":"saved","last_accessed":8}
        ]});
        m.response(token, Ok(entries));
        assert_eq!(
            m.sessions.iter().map(id).collect::<Vec<_>>(),
            vec![
                "busy-new",
                "busy-old",
                "idle-new",
                "idle-old",
                "saved",
                "saved-old"
            ]
        );
        assert_eq!(
            m.sessions
                [crate::session_list::session_index(&m.sessions, m.options.selected).unwrap()]["id"],
            "saved"
        );
    }
    #[test]
    fn inactive_sessions_sort_by_last_opened_across_providers_with_fallbacks() {
        let mut v = vec![
            json!({"id":"n-none-b","name":"B","availability":"stopped"}),
            json!({"id":"n-none-a","name":"A","availability":"stale"}),
            json!({"id":"claude-created","name":"C","availability":"stopped","conversation_created_at":"2026-10-28T04:12:16.412Z"}),
            json!({"id":"codex-ms","name":"X","availability":"saved","last_accessed":1793267529333_i64,"conversation_created_at":"2026-01-01T00:00:00Z"}),
            json!({"id":"gem-new","name":"G","availability":"stopped","conversation_created_at":"2026-10-29T11:47:19.039Z"}),
            json!({"id":"tie-2","name":"Same","availability":"saved","last_accessed":1793267529_i64}),
            json!({"id":"tie-1","name":"Same","availability":"saved","last_accessed":1793267529_i64}),
            json!({"id":"run","name":"Run","availability":"running","activity":"idle","last_accessed":1}),
        ];
        sort_sessions(&mut v);
        assert_eq!(
            v.iter().map(id).collect::<Vec<_>>(),
            [
                "run",
                "gem-new",
                "tie-1",
                "tie-2",
                "codex-ms",
                "claude-created",
                "n-none-a",
                "n-none-b"
            ]
        );
    }
    #[test]
    fn switching_confirms_latest_snapshot_and_progress_returns_to_sessions() {
        let mut m = setup();
        let old = json!({"id":"two","name":"Earlier","generation":"old"});
        m.sessions = vec![
            json!({"id":"one"}),
            json!({"id":"two","name":"Current","generation":"fresh","capabilities":{"switch_account":true}}),
        ];
        m.switch_confirmation(old, "codex:account2");
        assert!(m.dialog.as_ref().unwrap().body[0].contains("Current"));
        let sent = m.submit(0, vec![]);
        let Intent::Api(_, args) = &sent[0] else {
            panic!()
        };
        assert!(
            args.windows(2)
                .any(|w| w == ["--expected-generation", "fresh"])
        );
        m.submit(0, vec![]);
        assert_eq!(m.page, Page::Sessions);
        assert_eq!(m.options.selected, 1);
        for command in ["archive", "delete-session"] {
            m.queue.clear(); // the previous operation finished
            m.mutate(strings(&[command, "--session", "two"]));
            m.submit(0, vec![]);
            assert_eq!(m.page, Page::Sessions);
            assert_eq!(m.options.selected, 0);
        }
    }
    #[test]
    fn stale_target_uses_centered_status_lane_and_reserves_menu_space() {
        let mut m = setup();
        m.page = Page::Home;
        m.inventory((1..20).map(account).collect());
        let sent = m.preview(
            "Details",
            strings(&["session"]),
            Purpose::Limits("codex:account1".into()),
        );
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.response(token, Err("STALE_TARGET".into()));
        assert!(m.dialog.is_none());
        let mut c = Canvas::new(40, 18);
        m.paint_at(&mut c, 17, 100);
        let y = 14; // Two wrapped control rows begin at 16.
        let row = c.cells[y * 40..(y + 1) * 40]
            .iter()
            .map(|c| c.glyph.as_str())
            .collect::<String>();
        assert!(row.contains("session changed"));
        assert!(row.starts_with(' '));
        assert!(m.options.scroll < m.rows().len());
        assert!(c.cells.iter().any(|c| c.glyph == "1"));
    }
    #[test]
    fn direct_operation_details_close_returns_to_submenu_without_reopening() {
        let mut m = setup();
        for phase in ["running", "complete"] {
            m.go(Page::Operation("job".into()));
            m.operations = vec![json!({"id":"job","phase":phase})];
            m.activate();
            assert!(m.dialog.is_some());
            m.submit(0, vec![]);
            assert_eq!(m.page, Page::Operation("job".into()));
            assert_eq!(m.options.selected, 0);
            assert!(m.dialog.is_none());
        }
        m.go(Page::Launch("launch", "req".into()));
        let sent = m.activate();
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.response(token, Ok(json!({"phase":"running"})));
        assert!(m.dialog.is_some());
        m.submit(0, vec![]);
        assert_eq!(m.page, Page::Launch("launch", "req".into()));
        let sent = m.request(
            strings(&["launch", "status"]),
            Purpose::Launch("launch", "req".into()),
        );
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.response(token, Ok(json!({"phase":"running"})));
        assert!(m.dialog.is_none());
        m.go(Page::Account("codex:account1".into()));
        m.mutate(strings(&["account", "enable", "codex:account1"]));
        m.submit(0, vec![]);
        assert_eq!(m.page, Page::Home);
    }
    #[test]
    fn delete_availability_is_read_only_and_reasons_follow_backend() {
        let mut m = setup();
        m.page = Page::Account("codex:account1".into());
        assert_eq!(
            m.delete_account_row("codex:account1").preview.as_deref(),
            Some("(unsupported)")
        );
        m.capabilities["providers"][0]["account_deletion"] = json!(true);
        for (preview, reason) in [
            (
                json!({"can_delete":false,"owner_upgrade_required":true}),
                Some("(owner update needed)"),
            ),
            (
                json!({"can_delete":false,"affected_sessions":[{"running":true}]}),
                Some("(active sessions)"),
            ),
            (json!({"can_delete":true}), None),
        ] {
            let requests = m.refresh();
            let Intent::Api(token, args) = &requests[0] else {
                panic!("preview")
            };
            assert_eq!(args, &strings(&["account", "delete", "codex:account1"]));
            m.response(*token, Ok(preview));
            let row = m.delete_account_row("codex:account1");
            assert_eq!(row.enabled, reason.is_none());
            assert_eq!(row.preview.as_deref(), reason);
            assert!(m.dialog.is_none());
        }
    }
    #[test]
    fn initial_session_load_selects_top_then_preserves_operations_on_refresh() {
        let mut m = setup();
        m.page = Page::Sessions;
        m.sessions_known = false;
        m.sessions.clear();
        let intents = m.refresh_sessions();
        let Intent::Api(token, _) = &intents[0] else {
            panic!("state request")
        };
        let state = json!({"sessions":[{"id":"first","name":"First","availability":"saved"}],"capabilities":{}});
        m.response(*token, Ok(state.clone()));
        assert_eq!(m.options.selected, 0);
        m.options.selected = m.rows().len() - 1;
        let intents = m.refresh_sessions();
        let Intent::Api(token, _) = &intents[0] else {
            panic!("state request")
        };
        m.response(*token, Ok(state));
        assert_eq!(m.options.selected, m.rows().len() - 1);
    }
    #[test]
    fn completed_progress_uses_close_and_loading_is_not_a_menu_option() {
        let mut m = setup();
        m.page = Page::Sessions;
        m.sessions_known = false;
        assert_eq!(
            m.rows()
                .iter()
                .map(|r| r.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Operations"]
        );
        let mut c = Canvas::new(60, 18);
        m.paint_at(&mut c, 17, 100);
        let text = |y: usize| {
            c.cells[y * 60..(y + 1) * 60]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
        };
        assert!(text(5).contains("Loading…"));
        assert!(text(4).trim().is_empty());
        m.page = Page::Operation("done".into());
        m.operations = vec![json!({"id":"done","phase":"complete"})];
        m.options.selected = 0;
        m.activate();
        assert_eq!(m.dialog.as_ref().unwrap().buttons[1].label, "Close");
        m.form = Form::Operation("done".into());
        m.dialog = Some(Dialog::progress("Working"));
        let requests = m.request(strings(&["operations"]), Purpose::Operations);
        let Intent::Api(token, _) = requests[0] else {
            panic!()
        };
        m.response(token, Ok(json!([{"id":"done","phase":"complete"}])));
        assert_eq!(m.dialog.as_ref().unwrap().buttons[1].label, "Close");
        assert_eq!(m.rows()[0].label, "Details");
    }
    #[test]
    fn background_consent_is_separate_sequence_bound_and_cancelled_without_termination() {
        fn poll(m: &mut Manager, phase: &str, sequence: u64) {
            let requests = m.request(strings(&["operations"]), Purpose::Operations);
            let Intent::Api(token, _) = requests[0] else {
                panic!()
            };
            m.response(token, Ok(json!([{"id":"switch","phase":phase,"confirmation_seq":sequence,"detail":"2 background terminals are running. Stop them and continue?"}])));
        }
        for cancel in [false, true] {
            let mut m = setup();
            m.session_return = Some("session".into());
            m.show(
                "Working",
                vec![],
                &["Continue in background"],
                Form::Operation("switch".into()),
            );
            poll(&mut m, "awaiting_background_approval", 7);
            assert_eq!(
                m.dialog.as_ref().unwrap().title,
                "Stop background terminals?"
            );
            m.dialog.as_mut().unwrap().selected = 1;
            poll(&mut m, "awaiting_background_approval", 7);
            assert_eq!(m.dialog.as_ref().unwrap().selected, 1);
            let sent = if cancel {
                m.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            } else {
                m.submit(0, vec![])
            };
            let Intent::Api(_, args) = &sent[0] else {
                panic!()
            };
            assert_eq!(
                args,
                &strings(&[
                    "respond",
                    "switch",
                    if cancel { "no" } else { "yes" },
                    "--sequence",
                    "7"
                ])
            );
            assert_eq!(m.session_return.as_deref(), Some("session"));
        }
        let mut m = setup();
        poll(&mut m, "awaiting_background_approval", 7);
        assert!(
            m.dialog.is_none(),
            "background operation must not steal focus"
        );
        m.show(
            "Working",
            vec![],
            &["Continue in background"],
            Form::Operation("switch".into()),
        );
        poll(&mut m, "awaiting_background_approval", 7);
        poll(&mut m, "complete", 8);
        assert!(matches!(m.form, Form::Operation(_)));
        assert_eq!(m.dialog.as_ref().unwrap().buttons[1].label, "Close");
    }
    #[test]
    fn operation_consent_retains_dedicated_details_return() {
        let mut m = setup();
        m.page = Page::Operation("switch".into());
        m.details_open = true;
        m.show(
            "Stop background terminals?",
            vec![],
            &["Stop and continue", "Cancel operation"],
            Form::OperationApproval("switch".into(), 4),
        );
        let sent = m.submit(0, vec![]);
        assert_eq!(sent.len(), 1);
        assert!(m.details_open);
        m.dismiss();
        assert_eq!(m.page, Page::Operation("switch".into()));
    }
    fn session_state_args(intents: &[Intent]) -> Vec<Vec<String>> {
        intents
            .iter()
            .filter_map(|i| match i {
                Intent::Api(_, a) if a.first().is_some_and(|x| x == "state") => Some(a.clone()),
                _ => None,
            })
            .collect()
    }
    #[test]
    fn operations_button_always_opens_at_the_top_item() {
        let mut m = setup();
        m.sessions_known = true;
        m.operations = vec![
            json!({"id":"op1","kind":"restart","phase":"complete"}),
            json!({"id":"op2","kind":"restart","phase":"complete"}),
            json!({"id":"op3","kind":"restart","phase":"complete"}),
        ];
        for previous in [0, 2] {
            m.go(Page::Sessions);
            m.options.selected = m.rows().len() - 1; // the Operations row
            m.activate();
            all_operations(&mut m);
            assert_eq!(m.page, Page::Operations);
            assert_eq!(m.options.selected, 0);
            m.options.selected = previous;
            // Leave without Esc bookkeeping; re-open from the Sessions page.
            m.go(Page::Sessions);
        }
        // Even a remembered position from a sibling visit does not carry over.
        m.go(Page::Operations);
        m.options.selected = 2;
        m.pending.clear();
        m.go(Page::Sessions);
        m.options.selected = m.rows().len() - 1;
        let sent = m.activate();
        let Some(Intent::Api(token, _)) = sent
            .iter()
            .find(|i| matches!(i, Intent::Api(_, a) if a == &strings(&["operations"])))
        else {
            panic!("operations were not requested: {sent:?}")
        };
        all_operations(&mut m);
        m.response(*token, Ok(json!(m.operations.clone())));
        assert_eq!(m.options.selected, 0);
    }
    /// The Operations entry opens the queue popup; its All operations row opens the list.
    fn all_operations(m: &mut super::Manager) {
        assert_eq!(m.page, super::Page::Queue);
        m.options.selected = m
            .rows()
            .iter()
            .position(|r| r.label == "All operations")
            .unwrap();
        m.activate();
    }
    fn screen_text(c: &Canvas) -> String {
        c.cells
            .chunks(c.width)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn move_mode_reorders_saves_by_qualified_id_and_cancels_with_esc() {
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let mut m = dual_providers();
        m.capabilities["account_reorder"] = json!(true);
        let ids = |m: &Manager| m.accounts.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
        let original = ids(&m);
        // Footer: the Move hint appears only while an account row is focused.
        m.options.selected = 0;
        assert!(
            !screen_text(&{
                let mut c = Canvas::new(90, 30);
                m.paint(&mut c, 29);
                c
            })
            .contains("m: Move")
        );
        m.options.selected = 6;
        let focused = {
            let mut c = Canvas::new(90, 30);
            m.paint(&mut c, 29);
            screen_text(&c)
        };
        assert!(focused.contains("m: Move"));
        // Pick up the Claude account, move it to the top and see the marker.
        m.key(key(KeyCode::Char('m')));
        m.key(key(KeyCode::Up));
        assert_eq!(m.options.selected, 5);
        assert_eq!(m.accounts[0].id, "claude:claude-main");
        let moving = {
            let mut c = Canvas::new(90, 30);
            m.paint(&mut c, 29);
            screen_text(&c)
        };
        assert!(moving.contains("⇅") && moving.contains("(moving)"));
        assert!(
            moving.contains("▲/▼: Move")
                && moving.contains("Enter: Save")
                && moving.contains("Esc: Cancel")
        );
        assert!(!moving.contains("m: Move"));
        // Up at the top and other keys do nothing; Esc restores the old order.
        m.key(key(KeyCode::Up));
        assert_eq!(m.options.selected, 5);
        m.key(key(KeyCode::Esc));
        assert_eq!(ids(&m), original);
        assert_eq!(m.options.selected, 6);
        assert_eq!(m.page, Page::Home);
        // Enter saves with the provider-qualified id and the new position.
        m.key(key(KeyCode::Char('m')));
        m.key(key(KeyCode::Down));
        let sent = m.key(key(KeyCode::Enter));
        assert!(
            matches!(&sent[..], [Intent::Api(_, a)] if a == &strings(&["account", "reorder", "claude:claude-main", "--position", "2"])),
            "{sent:?}"
        );
        // Saved silently: no dialog, no acknowledgement text.
        let [Intent::Api(token, _)] = &sent[..] else {
            unreachable!()
        };
        assert!(m.dialog.is_none());
        m.response(*token, Ok(json!({"ok":true})));
        assert!(m.dialog.is_none() && m.note.is_empty());
        let moved = ids(&m);
        // A failed save reverts to the backend's order and surfaces the error.
        m.options.selected = 6;
        m.key(key(KeyCode::Char('m')));
        m.key(key(KeyCode::Up));
        let sent = m.key(key(KeyCode::Enter));
        let [Intent::Api(token, _)] = &sent[..] else {
            panic!("{sent:?}")
        };
        m.response(
            *token,
            Err("ACCOUNT_ORDER_CONFLICT: changed elsewhere".into()),
        );
        assert_eq!(ids(&m), moved);
        assert!(m.dialog.is_none() && !m.note.is_empty());
        assert_eq!(m.accounts[m.options.selected - 5].id, moved[1]);
        // Not advertised: no move mode and no hint.
        let mut m = dual_providers();
        m.options.selected = 6;
        m.key(key(KeyCode::Char('m')));
        let mut c = Canvas::new(90, 30);
        m.paint(&mut c, 29);
        assert!(!screen_text(&c).contains("m: Move") && !screen_text(&c).contains("⇅"));
    }
    #[test]
    fn accounts_keep_backend_order_and_duplicate_names_are_keyed_by_qualified_id() {
        let mut m = Manager::default();
        let mk = |id: &str| -> Account {
            serde_json::from_value(json!({"id":id,"selectable":true,"enabled":true,"usage_fresh":true,"usage":{"observed_at":100}})).unwrap()
        };
        m.inventory(vec![
            mk("gemini:work"),
            mk("claude:work"),
            mk("codex:work"),
            mk("codex:alpha"),
        ]);
        let ids: Vec<_> = m.accounts.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            ["gemini:work", "claude:work", "codex:work", "codex:alpha"]
        );
        // Selecting the second "work" and re-ordering the inventory follows the id.
        m.options.selected = 6;
        m.inventory(vec![mk("codex:alpha"), mk("codex:work"), mk("claude:work")]);
        assert_eq!(m.options.selected, 5 + 2);
        m.activate();
        assert_eq!(m.page, Page::Account("claude:work".into()));
        assert!(m.title().contains('✻'));
        // The home list shows distinguishing symbols for the identical names.
        let mut c = Canvas::new(90, 30);
        m.go(Page::Home);
        m.paint(&mut c, 29);
        let text = screen_text(&c);
        assert!(text.contains("✻ work") && text.contains("⏣ work"));
    }
    #[test]
    fn home_list_scrolls_instead_of_opening_a_submenu_and_keeps_the_selection_visible() {
        let mut m = setup();
        m.inventory((0..30).map(account).collect());
        let mut c = Canvas::new(60, 20);
        m.paint(&mut c, 19);
        assert_eq!(m.rows()[2].label, "Add account");
        assert!(screen_text(&c).contains("account0"));
        m.options.selected = 5 + 29;
        let mut c = Canvas::new(60, 20);
        m.paint(&mut c, 19);
        let text = screen_text(&c);
        assert!(m.account_scroll > 0);
        assert!(text.contains("account29") && !text.contains("account0 "));
        assert!(
            text.contains("5h") || text.contains("wk"),
            "bars still drawn"
        );
        m.options.selected = 5;
        let mut c = Canvas::new(60, 20);
        m.paint(&mut c, 19);
        assert_eq!(m.account_scroll, 0);
        assert!(!m.rows().iter().any(|r| r.label == "Manage Accounts"));
    }
    #[test]
    fn home_list_wrap_jumps_the_scroll_to_the_matching_end() {
        let mut m = setup();
        m.inventory((0..30).map(account).collect());
        let layout = crate::usage::AccountsLayout::fit(&m.accounts, 60);
        let press = |m: &mut Manager, code| {
            m.key(KeyEvent::new(code, KeyModifiers::NONE));
        };
        let paint = |m: &mut Manager| {
            let mut c = Canvas::new(60, 24);
            m.paint(&mut c, 23);
            c
        };
        let row = |c: &Canvas, y: usize| -> String {
            c.cells[y * 60..(y + 1) * 60]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect()
        };
        m.options.selected = 5 + 29;
        let c = paint(&mut m);
        let end = m.account_scroll;
        assert!(end > 0);
        assert!(screen_text(&c).contains("account29"));
        // The top `…`, under the options: one blank row on each side, at the names.
        assert_eq!(c.cells[9 * 60 + layout.name_x].glyph, "…");
        assert_eq!(row(&c, 9).trim(), "…");
        assert!(row(&c, 8).trim().is_empty() && row(&c, 10).trim().is_empty());
        // Down from the last account wraps to the top option: the list is at its top at once.
        press(&mut m, KeyCode::Down);
        assert_eq!(m.options.selected, 0);
        assert_eq!(m.account_scroll, 0);
        let c = paint(&mut m);
        assert_eq!(m.account_scroll, 0);
        assert!(screen_text(&c).contains("account0"));
        assert_ne!(c.cells[9 * 60 + layout.name_x].glyph, "…");
        // Up from the top option wraps to the last account: the list is at its end at once.
        press(&mut m, KeyCode::Up);
        assert_eq!(m.options.selected, 5 + 29);
        assert!(m.account_scroll >= end);
        paint(&mut m);
        assert_eq!(m.account_scroll, end);
    }
    #[test]
    fn page_selection_resets_on_entry_and_is_restored_on_return() {
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let mut m = setup();
        m.sessions_known = true;
        m.sessions = vec![json!({"id":"a","name":"A"}), json!({"id":"b","name":"B"})];
        m.operations = vec![
            json!({"id":"op1","kind":"restart","phase":"complete"}),
            json!({"id":"op2","kind":"restart","phase":"complete"}),
        ];
        // Home (row 1) -> Sessions: fresh top selection.
        m.options.selected = 1;
        m.key(key(KeyCode::Enter));
        assert_eq!(m.page, Page::Sessions);
        assert_eq!(m.options.selected, 0);
        // Sessions (row 1) -> Session: fresh; its own position changes.
        m.options.selected = 1;
        m.key(key(KeyCode::Enter));
        assert_eq!(m.page, Page::Session("b".into()));
        assert_eq!(m.options.selected, 0);
        m.options.selected = 2;
        // Back: parent keeps the position it had (the entered session).
        m.key(key(KeyCode::Esc));
        assert_eq!(m.page, Page::Sessions);
        assert_eq!(m.options.selected, 1);
        // Sessions -> Operations -> Operation, a nested chain, then all the way back.
        m.options.selected = m.rows().len() - 1;
        m.key(key(KeyCode::Enter));
        all_operations(&mut m);
        assert_eq!(m.page, Page::Operations);
        assert_eq!(m.options.selected, 0);
        m.options.selected = 3;
        m.key(key(KeyCode::Enter));
        assert_eq!(m.page, Page::Operation("op2".into()));
        assert_eq!(m.options.selected, 0);
        m.key(key(KeyCode::Esc));
        assert_eq!((m.page.clone(), m.options.selected), (Page::Operations, 3));
        m.key(key(KeyCode::Esc));
        assert_eq!(m.page, Page::Queue);
        m.key(key(KeyCode::Esc));
        assert_eq!(m.page, Page::Sessions);
        assert_eq!(m.options.selected, m.rows().len() - 1);
        m.key(key(KeyCode::Esc));
        assert_eq!((m.page.clone(), m.options.selected), (Page::Home, 1));
        // Re-entering Operations later always starts at the top again.
        m.options.selected = 1;
        m.key(key(KeyCode::Enter));
        m.options.selected = m.rows().len() - 1;
        m.key(key(KeyCode::Enter));
        all_operations(&mut m);
        assert_eq!((m.page.clone(), m.options.selected), (Page::Operations, 0));
        // Home -> an account page starts at its first row, back lands on the account.
        m.go(Page::Home);
        m.options.selected = 6;
        m.key(key(KeyCode::Enter));
        assert_eq!(m.options.selected, 0);
        m.key(key(KeyCode::Esc));
        assert_eq!((m.page.clone(), m.options.selected), (Page::Home, 6));
    }
    #[test]
    fn first_render_comes_from_cache_without_actions_then_fresh_state_replaces_it() {
        let mut m = setup();
        m.page = Page::Sessions;
        let cached = json!([{"id":"s1","name":"Saved copy","generation":"old","availability":"running","activity":"busy",
            "capabilities":{"stop":true,"rename":true,"restore":true}}]);
        m.show_cached(&cached);
        // Rendered immediately: no request needed, known, marked refreshing.
        assert!(m.sessions_known && m.cached_snapshot);
        assert_eq!(m.sessions[0]["name"], "Saved copy");
        // The notice belongs to the session list only, never a global note.
        assert!(m.note.is_empty());
        let notice = |m: &mut Manager| {
            let mut c = Canvas::new(80, 24);
            m.paint_at(&mut c, 23, 100);
            screen_text(&c).contains("Showing saved sessions; refreshing")
        };
        assert!(notice(&mut m));
        m.page = Page::Home;
        assert!(!notice(&mut m), "never on Home");
        m.page = Page::Sessions;
        assert!(m.cache_out.is_none(), "cache is never re-saved from itself");
        // Nothing from the cache can be acted on: no generation, no capabilities.
        assert!(m.sessions[0].get("generation").is_none());
        assert_eq!(m.sessions[0]["capabilities"], json!({}));
        assert_eq!(m.sessions[0]["activity"], "unknown");
        m.go(Page::Session("s1".into()));
        assert!(
            m.rows()
                .iter()
                .all(|r| !r.enabled || r.label == "Session details"),
            "{:?}",
            m.rows()
                .iter()
                .map(|r| (&r.label, r.enabled))
                .collect::<Vec<_>>()
        );
        // Fresh state replaces it and is persisted.
        m.page = Page::Sessions;
        let sent = m.read_sessions();
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.response(token, Ok(json!({"sessions":[{"id":"s1","name":"Fresh","generation":"new","availability":"running","activity":"idle","capabilities":{"stop":true}}]})));
        assert!(!m.cached_snapshot);
        assert_eq!(m.sessions[0]["name"], "Fresh");
        assert_eq!(m.sessions[0]["generation"], "new");
        assert!(!notice(&mut m), "gone once fresh data arrives");
        assert_eq!(m.cache_out.take().unwrap()[0]["name"], "Fresh");
        // A cache arriving after fresh data is ignored.
        m.show_cached(&cached);
        assert_eq!(m.sessions[0]["name"], "Fresh");
    }
    #[test]
    fn state_polls_every_five_seconds_only_while_visible() {
        let mut m = setup();
        m.page = Page::Sessions;
        let mut sent = 0;
        for tick in 1..=20 {
            let out = m.refresh_live();
            let n = session_state_args(&out).len();
            if n > 0 {
                assert_eq!(tick % 5, 0, "poll at tick {tick}");
                let Intent::Api(token, _) = out[0] else {
                    panic!()
                };
                m.response(token, Ok(json!({"sessions":[]})));
            }
            sent += n;
        }
        assert_eq!(sent, 4);
        m.visible = false;
        assert!((0..20).all(|_| session_state_args(&m.refresh_live()).is_empty()));
        // Regaining focus reads immediately.
        m.visible = true;
        assert_eq!(session_state_args(&m.read_sessions()).len(), 1);
    }
    #[test]
    fn cached_read_serves_polls_and_first_paint_with_full_state_every_thirty_seconds() {
        let mut m = setup();
        m.capabilities = json!({"state_cached": true});
        m.page = Page::Sessions;
        // Entering the page: cheap read plus the authoritative full read.
        let first = session_state_args(&m.read_sessions());
        assert_eq!(
            first,
            vec![strings(&["state", "--cached"]), strings(&["state"])]
        );
        let cached = json!({"sessions":[{"id":"c","name":"Cheap","generation":"g","availability":"saved","capabilities":{"stop":true}}],
            "snapshot":{"observed_at":1,"fresh":false},"capabilities":{"state_cached":true}});
        let tokens: Vec<u64> = (m.sequence - 1..=m.sequence).collect();
        m.response(tokens[0], Ok(cached));
        assert!(m.cached_snapshot, "stale snapshot shows without actions");
        assert_eq!(m.sessions[0]["capabilities"], json!({}));
        m.response(tokens[1], Ok(json!({"sessions":[{"id":"c","name":"Full","generation":"g2","availability":"saved"}],"capabilities":{"state_cached":true}})));
        assert!(!m.cached_snapshot);
        assert_eq!(m.sessions[0]["name"], "Full");
        // Periodic polls use the cheap read; every 30 s the full one.
        let mut kinds = vec![];
        for tick in 1..=30 {
            let out = m.refresh_live();
            for args in session_state_args(&out) {
                kinds.push((tick, args.len() == 2));
                let Intent::Api(token, _) = out[0] else {
                    panic!()
                };
                m.response(token, Ok(json!({"sessions":[],"snapshot":{"fresh":true},"capabilities":{"state_cached":true}})));
            }
        }
        assert_eq!(
            kinds,
            vec![
                (5, true),
                (10, true),
                (15, true),
                (20, true),
                (25, true),
                (30, false)
            ]
        );
    }
    #[test]
    fn manage_sessions_running_account_uses_the_icon_accent_and_name_stays_plain() {
        for width in [80usize, 120, 160] {
            for (selected, back, level) in
                [(0usize, false, 1000i64), (1, false, 600), (0, true, 600)]
            {
                let mut m = setup();
                m.page = Page::Sessions;
                m.sessions_known = true;
                m.sessions = vec![
                    json!({"id":"a","name":"alpha","provider":"claude","account_id":"claude:main","availability":"running","activity":"idle"}),
                    json!({"id":"b","name":"bravo","provider":"claude","account_id":"claude:main","availability":"running","activity":"idle"}),
                ];
                m.options.selected = selected;
                m.options.back = back;
                let mut field = crate::render::Field::new(width, 24, 1967);
                let mut c = field.paint(5000);
                m.paint_at(&mut c, 23, 100);
                let theme = c.theme.clone().unwrap();
                let glyphs = |y: usize| -> Vec<String> {
                    c.cells[y * width..(y + 1) * width]
                        .iter()
                        .map(|c| c.glyph.clone())
                        .collect()
                };
                let y = (0..24)
                    .find(|&y| glyphs(y).concat().contains("alpha"))
                    .unwrap();
                let line = glyphs(y);
                let name = (0..width)
                    .find(|&i| line[i..].concat().starts_with("alpha"))
                    .unwrap();
                let icon = line.iter().position(|g| g == "✻").unwrap();
                let accent = crate::render::Accent {
                    brightness: level,
                    bold: false,
                    flash: false,
                    ..Default::default()
                };
                for x in std::iter::once(icon).chain(icon + 2..icon + 6) {
                    assert_eq!(
                        c.cells[y * width + x].fg,
                        theme.accent(x, y, &accent).fg,
                        "w={width} x={x} sel={selected} back={back} {:?}",
                        line.concat()
                    );
                }
                for x in name..name + 5 {
                    let fg = c.cells[y * width + x].fg;
                    assert!(
                        fg == crate::widgets::SELECTED || fg == crate::widgets::SECONDARY,
                        "name is plain: w={width} x={x} {fg:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn live_state_refresh_is_single_flight_and_errors_clear_busy_claims() {
        let mut m = setup();
        m.page = Page::Sessions;
        m.sessions =
            vec![json!({"id":"id", "name":"Working", "availability":"running", "activity":"busy"})];
        let requests = m.refresh_sessions();
        let Intent::Api(token, args) = &requests[0] else {
            panic!()
        };
        assert_eq!(args, &strings(&["state"]));
        assert!(m.refresh_sessions().is_empty());
        m.response(*token, Ok(json!({"sessions":[{"id":"id","name":"Working","availability":"running","activity":"idle"}]})));
        assert_eq!(m.sessions[0]["activity"], "idle");
        let requests = m.refresh_sessions();
        let Intent::Api(token, _) = &requests[0] else {
            panic!()
        };
        m.sessions[0]["activity"] = json!("busy");
        m.response(*token, Err("Discovery unavailable".into()));
        assert_eq!(m.sessions[0]["activity"], "unknown");
        m.page = Page::Home;
        assert!(m.refresh_sessions().is_empty());
    }
    fn account(n: usize) -> Account {
        serde_json::from_value(json!({"id":format!("codex:account{n}"),"selectable":true,"enabled":true,"usage_fresh":true,"usage":{"observed_at":100,"short":{"duration_minutes":300,"used_percent":25},"weekly":{"duration_minutes":10080,"used_percent":35}},"switch_thresholds":{"customized":true,"short_percent":80,"weekly_percent":90}})).unwrap()
    }
    #[test]
    fn account_menu_reasons_show_on_hover_in_parentheses_all_beside_or_all_below() {
        let mut m = setup();
        let long = "the provider console deletes this account";
        m.capabilities["providers"][0]["operations"]["account_deletion"]["reason"] = json!(long);
        m.go(Page::Account("codex:account1".into()));
        let rows = m.rows();
        let at = |label: &str| rows.iter().position(|r| r.label == label).unwrap();
        let notes = [
            (at("Rename"), "(unsupported)".to_owned()),
            (at("Delete account"), format!("({long})")),
        ];
        for (width, beside) in [(140, true), (70, false)] {
            for selected in [at("Details"), notes[0].0, notes[1].0] {
                m.options.selected = selected;
                let mut c = Canvas::new(width, 30);
                m.paint(&mut c, 29);
                let text = screen_text(&c);
                for (row, note) in &notes {
                    let label = &rows[*row].label;
                    let line = text.lines().find(|l| l.contains(label.as_str())).unwrap();
                    assert!(!line.contains('·'), "{width}: {line}");
                    if *row != selected {
                        assert!(
                            !text.contains(note.as_str()),
                            "{width} {selected}: hover only"
                        );
                        continue;
                    }
                    assert_eq!(text.matches(note.as_str()).count(), 1, "{width}:\n{text}");
                    assert_eq!(line.contains(note.as_str()), beside, "{width}:\n{text}");
                }
            }
        }
    }
    fn setup() -> Manager {
        let mut m = Manager::default();
        m.inventory(vec![account(1), account(2)]);
        m.capabilities =
            json!({"providers":[{"id":"codex","account_limits":true,"enrollment":true}]});
        m
    }
    #[test]
    fn home_account_focus_dims_top_bullets_like_close_without_dimming_labels() {
        let mut m = setup();
        let mut field = crate::render::Field::new(100, 40, 1967);
        let base = field.paint(4200);
        let mut close = base.clone();
        m.options.back = true;
        m.paint(&mut close, 39);
        for usage_focus in [false, true] {
            m.options.back = false;
            m.options.selected = 5;
            m.usage_focus = usage_focus;
            let mut selected = base.clone();
            m.paint(&mut selected, 39);
            for y in [3, 4, 5, 7] {
                let range = y * 100..(y + 1) * 100;
                let x = close.cells[range.clone()]
                    .iter()
                    .position(|c| c.glyph == "○")
                    .unwrap();
                assert_eq!(selected.cells[y * 100 + x], close.cells[y * 100 + x]);
                let label = &selected.cells[y * 100 + x + 2];
                assert_eq!(label.fg, crate::widgets::SECONDARY);
                assert!(!label.bold);
            }
        }
    }
    #[tokio::test]
    async fn live_steering_reads_and_changes_through_the_backend_and_reports_failures() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let cli = tmp.path().join("sessionsation");
        std::fs::write(&cli, r#"#!/usr/bin/env python3
import json, pathlib, sys
root=pathlib.Path(__file__).parent
args=sys.argv[sys.argv.index('--api-version')+2:]
def ok(result): print(json.dumps({'api_version':1,'ok':True,'result':result}))
def fail(code):
 print(json.dumps({'api_version':1,'ok':False,'error':{'code':code,'message':'fixture'}}),file=sys.stderr);sys.exit(1)
if args[:2]!=['account','live-steering']:ok({});sys.exit(0)
with (root/'calls').open('a') as f:f.write(' '.join(args[2:])+'\n')
mode=(root/'mode').read_text()
value=(root/'value').read_text() or None
if mode=='unreadable':fail('SETTINGS_INVALID')
if '--enable' in args or '--disable' in args:
 if mode=='symlink':fail('SETTINGS_IS_SYMLINK')
 if '--enable' in args:value='accept'
 elif value=='accept':value=None
 (root/'value').write_text(value or '')
ok({'account_id':args[2],'provider':'claude','live_steering':value=='accept','setting':'crossSessionInbound','value':value,'settings_file':'/fixture/settings.json'})
"#).unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        let client = crate::client::Client {
            binary: cli,
            root: tmp.path().into(),
            herdr: "unused".into(),
            socket: "unused".into(),
            session: "fixture".into(),
        };
        let set = |mode: &str, value: &str| {
            std::fs::write(tmp.path().join("mode"), mode).unwrap();
            std::fs::write(tmp.path().join("value"), value).unwrap();
            std::fs::write(tmp.path().join("calls"), "").unwrap();
        };
        let calls = || std::fs::read_to_string(tmp.path().join("calls")).unwrap();
        async fn drive(m: &mut Manager, client: &crate::client::Client, mut intents: Vec<Intent>) {
            while let Some(intent) = intents.pop() {
                if let Intent::Api(token, args) = intent {
                    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
                    let result = client.api(&refs, None).await.map_err(|e| e.to_string());
                    intents.extend(m.response(token, result));
                }
            }
        }
        let key = "claude:main";
        let mut m = Manager::default();
        let mut claude = account(1);
        claude.id = key.into();
        let mut codex = account(2);
        codex.id = "codex:other".into();
        m.inventory(vec![claude, codex]);
        m.capabilities = json!({"claude_input_delivery":{"peer_message":{"opt_in":"account live-steering --enable"}}});
        let row = |m: &Manager| m.rows().into_iter().find(|r| r.label == "Live steering");
        let press = |m: &mut Manager, code| m.key(KeyEvent::new(code, KeyModifiers::NONE));
        // Codex accounts never offer it.
        m.go(Page::Account("codex:other".into()));
        assert!(row(&m).is_none());
        // Read: the backend state is shown, with its info text.
        set("plain", "");
        m.go(Page::Account(key.into()));
        assert!(!row(&m).unwrap().preview_choice, "unknown until read");
        let intents = m.refresh();
        drive(&mut m, &client, intents).await;
        assert_eq!(calls(), "claude:main\n");
        let r = row(&m).unwrap();
        assert_eq!(r.preview.as_deref(), Some("↑ Off"));
        assert!(r.info.as_deref().unwrap().contains("message this account"));
        m.options.selected = m
            .rows()
            .iter()
            .position(|r| r.label == "Live steering")
            .unwrap();
        // Enable: grab, choose On, set. The value is what the backend returns.
        press(&mut m, KeyCode::Right);
        assert!(row(&m).unwrap().preview_grabbed);
        press(&mut m, KeyCode::Up);
        assert_eq!(row(&m).unwrap().preview.as_deref(), Some("↓ On"));
        let intents = press(&mut m, KeyCode::Enter);
        assert!(m.dialog.is_some(), "the usual operation progress");
        drive(&mut m, &client, intents).await;
        assert_eq!(calls(), "claude:main\nclaude:main --enable\n");
        assert!(m.dialog.is_none());
        assert_eq!(row(&m).unwrap().preview.as_deref(), Some("↓ On"));
        assert!(!row(&m).unwrap().preview_grabbed);
        // Disable.
        press(&mut m, KeyCode::Right);
        press(&mut m, KeyCode::Down);
        let intents = press(&mut m, KeyCode::Enter);
        drive(&mut m, &client, intents).await;
        assert!(calls().ends_with("claude:main --disable\n"));
        assert_eq!(row(&m).unwrap().preview.as_deref(), Some("↑ Off"));
        // Esc reverts without a command; an unchanged choice sends nothing.
        set("plain", "");
        press(&mut m, KeyCode::Right);
        press(&mut m, KeyCode::Up);
        assert!(press(&mut m, KeyCode::Esc).is_empty());
        press(&mut m, KeyCode::Right);
        assert!(press(&mut m, KeyCode::Enter).is_empty());
        assert_eq!(calls(), "");
        // Error path: a symlinked settings file is refused and shown as unavailable,
        // also after the next read.
        set("symlink", "");
        press(&mut m, KeyCode::Right);
        press(&mut m, KeyCode::Up);
        let intents = press(&mut m, KeyCode::Enter);
        drive(&mut m, &client, intents).await;
        assert!(m.dialog.is_none());
        assert!(m.note.starts_with("SETTINGS_IS_SYMLINK"), "{}", m.note);
        let r = row(&m).unwrap();
        assert!(!r.enabled);
        assert_eq!(r.preview.as_deref(), Some("(settings.json is a symlink)"));
        let intents = m.refresh();
        drive(&mut m, &client, intents).await;
        assert_eq!(
            row(&m).unwrap().preview.as_deref(),
            Some("(settings.json is a symlink)")
        );
        // An operator's hold or refuse is shown honestly and cannot be changed here.
        for held in ["hold", "refuse"] {
            let mut m = Manager::default();
            m.inventory(vec![account(1)]);
            m.accounts[0].id = key.into();
            m.capabilities = json!({"claude_input_delivery":{"peer_message":{}}});
            m.go(Page::Account(key.into()));
            set("plain", held);
            let intents = m.refresh();
            drive(&mut m, &client, intents).await;
            let r = row(&m).unwrap();
            assert!(!r.enabled);
            assert_eq!(r.preview, Some(format!("(settings.json sets {held})")));
            m.options.selected = m
                .rows()
                .iter()
                .position(|r| r.label == "Live steering")
                .unwrap();
            press(&mut m, KeyCode::Right);
            assert!(m.steering_pick.is_none());
        }
        // A failed read is shown on the row, not assumed off.
        set("unreadable", "");
        m.steering_unavailable.clear();
        let intents = m.refresh();
        drive(&mut m, &client, intents).await;
        let r = row(&m).unwrap();
        assert!(
            !r.enabled
                && r.preview
                    .as_deref()
                    .is_some_and(|p| p.starts_with('(') && p.ends_with(')'))
        );
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn bypass_warning_reads_and_changes_through_the_backend() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let cli = tmp.path().join("sessionsation");
        std::fs::write(&cli, r#"#!/usr/bin/env python3
import json, pathlib, sys
root=pathlib.Path(__file__).parent
args=sys.argv[sys.argv.index('--api-version')+2:]
def ok(result): print(json.dumps({'api_version':1,'ok':True,'result':result}))
if args[:2]!=['account','bypass-warning']:ok({});sys.exit(0)
with (root/'calls').open('a') as f:f.write(' '.join(args[2:])+'\n')
value=(root/'value').read_text()
if '--skip' in args:value='skip'
elif '--show' in args:value='show'
(root/'value').write_text(value)
ok({'account_id':args[2],'provider':'claude','bypass_warning':value,'setting':'skipDangerousModePermissionPrompt','value':value=='skip'})
"#).unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        let client = crate::client::Client {
            binary: cli,
            root: tmp.path().into(),
            herdr: "unused".into(),
            socket: "unused".into(),
            session: "fixture".into(),
        };
        std::fs::write(tmp.path().join("value"), "skip").unwrap();
        std::fs::write(tmp.path().join("calls"), "").unwrap();
        let calls = || std::fs::read_to_string(tmp.path().join("calls")).unwrap();
        async fn drive(m: &mut Manager, client: &crate::client::Client, mut intents: Vec<Intent>) {
            while let Some(intent) = intents.pop() {
                if let Intent::Api(token, args) = intent {
                    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
                    let result = client.api(&refs, None).await.map_err(|e| e.to_string());
                    intents.extend(m.response(token, result));
                }
            }
        }
        let key = "claude:main";
        let mut m = Manager::default();
        let mut claude = account(1);
        claude.id = key.into();
        let mut codex = account(2);
        codex.id = "codex:other".into();
        m.inventory(vec![claude, codex]);
        m.capabilities = json!({"claude_bypass_warning":{"since_revision":22}});
        let row = |m: &Manager| m.rows().into_iter().find(|r| r.label == "Bypass warning");
        let press = |m: &mut Manager, code| m.key(KeyEvent::new(code, KeyModifiers::NONE));
        m.go(Page::Account("codex:other".into()));
        assert!(row(&m).is_none(), "Codex accounts never offer it");
        // An older backend without the capability hides it.
        m.capabilities = json!({});
        m.go(Page::Account(key.into()));
        assert!(row(&m).is_none());
        m.capabilities = json!({"claude_bypass_warning":{"since_revision":22}});
        m.go(Page::Account(key.into()));
        assert!(!row(&m).unwrap().preview_choice, "unknown until read");
        let intents = m.refresh();
        drive(&mut m, &client, intents).await;
        assert_eq!(calls(), "claude:main\n");
        let r = row(&m).unwrap();
        assert_eq!(r.preview.as_deref(), Some("↓ Skip"));
        assert_eq!(
            r.info.as_deref(),
            Some("(skip Claude's YOLO-mode startup warning)")
        );
        m.options.selected = m
            .rows()
            .iter()
            .position(|r| r.label == "Bypass warning")
            .unwrap();
        // Skip is the top option, Show the bottom one.
        press(&mut m, KeyCode::Right);
        assert!(row(&m).unwrap().preview_grabbed);
        press(&mut m, KeyCode::Down);
        assert_eq!(row(&m).unwrap().preview.as_deref(), Some("↑ Show"));
        let intents = press(&mut m, KeyCode::Enter);
        assert!(m.dialog.is_some(), "the usual operation progress");
        drive(&mut m, &client, intents).await;
        assert_eq!(calls(), "claude:main\nclaude:main --show\n");
        assert!(m.dialog.is_none());
        assert_eq!(row(&m).unwrap().preview.as_deref(), Some("↑ Show"));
        assert!(!row(&m).unwrap().preview_grabbed);
        // Back to Skip.
        press(&mut m, KeyCode::Right);
        press(&mut m, KeyCode::Up);
        let intents = press(&mut m, KeyCode::Enter);
        drive(&mut m, &client, intents).await;
        assert!(calls().ends_with("claude:main --skip\n"));
        assert_eq!(row(&m).unwrap().preview.as_deref(), Some("↓ Skip"));
        // Esc reverts without a command; an unchanged choice sends nothing.
        std::fs::write(tmp.path().join("calls"), "").unwrap();
        press(&mut m, KeyCode::Right);
        press(&mut m, KeyCode::Down);
        assert!(press(&mut m, KeyCode::Esc).is_empty());
        press(&mut m, KeyCode::Right);
        assert!(press(&mut m, KeyCode::Enter).is_empty());
        assert_eq!(calls(), "");
    }
    #[test]
    fn home_right_column_survives_single_column_rows() {
        let mut m = setup();
        let key = |m: &mut Manager, code| m.key(KeyEvent::new(code, KeyModifiers::NONE));
        m.options.selected = 5;
        key(&mut m, KeyCode::Right);
        assert!(m.usage_focus);
        // Up through the single-column rows: the cursor sits on their only column.
        while m.options.selected > 0 {
            key(&mut m, KeyCode::Up);
            if !m.usage_selectable() {
                assert!(!m.usage_focus, "row {}", m.options.selected);
            }
        }
        // Back down onto the account rows: the right column again.
        while m.options.selected < 5 {
            key(&mut m, KeyCode::Down);
        }
        assert!(m.usage_focus);
        key(&mut m, KeyCode::Down);
        assert_eq!(m.options.selected, 6);
        assert!(m.usage_focus);
    }
    #[test]
    fn home_close_arrow_resets_the_column_to_the_left() {
        let mut m = setup();
        let key = |m: &mut Manager, code| m.key(KeyEvent::new(code, KeyModifiers::NONE));
        m.options.selected = 5;
        key(&mut m, KeyCode::Right);
        assert!(m.usage_focus);
        key(&mut m, KeyCode::Left);
        assert!(!m.usage_focus && !m.options.back);
        key(&mut m, KeyCode::Left);
        assert!(m.options.back, "on Close");
        key(&mut m, KeyCode::Right);
        assert!(!m.options.back);
        assert!(!m.usage_focus, "back on the left option");
        for code in [KeyCode::Up, KeyCode::Down, KeyCode::Down] {
            key(&mut m, code);
        }
        assert!(m.usage_selectable() && !m.usage_focus);
        // Left onto Close straight from a single-column row resets it too.
        m.options.selected = 5;
        key(&mut m, KeyCode::Right);
        while m.usage_selectable() {
            key(&mut m, KeyCode::Up);
        }
        key(&mut m, KeyCode::Left);
        assert!(m.options.back);
        key(&mut m, KeyCode::Right);
        while m.options.selected < 5 {
            key(&mut m, KeyCode::Down);
        }
        assert!(!m.usage_focus);
    }
    #[test]
    fn home_column_clamps_on_rows_with_fewer_columns() {
        let mut m = setup();
        let key = |m: &mut Manager, code| m.key(KeyEvent::new(code, KeyModifiers::NONE));
        m.options.selected = 5;
        key(&mut m, KeyCode::Right);
        // Right on a single-column row neither leaves it nor forgets the column.
        key(&mut m, KeyCode::Up);
        let single = m.options.selected;
        assert!(!m.usage_selectable() && !m.usage_focus);
        key(&mut m, KeyCode::Right);
        assert_eq!(m.options.selected, single);
        assert!(!m.usage_focus && !m.options.back);
        assert_eq!(m.home_column, 1, "the right column is still preferred");
        while m.options.selected < 5 {
            key(&mut m, KeyCode::Down);
        }
        assert!(m.usage_focus);
    }
    #[test]
    fn home_usage_focus_limits_returns_to_same_selection() {
        let mut m = setup();
        m.options.selected = 5;
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        m.key(key(KeyCode::Right));
        assert!(m.usage_focus);
        let sent = m.key(key(KeyCode::Enter));
        let Intent::Api(token, args) = &sent[0] else {
            panic!()
        };
        assert_eq!(args, &strings(&["account", "limits", "codex:account1"]));
        m.response(
            *token,
            Ok(json!({"limits":{"short_percent":80,"weekly_percent":90}})),
        );
        assert!(matches!(m.form, Form::Limits(_)));
        m.key(key(KeyCode::Esc));
        assert_eq!(m.page, Page::Home);
        assert_eq!(m.options.selected, 5);
        assert!(m.usage_focus);
        let sent = m.key(key(KeyCode::Enter));
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.response(token, Ok(json!({"limits":{}})));
        let sent = m.submit(0, vec!["20".into(), "10".into()]);
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.response(token, Ok(json!({"limits":{}})));
        m.submit(0, vec![]);
        assert_eq!(m.page, Page::Home);
        assert_eq!(m.options.selected, 5);
        assert!(m.usage_focus);
        m.key(key(KeyCode::Left));
        assert!(!m.usage_focus);
        assert!(!m.options.back);
        m.accounts[0].details.insert("enabled".into(), json!(false));
        m.key(key(KeyCode::Right));
        assert!(m.usage_focus);
    }
    #[test]
    fn home_status_shortcuts_open_dialogs_and_preserve_focus_on_cancel() {
        for (status, field, value, title, command) in [
            (
                "Disabled",
                "enabled",
                json!(false),
                "Enable account",
                "enable",
            ),
            (
                "Login required",
                "auth_required",
                json!(true),
                "Codex login",
                "login",
            ),
            (
                "Deregistered",
                "membership",
                json!("removed"),
                "Restore account",
                "restore",
            ),
        ] {
            let mut m = setup();
            m.options.selected = 5;
            m.accounts[0].details.insert(field.into(), value);
            assert_eq!(Usage::account(&m.accounts[0]).status, status);
            m.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
            assert!(m.usage_focus);
            assert!(
                m.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
                    .is_empty()
            );
            assert_eq!(m.dialog.as_ref().unwrap().title, title);
            m.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            assert_eq!(m.page, Page::Home);
            assert_eq!(m.options.selected, 5);
            assert!(m.usage_focus);
            m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            let sent = m.submit(0, vec![]);
            assert!(
                matches!(&sent[..], [Intent::Api(_, args)] if args == &strings(&["account", command, "codex:account1"]))
            );
        }
    }
    #[test]
    fn add_account_accepts_thirty_and_rejects_thirty_one_without_dispatch() {
        let mut m = setup();
        m.add();
        assert!(m.submit(0, vec!["a".repeat(31)]).is_empty());
        assert!(m.dialog.as_ref().unwrap().body[0].contains("1–30"));
        let sent = m.submit(0, vec!["a".repeat(30)]);
        let Intent::Api(_, args) = &sent[0] else {
            panic!()
        };
        assert_eq!(&args[..2], ["account", "add"]);
        assert_eq!(args[2], format!("codex:{}", "a".repeat(30)));
    }
    #[test]
    fn duplicate_account_validation_refreshes_without_io() {
        let mut m = setup();
        m.add();
        m.dialog.as_mut().unwrap().paste("account1");
        let mut canvas = Canvas::new(80, 30);
        m.paint(&mut canvas, 29);
        assert!(!m.dialog.as_ref().unwrap().fields[0].valid());
        assert!(canvas.cells.iter().any(|c| c.fg == [215, 0, 0]));
        assert!(
            m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                .is_empty()
        );
        m.dialog.as_mut().unwrap().fields[0].input.clear();
        m.dialog.as_mut().unwrap().paste("fresh");
        assert!(m.dialog.as_ref().unwrap().fields[0].valid());
        let fresh: Account =
            serde_json::from_value(json!({"id":"codex:fresh","selectable":false})).unwrap();
        m.inventory(vec![fresh]);
        assert!(!m.dialog.as_ref().unwrap().fields[0].valid());
    }
    #[test]
    fn esc_band_overlays_the_hint_rows_and_second_esc_closes_during_fade_in() {
        use crate::close_band::{FADE_IN, FADE_OUT, HOLD, MESSAGE};
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let screen = |m: &mut Manager| {
            let mut c = Canvas::new(100, 30);
            m.paint_at(&mut c, 29, 100);
            screen_text(&c)
        };
        let mut m = setup();
        assert!(m.key(esc).is_empty());
        // Mid fade-in the key is still handled: a second Esc closes at once.
        m.close_band = Some(std::time::Instant::now() - FADE_IN / 2);
        assert!(matches!(&m.key(esc)[..], [Intent::Exit]));
        // During the hold the message is shown in place of the hints.
        let mut m = setup();
        m.key(esc);
        m.close_band = Some(std::time::Instant::now() - FADE_IN - HOLD / 2);
        let text = screen(&mut m);
        assert!(text.contains(MESSAGE) && !text.contains("Esc: Close"));
        // After the window the next Esc is a first press again.
        m.close_armed =
            Some(std::time::Instant::now() - crate::close_band::CLOSE_WINDOW - FADE_OUT / 100);
        assert!(m.key(esc).is_empty());
        // The band ends after its full length.
        m.close_band = Some(std::time::Instant::now() - crate::close_band::TOTAL);
        assert!(!screen(&mut m).contains(MESSAGE));
        assert!(m.close_band.is_none());
    }
    #[test]
    fn home_needs_a_second_esc_within_a_second_and_footers_follow_context() {
        let mut m = setup();
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let footer = |m: &mut Manager| {
            let mut c = Canvas::new(100, 30);
            m.paint_at(&mut c, 29, 100);
            screen_text(&c)
        };
        assert!(footer(&mut m).contains("Esc: Close"));
        assert!(m.key(esc).is_empty(), "first Esc only arms");
        assert!(
            footer(&mut m).contains("Esc: Close"),
            "hints are not renamed"
        );
        assert!(m.close_pending());
        assert!(matches!(&m.key(esc)[..], [Intent::Exit]));
        // An expired arm needs two presses again; other keys disarm.
        m.close_armed = Some(std::time::Instant::now() - std::time::Duration::from_secs(2));
        assert!(m.key(esc).is_empty());
        m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(m.key(esc).is_empty(), "disarmed by another key");
        m.close_armed = None;
        // Close itself still closes at once.
        m.options.back = true;
        assert!(matches!(
            &m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))[..],
            [Intent::Exit]
        ));
        m.options.back = false;
        m.go(Page::Sessions);
        assert!(footer(&mut m).contains("Esc: Back"));
        m.show(
            "Confirm",
            vec!["Sure?".into()],
            &["Confirm", "Cancel"],
            Form::None,
        );
        assert!(footer(&mut m).contains("Esc: Cancel"));
        m.show("Info", vec!["Done.".into()], &["Close"], Form::None);
        assert!(footer(&mut m).contains("Enter/Esc: Close"));
    }
    #[test]
    fn session_badges_follow_operations_and_open_their_progress() {
        let mut m = setup();
        m.space_names.insert("w2".into(), "work".into());
        m.sessions_known = true;
        m.sessions = vec![
            json!({"id":"s1","name":"liq-glass-lead","account_id":"codex:account1","availability":"running","activity":"idle","pending_operation_id":"op1","capabilities":{}}),
            json!({"id":"s2","name":"other","account_id":"codex:account1","availability":"saved","capabilities":{}}),
        ];
        m.operations = vec![
            json!({"id":"op1","kind":"move_space","session_id":"s1","phase":"dispatching","space_id":"w2","created_at":90}),
            json!({"id":"op2","kind":"move_space","session_id":"s2","phase":"failed","created_at":95,"detail":"gone"}),
            json!({"id":"op0","kind":"move_space","session_id":"s2","phase":"complete","created_at":10}),
        ];
        let badges = m.badges();
        assert_eq!(badges["s1"].text, "moving to work");
        assert!(badges["s1"].active);
        assert_eq!(badges["s2"].text, "move failed");
        assert!(!badges["s2"].active);
        m.go(Page::Sessions);
        let mut c = Canvas::new(100, 30);
        m.paint_at(&mut c, 29, 100);
        let text = screen_text(&c);
        assert!(text.contains("moving to work") && text.contains("✗ move failed"));
        // The action menu never leaks the operation kind into labels.
        assert!(!m.rows().iter().any(|r| r.label.contains("moving")));
        // Right focuses the badge, Enter opens its progress over the list, Left returns.
        m.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(m.badge_focus);
        let sent = m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(&sent[..], [Intent::Api(_, a)] if a[0] == "operations"));
        assert_eq!(m.page, Page::Sessions);
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.title, "Move to Space");
        assert!(d.body.iter().any(|l| l.contains("op1")));
        m.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(m.dialog.is_none());
        assert_eq!(m.page, Page::Sessions);
        assert!(m.badge_focus, "closing returns to the badge");
        m.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(!m.badge_focus && !m.options.back);
        m.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(m.options.back);
        // A row without a badge keeps Right as before.
        m.options.back = false;
        m.options.selected = crate::session_list::session_row(&m.sessions, 1);
        m.operations.truncate(1);
        m.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(!m.badge_focus);
    }
    #[test]
    fn finished_operation_badges_can_be_dismissed_and_stay_dismissed() {
        let fresh = || {
            let mut m = setup();
            m.sessions_known = true;
            m.sessions = vec![
                json!({"id":"s1","name":"busy","account_id":"codex:account1","availability":"running","activity":"idle","capabilities":{}}),
                json!({"id":"s2","name":"other","account_id":"codex:account1","availability":"saved","capabilities":{}}),
            ];
            m.operations = vec![
                json!({"id":"op1","kind":"move_space","session_id":"s1","phase":"dispatching","created_at":90}),
                json!({"id":"op2","kind":"move_space","session_id":"s2","phase":"failed","created_at":95,"detail":"gone"}),
            ];
            m.go(Page::Sessions);
            m
        };
        let press = |m: &mut Manager, code| m.key(KeyEvent::new(code, KeyModifiers::NONE));
        let open = |m: &mut Manager, session: usize| {
            m.dialog = None;
            m.form = Form::None;
            m.badge_focus = false;
            m.options.back = false;
            m.options.selected = crate::session_list::session_row(&m.sessions, session);
            press(m, KeyCode::Right);
            assert!(m.badge_focus);
            press(m, KeyCode::Enter);
            m.dialog
                .as_ref()
                .unwrap()
                .buttons
                .iter()
                .map(|b| b.label.clone())
                .collect::<Vec<_>>()
        };
        let mut m = fresh();
        // Running: no Dismiss. Failed and not dismissed: Dismiss, Retry, Close with Dismiss focused;
        // Retry is disabled because the backend does not offer recovery for this kind.
        assert_eq!(open(&mut m, 0), ["Continue in background"]);
        assert_eq!(open(&mut m, 1), ["Dismiss", "Retry", "Close"]);
        assert_eq!(m.dialog.as_ref().unwrap().selected, 0);
        assert!(!m.dialog.as_ref().unwrap().buttons[1].enabled);
        m.paint_at(&mut Canvas::new(100, 30), 29, 100);
        // Down skips the disabled Retry; Up returns to the default Dismiss, which hides
        // the badge and returns to the session row.
        press(&mut m, KeyCode::Down);
        press(&mut m, KeyCode::Down);
        assert_eq!(m.dialog.as_ref().unwrap().selected, 2);
        press(&mut m, KeyCode::Up);
        press(&mut m, KeyCode::Up);
        assert_eq!(m.dialog.as_ref().unwrap().selected, 0);
        press(&mut m, KeyCode::Enter);
        assert!(m.dialog.is_none());
        assert_eq!(m.page, Page::Sessions);
        assert_eq!(
            m.options.selected,
            crate::session_list::session_row(&m.sessions, 1)
        );
        assert!(!m.badge_focus);
        assert!(!m.badges().contains_key("s2"));
        assert!(m.badges().contains_key("s1"));
        let mut c = Canvas::new(100, 30);
        m.paint_at(&mut c, 29, 100);
        assert!(!screen_text(&c).contains("move failed"));
        // Persisted next to the preferences and restored after a restart.
        let saved = m.dismissed_out.take().unwrap();
        assert_eq!(saved, ["op2"]);
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("dismissed-operations.json");
        crate::state_cache::save_dismissed(&path, std::path::Path::new("/r"), &saved).unwrap();
        let mut reloaded = fresh();
        reloaded.dismissed = crate::state_cache::load_dismissed(&path, std::path::Path::new("/r"))
            .unwrap()
            .into_iter()
            .collect();
        assert!(!reloaded.badges().contains_key("s2"));
        assert!(
            crate::state_cache::load_dismissed(&path, std::path::Path::new("/other")).is_none()
        );
        // A new operation on the same session shows its badge again.
        reloaded.operations.push(
            json!({"id":"op3","kind":"move_space","session_id":"s2","phase":"failed","created_at":99}),
        );
        assert_eq!(reloaded.badges()["s2"].id, "op3");
        // Operations the backend no longer lists are pruned.
        let sent = reloaded.refresh();
        let token = sent
            .iter()
            .find_map(|i| match i {
                Intent::Api(token, args) if args[0] == "operations" => Some(*token),
                _ => None,
            })
            .unwrap();
        reloaded.response(
            token,
            Ok(json!([{"id":"op3","kind":"move_space","session_id":"s2","phase":"failed","created_at":99}])),
        );
        assert!(reloaded.dismissed.is_empty());
        assert_eq!(reloaded.dismissed_out.take().unwrap(), Vec::<String>::new());
    }
    #[test]
    fn dismissed_operation_details_offer_undismiss_which_restores_the_badge() {
        let mut m = setup();
        m.sessions_known = true;
        m.sessions = vec![
            json!({"id":"s2","name":"other","account_id":"codex:account1","availability":"saved","capabilities":{}}),
        ];
        m.operations = vec![
            json!({"id":"op2","kind":"move_space","session_id":"s2","phase":"failed","created_at":95}),
        ];
        m.go(Page::Sessions);
        let labels = |m: &Manager| {
            m.dialog
                .as_ref()
                .unwrap()
                .buttons
                .iter()
                .map(|b| b.label.clone())
                .collect::<Vec<_>>()
        };
        // Failed, not dismissed: Dismiss, Retry, Close.
        m.open_operation("op2".into());
        assert_eq!(labels(&m), ["Dismiss", "Retry", "Close"]);
        m.dismiss();
        m.dismiss_operation("op2");
        m.dismissed_out = None;
        assert!(!m.badges().contains_key("s2"));
        // Dismissed: the same option undoes it, with Close still focused.
        m.open_operation("op2".into());
        assert_eq!(labels(&m), ["Undismiss", "Close"]);
        assert_eq!(m.dialog.as_ref().unwrap().selected, 1);
        // A poll of the finished operation keeps the label and the focus.
        if let Some(d) = &mut m.dialog {
            finish_operation(d, true, Some(true));
        }
        assert_eq!(labels(&m), ["Undismiss", "Close"]);
        assert_eq!(m.dialog.as_ref().unwrap().selected, 1);
        m.paint_at(&mut Canvas::new(100, 30), 29, 100);
        m.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(m.dialog.is_none());
        // Removed from dismissed-operations.json; the badge shows again.
        assert!(m.dismissed.is_empty());
        assert_eq!(m.dismissed_out.take().unwrap(), Vec::<String>::new());
        assert_eq!(m.badges()["s2"].id, "op2");
        m.open_operation("op2".into());
        assert_eq!(labels(&m), ["Dismiss", "Retry", "Close"]);
    }
    #[test]
    fn retry_is_offered_only_for_failed_undismissed_operations_and_follows_can_recover() {
        let mut m = setup();
        m.operations = vec![
            json!({"id":"r1","kind":"restart","session_id":"s1","phase":"failed","can_recover":true,"created_at":50}),
            json!({"id":"r2","kind":"restart","session_id":"s1","phase":"failed","can_recover":false,"created_at":51}),
            json!({"id":"r3","kind":"restart","session_id":"s1","phase":"complete","created_at":52}),
        ];
        let labels = |m: &Manager| -> Vec<(String, bool)> {
            m.dialog
                .as_ref()
                .unwrap()
                .buttons
                .iter()
                .map(|b| (b.label.clone(), b.enabled))
                .collect()
        };
        m.open_operation("r1".into());
        assert_eq!(
            labels(&m),
            [
                ("Dismiss".into(), true),
                ("Retry".into(), true),
                ("Close".into(), true)
            ]
        );
        assert_eq!(
            m.dialog.as_ref().unwrap().selected,
            0,
            "Dismiss is the default"
        );
        m.paint_at(&mut Canvas::new(100, 30), 29, 100);
        // Retry asks the backend to recover after an explicit confirmation; nothing is sent yet.
        m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(
            m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                .is_empty()
        );
        assert_eq!(m.dialog.as_ref().unwrap().title, "Retry operation");
        assert!(matches!(&m.form, Form::Confirm(args) if args == &["recover", "r1"]));
        m.dismiss();
        m.open_operation("r2".into());
        m.paint_at(&mut Canvas::new(100, 30), 29, 100);
        assert_eq!(labels(&m)[1], ("Retry".into(), false));
        m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(
            matches!(m.form, Form::Operation(_)),
            "a disabled Retry does nothing"
        );
        m.dismiss();
        m.open_operation("r3".into());
        assert_eq!(
            labels(&m).iter().map(|l| l.0.as_str()).collect::<Vec<_>>(),
            ["Dismiss", "Close"]
        );
        m.dismiss();
        m.dismissed.insert("r1".into());
        m.open_operation("r1".into());
        assert_eq!(
            labels(&m).iter().map(|l| l.0.as_str()).collect::<Vec<_>>(),
            ["Undismiss", "Close"]
        );
    }
    #[test]
    fn inline_operations_render_as_launches_and_resumes_without_actions() {
        let mut m = setup();
        m.operations = vec![
            json!({"id":"i1","kind":"launch","inline":true,"name":"Fresh work","account_id":"codex:fixture0","phase":"attached","pane_id":"w1:p1","can_cancel":false,"can_respond":false,"can_recover":false,"created_at":60,"future_field":{"x":1}}),
            json!({"id":"i2","kind":"resume","inline":true,"session_id":"s1","phase":"failed","detail":"SESSION_NAME_EXISTS","can_cancel":false,"can_respond":false,"can_recover":false,"created_at":61}),
        ];
        m.go(Page::Operations);
        let text = m
            .rows()
            .iter()
            .map(|r| format!("{} {}", r.label, r.preview.clone().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("Fresh work")
                && text.contains("New session")
                && text.contains("in progress"),
            "{text}"
        );
        assert!(text.contains("Resume") && text.contains("failed"), "{text}");
        for id in ["i1", "i2"] {
            m.open_operation(id.into());
            let buttons: Vec<_> = m
                .dialog
                .as_ref()
                .unwrap()
                .buttons
                .iter()
                .map(|b| (b.label.clone(), b.enabled))
                .collect();
            for (label, enabled) in &buttons {
                assert!(
                    !matches!(
                        label.as_str(),
                        "Retry" | "Cancel operation" | "Respond yes" | "Respond no"
                    ) || !enabled,
                    "{id}: {label} must not be actionable"
                );
            }
            m.dismiss();
        }
        let rows = m.rows();
        let gated: Vec<_> = rows
            .iter()
            .filter(|r| matches!(r.label.as_str(), "Cancel operation" | "Recover"))
            .collect();
        assert!(gated.iter().all(|r| !r.enabled));
    }
    #[test]
    fn operation_ids_stay_out_of_the_list_and_details_are_titled_by_kind() {
        let mut m = setup();
        let a = "11111111-1111-4111-8111-111111111111";
        m.operations = vec![
            json!({"id":a,"kind":"move_space","phase":"failed","detail":"Space vanished.","created_at":50}),
        ];
        m.go(Page::Operations);
        for (width, height) in [(120, 24), (45, 16), (30, 16)] {
            let mut c = Canvas::new(width, height);
            m.paint(&mut c, height - 1);
            assert!(!screen_text(&c).contains(a), "{width}x{height}");
        }
        m.options.selected = 2;
        m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(m.page, Page::Operation(a.into()));
        assert_eq!(m.title(), "Move to Space");
        m.options.selected = 0;
        m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.title, "Move to Space");
        assert!(d.body.contains(&"Space vanished.".to_owned()));
        let id_line = d.body.iter().position(|l| l.contains(a)).unwrap();
        assert!(d.dim_from.is_some_and(|i| i <= id_line), "IDs are dim");
    }
    fn operations_manager() -> Manager {
        let mut m = setup();
        m.operations = vec![
            json!({"id":"old","kind":"switch","phase":"complete","created_at":10,"updated_at":1000}),
            json!({"id":"new","kind":"account_delete","account_id":"codex:fixture0","phase":"running","created_at":20,"updated_at":1}),
            json!({"id":"legacy","kind":"move_space","phase":"failed","detail":"Destination Space is gone.","updated_at":2000}),
        ];
        m.go(Page::Operations);
        m
    }
    fn press_key(m: &mut Manager, code: KeyCode) {
        m.key(KeyEvent::new(code, KeyModifiers::NONE));
    }
    #[test]
    fn operations_filter_picker_grabs_changes_without_wrap_commits_and_reverts() {
        let mut m = operations_manager();
        m.options.selected = 0;
        assert_eq!(m.rows()[0].preview.as_deref(), Some("↓ All"));
        for code in [KeyCode::Right, KeyCode::Enter, KeyCode::Char(' ')] {
            press_key(&mut m, code);
            assert!(m.filter_grab.is_some(), "{code:?} grabs");
            assert!(m.rows()[0].preview_grabbed);
            press_key(&mut m, KeyCode::Left);
            assert!(m.filter_grab.is_none());
            assert!(!m.options.back, "Left committed instead of going back");
        }
        press_key(&mut m, KeyCode::Right);
        press_key(&mut m, KeyCode::Up);
        assert_eq!(
            m.operation_filter,
            OperationFilter::All,
            "no wrap at the top"
        );
        for _ in 0..6 {
            press_key(&mut m, KeyCode::Down);
        }
        assert_eq!(
            m.operation_filter,
            OperationFilter::Completed,
            "no wrap at the bottom"
        );
        assert_eq!(m.rows()[0].preview.as_deref(), Some("↑ Completed"));
        press_key(&mut m, KeyCode::Up);
        assert_eq!(m.rows()[0].preview.as_deref(), Some("↕ Failed"));
        // Enter commits; the filter stays.
        press_key(&mut m, KeyCode::Enter);
        assert!(m.filter_grab.is_none());
        assert_eq!(m.operation_filter, OperationFilter::Failed);
        // Esc reverts and does not leave the page.
        press_key(&mut m, KeyCode::Right);
        press_key(&mut m, KeyCode::Up);
        press_key(&mut m, KeyCode::Esc);
        assert_eq!(m.operation_filter, OperationFilter::Failed);
        assert_eq!(m.page, Page::Operations);
        assert!(m.close_armed.is_none());
        // Another row dims while grabbed.
        press_key(&mut m, KeyCode::Right);
        let mut c = Canvas::new(60, 16);
        m.paint(&mut c, 15);
        assert!(screen_text(&c).contains("Choose"), "{}", screen_text(&c));
        press_key(&mut m, KeyCode::Esc);
        // Left reaches the back arrow once the picker is not grabbed.
        press_key(&mut m, KeyCode::Left);
        assert!(m.options.back);
        assert_eq!(m.page, Page::Operations);
    }
    #[test]
    fn operations_list_does_not_move_when_the_filter_changes() {
        let mut m = operations_manager();
        let xs = |m: &mut Manager| -> (usize, usize, usize) {
            let mut c = Canvas::new(80, 16);
            m.paint(&mut c, 15);
            let lines: Vec<String> = screen_text(&c).lines().map(str::to_owned).collect();
            let col = |needle: &str| {
                lines
                    .iter()
                    .find_map(|l| l.find(needle).map(|b| l[..b].chars().count()))
                    .unwrap_or(usize::MAX)
            };
            (
                col("Show"),
                col("Today").min(col("Undated")),
                col("›").min(col("↕")),
            )
        };
        let all = xs(&mut m);
        for f in OperationFilter::ALL {
            m.operation_filter = f;
            let got = xs(&mut m);
            assert_eq!((got.0, got.2), (all.0, all.2), "{f:?}");
        }
        m.operation_filter = OperationFilter::Failed;
        let failed = m.rows();
        m.operation_filter = OperationFilter::All;
        let rows = m.rows();
        let entry_column = |r: &Vec<Row>| {
            r.iter().find_map(|r| {
                r.preview_column
                    .filter(|_| !r.separator && r.label != "Show")
            })
        };
        assert_eq!(entry_column(&failed), entry_column(&rows));
        let width = |r: &Vec<Row>| Options::left(80, r);
        for f in OperationFilter::ALL {
            m.operation_filter = f;
            assert_eq!(width(&m.rows()), width(&rows), "{f:?}");
        }
        let _ = all.1;
    }
    #[test]
    fn no_left_right_chevron_selector_remains_in_the_sources() {
        for (name, src) in [
            ("widgets", include_str!("widgets.rs")),
            ("management", include_str!("management.rs")),
            ("app", include_str!("app.rs")),
            ("session_list", include_str!("session_list.rs")),
        ] {
            let code = src.split("#[cfg(test)]").next().unwrap();
            assert!(!code.contains('‹'), "{name} still draws a left chevron");
        }
    }
    #[test]
    fn scrolling_operations_keep_the_filter_and_a_range_counter() {
        let mut m = setup();
        m.operations = (0..10)
            .map(|i| json!({"id":format!("op{i}"),"kind":"switch","phase":"complete","created_at":50}))
            .collect();
        m.go(Page::Operations);
        let mut c = Canvas::new(45, 16);
        m.paint(&mut c, 15);
        let text = screen_text(&c);
        assert!(text.contains("Show ›") && text.contains(" All"), "{text}");
        assert!(text.contains(" of 12"), "{text}");
        m.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        let mut c = Canvas::new(45, 16);
        m.paint(&mut c, 15);
        assert!(screen_text(&c).contains("12 of 12"));
        m.key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert!(m.options.selected < 11);
    }
    #[test]
    fn deletion_preview_cancels_without_mutation_and_confirmation_delegates_once() {
        let mut m = setup();
        m.capabilities["providers"][0]["account_deletion"] = json!(true);
        m.go(Page::Account("codex:account1".into()));
        m.options.selected = m
            .rows()
            .iter()
            .position(|r| r.label == "Delete account")
            .unwrap();
        let sent = m.activate();
        let Intent::Api(token, ref args) = sent[0] else {
            panic!()
        };
        assert_eq!(args, &strings(&["account", "delete", "codex:account1"]));
        assert_eq!(m.dialog.as_ref().unwrap().body, vec!["Loading…"]);
        m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let preview =
            json!({"account_id":"codex:account1","can_delete":true,"file_scanning":false});
        m.response(token, Ok(preview.clone()));
        assert!(m.dialog.is_none());
        let sent = m.activate();
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.response(token, Ok(preview));
        assert!(
            m.dialog.as_ref().unwrap().fields[0]
                .label
                .contains("codex-account1")
        );
        assert!(m.submit(0, vec!["codex:account1".into()]).is_empty());
        // Exercise the actual small-popup input path, not submit directly.
        let d = m.dialog.as_mut().unwrap();
        d.status = None;
        d.paste("codex-account1");
        d.paint(&mut Canvas::new(40, 18), &Palette::default());
        assert!(d.offset + d.visible < d.wrapped);
        let sent = m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let Intent::Api(token, ref args) = sent[0] else {
            panic!()
        };
        assert!(
            args.windows(2)
                .any(|w| w == ["--confirm", "codex:account1"])
        );
        assert!(args.contains(&"--request-id".into()));
        m.response(
            token,
            Ok(json!({"id":"deletion-1","phase":"admitted","can_cancel":false})),
        );
        assert!(matches!(&m.form, Form::Operation(id) if id == "deletion-1"));
        assert!(m.submit(0, vec![]).is_empty()); // detach, never cancel/replay
        assert!(m.dialog.is_none());
    }
    #[test]
    fn deletion_requires_updated_owner_and_blocks_active_account_without_confirmation() {
        for preview in [
            json!({"can_delete":false,"owner_upgrade_required":true}),
            json!({"can_delete":false,"owner_upgrade_required":false}),
        ] {
            let mut m = setup();
            m.go(Page::Account("codex:account1".into()));
            let sent = m.preview(
                "Delete account",
                strings(&["account", "delete", "codex:account1"]),
                Purpose::Delete("codex:account1".into()),
            );
            let Intent::Api(token, _) = sent[0] else {
                panic!()
            };
            m.response(token, Ok(preview));
            assert!(m.dialog.as_ref().unwrap().fields.is_empty());
            assert!(matches!(m.form, Form::None));
        }
    }
    #[test]
    fn cancelled_preview_cannot_reopen_and_friendly_confirmation_maps_to_backend_id() {
        let mut m = setup();
        m.go(Page::Account("codex:account1".into()));
        m.options.selected = m
            .rows()
            .iter()
            .position(|r| r.label == "Deregister account")
            .unwrap();
        let sent = m.activate();
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        assert_eq!(m.dialog.as_ref().unwrap().body, vec!["Loading…"]);
        m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(m.dialog.is_none());
        let preview = json!({"account_id":"codex:account1","can_remove":true,"revision":"preview","stored_data":{},"affected_sessions":[]});
        m.response(token, Ok(preview.clone()));
        assert!(m.dialog.is_none());
        let sent = m.activate();
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.response(token, Ok(preview));
        assert!(
            m.dialog.as_ref().unwrap().fields[0]
                .label
                .contains("codex-account1")
        );
        m.submit(0, vec!["wrong".into()]);
        m.submit(0, vec!["wrong again".into()]);
        assert_eq!(
            m.dialog.as_ref().unwrap().status.as_deref(),
            Some("Type codex-account1 to confirm.")
        );
        let sent = m.submit(0, vec!["codex-account1".into()]);
        let Intent::Api(_, args) = &sent[0] else {
            panic!()
        };
        assert!(
            args.windows(2)
                .any(|w| w == ["--confirm", "codex:account1"])
        );
    }
    #[test]
    fn pending_inventory_keeps_add_account_until_actual_rows_overflow() {
        let mut m = Manager::default();
        assert!(!m.inventory_known);
        m.paint(&mut Canvas::new(80, 30), 29);
        assert_eq!(m.rows()[2].label, "Add account");
        m.options.selected = 2;
        m.activate();
        assert_eq!(m.dialog.as_ref().unwrap().title, "Add account");
        m.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        m.inventory(vec![account(1), account(2)]);
        m.paint(&mut Canvas::new(80, 30), 29);
        assert_eq!(m.rows()[2].label, "Add account");
        assert_eq!(m.options.selected, 2);
        m.inventory((0..30).map(account).collect());
        m.paint(&mut Canvas::new(80, 30), 29);
        assert_eq!(m.rows()[2].label, "Add account");
        assert_eq!(m.options.selected, 2);
    }
    #[test]
    fn grouped_actions_skip_separators_and_confirmation_binds_original_generation() {
        let mut m = setup();
        m.go(Page::Account("codex:account1".into()));
        assert!(m.rows()[2].separator && m.rows()[4].separator);
        m.options.selected = 0;
        m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(m.options.selected, 1);
        m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(m.options.selected, 3);
        let s = json!({"id":"stable","name":"Test","generation":"old","availability":"running","capabilities":{"stop":true}});
        m.sessions = vec![s];
        m.go(Page::Session("stable".into()));
        m.options.selected = m
            .rows()
            .iter()
            .position(|r| r.label == "End session")
            .unwrap();
        let checked = m.activate();
        let Intent::Api(check, _) = &checked[0] else {
            panic!()
        };
        m.response(
            *check,
            Ok(json!({"generation":"old","freshness":"fresh","runtime_state":"idle"})),
        );
        m.sessions[0]["generation"] = json!("replacement");
        let sent = m.submit(0, vec![]);
        let Intent::Api(_, args) = &sent[0] else {
            panic!()
        };
        assert!(args.contains(&"old".into()));
        assert!(!args.contains(&"replacement".into()));
        assert!(m.submit(0, vec![]).is_empty());
        assert!(m.dialog.is_none()); // background never resubmits
    }
    #[test]
    fn limits_translate_remaining_and_fail_closed_without_replaying() {
        let mut m = setup();
        m.form = Form::Limits("codex:account1".into());
        m.dialog = Some(Dialog::new("Limits", vec![], &["Save"]));
        assert!(m.submit(0, vec!["NaN".into(), "20".into()]).is_empty());
        let sent = m.submit(0, vec!["20".into(), "10".into()]);
        let Intent::Api(token, args) = &sent[0] else {
            panic!()
        };
        assert_eq!(
            args,
            &strings(&[
                "account",
                "limits",
                "codex:account1",
                "--short-used",
                "80",
                "--weekly-used",
                "90"
            ])
        );
        m.submit(0, vec![]);
        assert!(m.dialog.is_none());
        assert!(m.response(*token, Err("BACKEND_TIMEOUT".into())).is_empty());
        assert!(m.dialog.is_none());
        assert!(m.note.contains("BACKEND_TIMEOUT"));
    }
    #[test]
    fn late_account_preview_cannot_open_over_another_account() {
        let mut m = setup();
        m.go(Page::Account("codex:account1".into()));
        m.options.selected = m
            .rows()
            .iter()
            .position(|r| r.label == "Deregister account")
            .unwrap();
        let sent = m.activate();
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.go(Page::Account("codex:account2".into()));
        m.response(
            token,
            Ok(json!({"can_remove":true,"account_id":"codex:account1","revision":"old"})),
        );
        assert!(m.dialog.is_none());
    }
    fn claude(name: &str) -> Account {
        serde_json::from_value(json!({"id":format!("claude:{name}"),"provider":"claude","selectable":true,"enabled":true,"usage_fresh":true,"usage":{"state":"fresh","source":"claude-code-statusline","semantics":"Claude Code statusline rate limits","observed_at":100,"short":{"duration_minutes":300,"used_percent":40,"resets_at":1_800_000_000},"weekly":null}})).unwrap()
    }
    fn dual_providers() -> Manager {
        let mut m = Manager::default();
        m.inventory(vec![account(1), claude("claude-main"), account(2)]);
        m.capabilities = json!({"providers":[
            {"id":"codex","implemented":true,"account_limits":true,"enrollment":true,"account_deletion":true,"account_removal":true},
            {"id":"claude","implemented":true,"account_limits":false,"enrollment":true,"account_deletion":false,"account_removal":false}
        ]});
        m
    }
    fn claude_session(availability: &str) -> Value {
        json!({"id":"c","generation":"g","name":"Work","provider":"claude","account_id":"claude:claude-main","conversation_id":"native-1","directory":null,"availability":availability,"native_status":"idle","native_name":null,"attachment":null,"capabilities":{"focus":false,"rename":true,"resume":availability=="stopped"}})
    }
    #[test]
    fn add_account_dialogs_put_a_separator_above_cancel() {
        use crate::dialog::Event;
        let mut m = dual_providers();
        m.add();
        let d = m.dialog.as_ref().unwrap();
        let labels: Vec<_> = d
            .buttons
            .iter()
            .map(|b| (b.label.as_str(), b.separator))
            .collect();
        let n = labels.len();
        assert!(d.buttons[n - 2].separator && d.buttons[n - 1].label == "Cancel");
        // Arrow navigation skips the separator.
        let d = m.dialog.as_mut().unwrap();
        d.selected = n - 3;
        d.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(d.selected, n - 1);
        d.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(d.selected, n - 3);
        assert!(matches!(
            { d.selected = n - 1; d.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)) },
            Event::Choose(i, _) if i == n - 1
        ));
        // Provider choice and Cancel still resolve through the raw button index.
        m.submit(1, vec![]);
        let d = m.dialog.as_ref().unwrap();
        assert!(d.body[0].starts_with("Claude Code account name"));
        let n = d.buttons.len();
        assert!(d.buttons[n - 2].separator && d.buttons[n - 1].label == "Cancel");
        m.submit(n - 1, vec![]);
        assert!(m.dialog.is_none());
    }
    #[test]
    fn add_account_chooses_provider_then_runs_the_provider_flow() {
        let mut m = dual_providers();
        m.add();
        let d = m.dialog.as_ref().unwrap();
        let buttons: Vec<_> = d.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(buttons, ["Codex", "Claude Code", "", "Cancel"]);
        assert!(d.fields.is_empty());
        m.submit(1, vec![]);
        let d = m.dialog.as_ref().unwrap();
        assert!(d.body[0].starts_with("Claude Code account name"));
        // Names are unique per provider only: a Codex name may be reused for Claude.
        assert!(!d.fields[0].excluded.contains(&"account1".to_owned()));
        assert!(d.fields[0].excluded.contains(&"claude-main".to_owned()));
        let sent = m.submit(0, vec!["claude-two".into()]);
        assert!(
            matches!(&sent[..], [Intent::Api(_, args)] if args == &strings(&["account", "add", "claude:claude-two"]))
        );
        let mut m = dual_providers();
        m.add();
        m.submit(0, vec![]);
        let sent = m.submit(0, vec!["codex-two".into()]);
        assert!(
            matches!(&sent[..], [Intent::Api(_, args)] if args == &strings(&["account", "add", "codex:codex-two"]))
        );
        // Cancel at the provider step mutates nothing.
        let mut m = dual_providers();
        m.add();
        assert!(m.submit(2, vec![]).is_empty());
        assert!(m.dialog.is_none());
    }
    #[test]
    fn single_provider_add_skips_the_picker_and_invalid_names_are_rejected() {
        let mut m = setup();
        m.add();
        assert!(m.dialog.as_ref().unwrap().body[0].starts_with("Codex account name"));
        assert!(m.submit(0, vec!["bad name".into()]).is_empty());
        assert!(m.dialog.as_ref().unwrap().body[0].starts_with("Use 1–"));
    }
    #[test]
    fn existing_claude_home_and_unauthenticated_add_are_explained() {
        let mut m = dual_providers();
        m.add_for("claude");
        let sent = m.submit(0, vec!["adopted".into()]);
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.response(
            token,
            Ok(json!({"existing_home":true,"auth":{"logged_in":true}})),
        );
        let d = m.dialog.as_ref().unwrap();
        assert!(d.body[0].contains("registered unchanged"));
        assert_eq!(d.body.len(), 1);
    }
    #[test]
    fn logout_row_is_gated_by_capability_and_confirms_before_running() {
        let mut m = dual_providers();
        let labels =
            |m: &Manager| -> Vec<String> { m.rows().iter().map(|r| r.label.clone()).collect() };
        m.go(Page::Account("claude:claude-main".into()));
        // Older backend: no capability, no row; logged in so no Log in either.
        assert!(!labels(&m).iter().any(|l| l == "Log out" || l == "Log in"));
        m.capabilities["providers"][1]["account_logout"] = json!(false);
        m.capabilities["providers"][1]["operations"] =
            json!({"account_logout":{"reason":"not yet available"}});
        let at = labels(&m).iter().position(|l| l == "Log out").unwrap();
        assert!(!m.rows()[at].enabled);
        m.capabilities["providers"][1]["account_logout"] = json!(true);
        let l = labels(&m);
        assert_eq!(l[..2], ["Details", "Rename"]);
        assert_eq!(l[3..5], ["Limits", "Log out"]);
        assert!(m.rows()[at].enabled);
        m.options.selected = at;
        m.activate();
        let d = m.dialog.as_ref().unwrap();
        assert!(d.body[0].starts_with("Sign out of claude-claude-main? Sessions keep"));
        let sent = m.submit(0, vec![]);
        assert!(
            matches!(&sent[..], [Intent::Api(_, a)] if a == &strings(&["account", "logout", "claude:claude-main"]))
        );
        // Logged-out accounts offer Log in and no Limits or Log out.
        m.accounts[1]
            .details
            .insert("auth_required".into(), json!(true));
        let l = labels(&m);
        assert!(l.contains(&"Log in".to_owned()));
        assert!(!l.iter().any(|x| x == "Limits" || x == "Log out"));
    }
    #[test]
    fn claude_account_login_uses_the_account_provider_capability() {
        let mut m = dual_providers();
        m.accounts[1]
            .details
            .insert("auth_required".into(), json!(true));
        m.go(Page::Account("claude:claude-main".into()));
        let login = m.rows().iter().position(|r| r.label == "Log in").unwrap();
        assert!(m.rows()[login].enabled);
        m.options.selected = login;
        m.activate();
        m.login_screen("claude:claude-main".into(), Ok(None));
        let sent = m.submit(0, vec![]);
        assert!(
            matches!(&sent[..], [Intent::Api(_, args)] if args == &strings(&["account", "login", "claude:claude-main"]))
        );
        // Another provider's flag never enables this provider's action.
        m.capabilities["providers"][1]["enrollment"] = json!(false);
        m.go(Page::Account("claude:claude-main".into()));
        assert!(
            !m.rows()
                .iter()
                .find(|r| r.label == "Log in")
                .unwrap()
                .enabled
        );
    }
    #[test]
    fn provider_capabilities_gate_limits_and_deletion_per_account() {
        let mut m = dual_providers();
        m.go(Page::Account("claude:claude-main".into()));
        let rows = m.rows();
        assert!(!rows.iter().find(|r| r.label == "Limits").unwrap().enabled);
        assert!(!m.delete_account_row("claude:claude-main").enabled);
        assert!(
            !rows
                .iter()
                .find(|r| r.label == "Deregister account")
                .unwrap()
                .enabled
        );
        m.go(Page::Account("codex:account1".into()));
        let rows = m.rows();
        assert!(rows.iter().find(|r| r.label == "Limits").unwrap().enabled);
        assert!(m.delete_account_row("codex:account1").enabled);
        assert!(
            rows.iter()
                .find(|r| r.label == "Deregister account")
                .unwrap()
                .enabled
        );
    }
    #[test]
    fn usage_details_label_source_semantics_age_and_missing_windows() {
        let lines = usage_lines(&claude("claude-main"), 400);
        assert_eq!(fact(&lines, "Account").as_deref(), Some("✻ claude-main"));
        assert_eq!(fact(&lines, "Provider").as_deref(), Some("Claude Code"));
        assert_eq!(
            fact(&lines, "Usage").as_deref(),
            Some("fresh · claude-code-statusline")
        );
        assert_eq!(
            fact(&lines, "Semantics").as_deref(),
            Some("Claude Code statusline rate limits")
        );
        assert_eq!(fact(&lines, "Observed").as_deref(), Some("5m ago"));
        // Both meanings, joined with the small bullet.
        assert!(
            fact(&lines, "5h window")
                .unwrap()
                .starts_with("60% left · 40% used · resets ")
        );
        assert_eq!(fact(&lines, "Weekly").as_deref(), Some("unavailable"));
        assert!(!lines.join("\n").contains('/'), "no slashes between values");
        let mut missing = claude("claude-new");
        missing.usage = json!({"state":"missing","observed_at":null,"short":null,"weekly":null});
        let lines = usage_lines(&missing, 400);
        assert_eq!(fact(&lines, "Status").as_deref(), Some("Usage unavailable"));
        assert!(fact(&lines, "Observed").is_none());
        for bogus in [json!(0), json!(null), json!(-5), json!(10_000)] {
            let mut g = gemini("gem-main");
            g.usage["observed_at"] = bogus;
            let text = usage_lines(&g, 400).join("\n");
            assert!(!text.contains("Observed") && !text.contains("ago"));
            assert_eq!(Usage::account_at(&g, 400).age, None);
        }
        let mut codex = account(1);
        codex.usage["weekly"] = Value::Null;
        assert_eq!(
            fact(&usage_lines(&codex, 400), "Weekly").as_deref(),
            Some("unavailable")
        );
    }
    #[test]
    fn account_details_explain_the_threshold_marker_in_a_dim_legend() {
        let mut home = claude("main");
        home.details
            .insert("config_home".into(), json!("/data/accounts/claude-main"));
        let lines = usage_lines(&home, 400);
        assert_eq!(
            fact(&lines, "Directory").as_deref(),
            Some("/data/accounts/claude-main")
        );
        assert!(fact(&usage_lines(&account(1), 400), "Directory").is_none());
        let mut a = account(1);
        a.details.insert(
            "switch_thresholds".into(),
            json!({"customized":true,"short_percent":80,"weekly_percent":90}),
        );
        let mut m = setup();
        m.show_usage(&a);
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.title, "Account details");
        let legend = d
            .body
            .iter()
            .position(|l| l.starts_with("┿ marks"))
            .unwrap();
        assert!(d.body[legend].contains("(20% left 5h · 10% left wk)"));
        assert_eq!(d.dim_from, Some(legend));
        assert!(d.body[legend..].iter().any(|l| l.contains("Account ID")));
        // Without a saved threshold there is no legend.
        let mut plain = account(2);
        plain.details.remove("switch_thresholds");
        let lines = usage_lines(&plain, 400);
        assert!(!lines.iter().any(|l| l.starts_with("┿")));
    }
    #[test]
    fn details_popups_hang_wrapped_values_under_the_value_column() {
        // (value column of the `label` row, first text column of the next row).
        fn hang(m: &mut Manager, label: &str) -> (usize, usize) {
            let mut c = Canvas::new(48, 30);
            let palette = m.palette.clone();
            let d = m.dialog.as_mut().unwrap();
            assert!(d.hanging, "{label}");
            d.paint(&mut c, &palette);
            let rows: Vec<Vec<char>> = c
                .cells
                .chunks(48)
                .map(|r| {
                    r.iter()
                        .map(|c| c.glyph.chars().next().unwrap_or(' '))
                        .collect()
                })
                .collect();
            let y = rows
                .iter()
                .position(|r| r.iter().collect::<String>().contains(label))
                .unwrap();
            let line: String = rows[y].iter().collect();
            let at = line[..line.find(label).unwrap()].chars().count() + label.chars().count();
            let value = (at..48).find(|&x| rows[y][x] != ' ').unwrap();
            let border = rows[y + 1].iter().position(|&c| c == '│');
            let next = border.and_then(|b| (b + 1..48).find(|&x| rows[y + 1][x] != ' '));
            (value, next.unwrap_or(0))
        }
        let long = "/data/accounts/claude-main/with/a/rather/long/home/directory";
        let mut a = claude("main");
        a.details.insert("config_home".into(), json!(long));
        let mut m = setup();
        m.show_usage(&a);
        let (value, next) = hang(&mut m, "Directory:");
        assert_eq!(next, value, "account details");
        // Values above share the column the continuation hangs at.
        let (status, _) = hang(&mut m, "Status:");
        assert_eq!(status, value);
        let mut m = setup();
        m.sessions = vec![json!({"id":"one","name":"Long","generation":"g","directory":long})];
        m.go(Page::Session("one".into()));
        m.options.selected = 1;
        m.activate();
        let (value, next) = hang(&mut m, "Directory:");
        assert_eq!(next, value, "session details");
        let mut m = setup();
        let key = "11111111-1111-4111-8111-111111111111";
        m.operations = vec![json!({"id":key,"kind":"move_space","phase":"failed","created_at":5})];
        m.open_operation(key.into());
        let (value, next) = hang(&mut m, "Operation ID");
        assert_eq!(next, value, "operation details");
    }
    fn fact(lines: &[String], label: &str) -> Option<String> {
        lines
            .iter()
            .find_map(|l| l.strip_prefix(&format!("{label}:")))
            .map(|v| v.trim().to_owned())
    }
    #[test]
    fn session_details_share_one_layout_for_every_provider() {
        let mut live = claude_session("running");
        live["attachment"] =
            json!({"host":"herdr","target":"herdr/default/w:p","pid":4242,"process_start":1});
        live["native_name"] = json!("Native title");
        live["directory"] = json!("/work/example");
        live["last_accessed"] = json!(100 - 300);
        live["conversation_created_at"] = json!("1969-12-29T00:01:40Z");
        let lines = Manager::session_detail_lines(&live);
        let labels: Vec<_> = lines
            .iter()
            .take(5)
            .map(|l| l.split(':').next().unwrap().to_owned())
            .collect();
        assert_eq!(
            labels,
            ["Account", "Status", "Directory", "Last active", "Created"]
        );
        assert_eq!(fact(&lines, "Account").as_deref(), Some("✻ claude-main"));
        assert_eq!(fact(&lines, "Status").as_deref(), Some("running · idle"));
        assert_eq!(fact(&lines, "Last active").as_deref(), Some("5m ago"));
        assert_eq!(fact(&lines, "Created").as_deref(), Some("3d ago"));
        let ids = lines.iter().position(|l| l == "Identifiers").unwrap();
        let rest = lines[ids..].join("\n");
        assert!(rest.contains("Native session  native-1"));
        assert!(rest.contains("4242"));
        // Values line up in one column across both groups.
        let column = |l: &String| l.find("native-1").or_else(|| l.find("✻"));
        assert_eq!(
            lines[0].chars().position(|c| c == '✻'),
            lines
                .iter()
                .find(|l| l.contains("native-1"))
                .and_then(|l| l.find("native-1"))
        );
        let _ = column;
        // The same labels for Codex; no generation hash.
        let codex = json!({"id":"x","name":"n","account_id":"codex:work","availability":"saved",
            "generation":"gen-hash","conversation_id":"conv","history_file_bytes":24576});
        let lines = Manager::session_detail_lines(&codex);
        assert_eq!(fact(&lines, "Account").as_deref(), Some("⏣ work"));
        assert_eq!(fact(&lines, "Status").as_deref(), Some("saved"));
        assert_eq!(fact(&lines, "History").as_deref(), Some("24 KB"));
        assert!(!lines.join("\n").contains("gen-hash"));
        let stopped = Manager::session_detail_lines(&claude_session("stopped"));
        assert_eq!(fact(&stopped, "Directory").as_deref(), Some("unknown"));
        assert!(!stopped.join("\n").contains("Process"));
        for (state, label) in [("stale", "stale"), ("unknown", "unknown")] {
            let lines = Manager::session_detail_lines(&claude_session(state));
            assert_eq!(fact(&lines, "Status").as_deref(), Some(label));
        }
        assert_eq!(parse_time("1970-01-02T00:00:00Z"), Some(86_400));
        assert_eq!(parse_time("1970-01-01T01:00:00+01:00"), Some(0));
        assert_eq!(parse_time("nonsense"), None);
    }
    #[test]
    fn claude_rename_is_a_label_change_without_an_observation_round_trip() {
        let mut m = Manager {
            sessions: vec![{
                let mut s = claude_session("running");
                s["attachment"] = json!({"host":"herdr","pid":1});
                s["capabilities"]["focus"] = json!(true);
                s
            }],
            page: Page::Session("c".into()),
            ..Default::default()
        };
        let labels: Vec<_> = m.rows().iter().map(|r| r.label.clone()).collect();
        assert_eq!(
            labels[..3],
            ["Focus session", "Session details", "Rename session"]
        );
        assert!(m.rows()[3..].iter().all(|r| !r.enabled));
        m.options.selected = 2;
        // No `observe` request: Claude has no provider observation to wait for.
        assert!(m.activate().is_empty());
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.fields[0].label, "Sessionsation name");
        assert!(d.body[0].contains("next time"));
        let sent = m.submit(0, vec!["Renamed".into()]);
        assert!(matches!(&sent[..], [Intent::Api(_, args)]
            if args[0] == "rename" && args.ends_with(&strings(&["--name", "Renamed"]))));
        m.page = Page::Session("c".into());
        m.options.selected = 0;
        assert!(matches!(&m.activate()[..], [Intent::Focus(s)] if s["id"] == "c"));
    }
    #[test]
    fn claude_focus_and_start_never_launch_for_stopped_stale_or_detached_sessions() {
        for (availability, attachment) in [("running", Value::Null), ("stale", Value::Null)] {
            let mut s = claude_session(availability);
            s["attachment"] = attachment;
            let mut m = Manager {
                sessions: vec![s],
                page: Page::Session("c".into()),
                ..Default::default()
            };
            m.options.selected = 0;
            assert!(!m.rows()[0].enabled);
            assert!(
                m.activate().is_empty(),
                "{availability}: disabled row sends nothing"
            );
        }
    }
    fn triple_providers() -> Manager {
        let mut m = dual_providers();
        let mut all = m.accounts.clone();
        all.push(gemini("gem-main"));
        m.inventory(all);
        m.capabilities["providers"].as_array_mut().unwrap().push(json!({"id":"gemini","implemented":true,"enrollment":true,"account_limits":false,"operations":{"account_limits":{"supported":false,"reason":"Gemini CLI exposes no usage limits"}}}));
        m
    }
    fn gemini(name: &str) -> Account {
        serde_json::from_value(json!({"id":format!("gemini:{name}"),"provider":"gemini","selectable":true,"enabled":true,"usage_fresh":false,"usage_source":"gemini-cli-unsupported","usage":{"state":"unsupported","source":"gemini-cli-unsupported","observed_at":null,"short":null,"weekly":null}})).unwrap()
    }
    fn gemini_session(native_status: Value) -> Value {
        json!({"id":"g","generation":"g","name":"Work","provider":"gemini","account_id":"gemini:gem-main","conversation_id":"gem-native-1","directory":"/d","availability":"running","native_status":native_status,"attachment":{"host":"herdr","pid":7},"capabilities":{"focus":true,"rename":true,"resume":false,"add_to_herdr":false},"capability_reasons":{"add_to_herdr":"NO_NATIVE_ATTACH"}})
    }
    #[test]
    fn gemini_is_a_third_provider_in_add_filters_tags_and_limits() {
        let mut m = triple_providers();
        m.add();
        let d = m.dialog.as_ref().unwrap();
        let buttons: Vec<_> = d.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(
            buttons,
            ["Codex", "Claude Code", "Gemini CLI", "", "Cancel"]
        );
        m.submit(2, vec![]);
        let d = m.dialog.as_ref().unwrap();
        assert!(d.body[0].starts_with("Gemini CLI account name"));
        assert!(d.body[1].contains("Gemini CLI's own interactive sign-in"));
        let sent = m.submit(0, vec!["gem-two".into()]);
        assert!(
            matches!(&sent[..], [Intent::Api(_, args)] if args == &strings(&["account", "add", "gemini:gem-two"]))
        );
        // Unsupported usage is stated, never 0%; Limits is disabled with the backend reason.
        let u = Usage::account(&gemini("gem-main"));
        assert_eq!(u.status, "Usage unsupported");
        assert!(u.status_only());
        m.go(Page::Account("gemini:gem-main".into()));
        let rows = m.rows();
        assert!(!rows.iter().any(|r| r.label == "Limits"));
        let lines = usage_lines(&m.accounts[3], 0);
        assert_eq!(fact(&lines, "Provider").as_deref(), Some("Gemini CLI"));
        assert_eq!(fact(&lines, "Status").as_deref(), Some("Usage unsupported"));
        assert!(
            fact(&lines, "Usage")
                .unwrap()
                .contains("gemini-cli-unsupported")
        );
        assert!(fact(&lines, "5h window").is_none() && fact(&lines, "Weekly").is_none());
        assert!(!lines.join("\n").contains('%'));
    }
    #[test]
    fn gemini_login_plan_uses_the_pane_flow_and_wording() {
        let mut m = triple_providers();
        m.add_for("gemini");
        let sent = m.submit(0, vec!["new".into()]);
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        let out = m.response(token, Ok(json!({"login":{"action":"exec","argv":["sessionsation","account","login","gemini:new"],"credentials_in_client":false},"auth":{"logged_in":false}})));
        assert!(
            matches!(&out[..], [Intent::LaunchLogin(argv)] if argv == &strings(&["sessionsation","account","login","gemini:new"]))
        );
        assert_eq!(
            m.dialog.as_ref().unwrap().body[0],
            "Complete the Gemini CLI login in the opened pane…"
        );
    }
    fn antigravity(name: &str) -> Account {
        serde_json::from_value(json!({"id":format!("antigravity:{name}"),"provider":"antigravity","selectable":true,"enabled":true,"usage_fresh":false,"usage_state":"unsupported","usage_source":"antigravity-unsupported","usage":{"state":"unsupported","source":"antigravity-unsupported","observed_at":null,"short":null,"weekly":null}})).unwrap()
    }
    fn quad_providers() -> Manager {
        let mut m = triple_providers();
        let mut all = m.accounts.clone();
        all.push(antigravity("agy-main"));
        m.inventory(all);
        m.capabilities["providers"].as_array_mut().unwrap().push(json!({"id":"antigravity","implemented":true,"enrollment":true,"account_limits":false,"account_logout":false,"usage":false,"usage_state":"unsupported","operations":{"account_limits":{"supported":false,"reason":"Antigravity CLI exposes no usage limits"}}}));
        m
    }
    #[test]
    fn antigravity_is_a_fourth_provider_in_add_filters_tags_and_limits() {
        let mut m = quad_providers();
        m.add();
        let d = m.dialog.as_ref().unwrap();
        let buttons: Vec<_> = d.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(
            buttons,
            [
                "Codex",
                "Claude Code",
                "Antigravity CLI",
                "Gemini CLI",
                "",
                "Cancel"
            ]
        );
        m.submit(2, vec![]);
        let d = m.dialog.as_ref().unwrap();
        assert!(d.body[0].starts_with("Antigravity CLI account name"));
        assert!(d.body[1].contains("Antigravity CLI's own interactive sign-in"));
        let sent = m.submit(0, vec!["agy-two".into()]);
        assert!(
            matches!(&sent[..], [Intent::Api(_, args)] if args == &strings(&["account", "add", "antigravity:agy-two"]))
        );
        let u = Usage::account(&antigravity("agy-main"));
        assert_eq!(u.status, "Usage unsupported");
        assert!(u.status_only());
        m.go(Page::Account("antigravity:agy-main".into()));
        let rows = m.rows();
        let labels: Vec<_> = rows.iter().map(|r| r.label.as_str()).collect();
        // No Limits, auth-mode, API-key or live-steering items for Antigravity.
        for hidden in [
            "Limits",
            "Live steering",
            "Account auth",
            "API key",
            "Auth mode",
        ] {
            assert!(!labels.contains(&hidden), "{hidden} in {labels:?}");
        }
        // Logout is unsupported: never an enabled row.
        assert!(rows.iter().all(|r| r.label != "Log out" || !r.enabled));
        let acct = m
            .accounts
            .iter()
            .find(|a| a.id == "antigravity:agy-main")
            .unwrap();
        let lines = usage_lines(acct, 0);
        assert_eq!(fact(&lines, "Provider").as_deref(), Some("Antigravity CLI"));
        assert_eq!(fact(&lines, "Status").as_deref(), Some("Usage unsupported"));
        assert!(fact(&lines, "Auth").is_none() && fact(&lines, "API key").is_none());
        assert!(fact(&lines, "5h window").is_none() && fact(&lines, "Weekly").is_none());
        assert!(!lines.join("\n").contains('%'));
    }
    #[test]
    fn antigravity_login_plan_uses_the_pane_flow_without_an_auth_step() {
        let mut m = quad_providers();
        m.add_for("antigravity");
        let sent = m.submit(0, vec!["new".into()]);
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        let out = m.response(token, Ok(json!({"login":{"action":"exec","argv":["sessionsation","account","login","antigravity:new"],"credentials_in_client":false},"auth":{"logged_in":false}})));
        assert!(
            matches!(&out[..], [Intent::LaunchLogin(argv)] if argv == &strings(&["sessionsation","account","login","antigravity:new"]))
        );
        assert_eq!(
            m.dialog.as_ref().unwrap().body[0],
            "Complete the Antigravity CLI login in the opened pane…"
        );
        assert!(!matches!(m.form, Form::GeminiAuth(..) | Form::GeminiKey(_)));
    }
    const GEMINI_REASON: &str =
        "retired by Google for personal accounts — use Antigravity, API key or Workspace";
    fn gemini_auth_modes() -> Value {
        json!([
            {"id":"google_personal","mode":"google","label":"Google login","available":false,"reason":GEMINI_REASON},
            {"id":"google_workspace","mode":"google","label":"Google Workspace login","available":true,"reason":null},
            {"id":"api_key","mode":"api_key","label":"API key","available":true,"reason":null,"input":"stdin_json","field":"api_key"}
        ])
    }
    fn gemini_auth_manager() -> Manager {
        let mut m = triple_providers();
        let providers = m.capabilities["providers"].as_array_mut().unwrap();
        providers.last_mut().unwrap()["auth_modes"] = gemini_auth_modes();
        m
    }
    fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn paint_dialog(m: &mut Manager) -> Canvas {
        let mut c = Canvas::new(110, 30);
        m.paint(&mut c, 29);
        c
    }
    /// Screen row and first cell of the dialog row that is exactly `label`.
    fn row_of(c: &Canvas, label: &str) -> (usize, usize) {
        (0..c.height)
            .find_map(|y| {
                let cells: Vec<&str> = (0..c.width)
                    .map(|x| c.cells[y * c.width + x].glyph.as_str())
                    .collect();
                let label: Vec<String> = label.chars().map(String::from).collect();
                (0..cells.len().saturating_sub(label.len())).find_map(|x| {
                    let hit = cells[x..x + label.len()]
                        .iter()
                        .zip(&label)
                        .all(|(a, b)| a == b)
                        && cells[x + label.len()] == " "
                        && x >= 2
                        && cells[x - 1] == " ";
                    hit.then_some((y, x))
                })
            })
            .unwrap_or_else(|| panic!("no row {label}:\n{}", screen_text(c)))
    }
    #[test]
    fn gemini_auth_choice_lists_backend_modes_in_order_and_strikes_the_unavailable_one() {
        let mut m = gemini_auth_manager();
        m.show_login("gemini:gem-main");
        let d = m.dialog.as_ref().unwrap();
        let labels: Vec<_> = d.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Google login",
                "Google Workspace login",
                "API key",
                "",
                "Cancel"
            ]
        );
        // Availability comes from the backend data, not from the UI.
        let enabled: Vec<_> = d.buttons.iter().map(|b| b.enabled).collect();
        assert_eq!(enabled[..3], [false, true, true]);
        // The cursor starts on the first selectable option.
        assert_eq!(d.selected, 1);
        let c = paint_dialog(&mut m);
        let (y, x) = row_of(&c, "Google login");
        assert!(
            (x..x + "Google login".len())
                .filter(|col| c.cells[y * c.width + col].glyph != " ")
                .all(|col| c.cells[y * c.width + col].strike),
            "unavailable option is crossed out"
        );
        let (y, x) = row_of(&c, "Google Workspace login");
        assert!(!c.cells[y * c.width + x + 3].strike);
        // A backend that marks every option available strikes nothing.
        let mut open = gemini_auth_manager();
        open.capabilities["providers"][2]["auth_modes"][0]["available"] = json!(true);
        open.show_login("gemini:gem-main");
        assert!(open.dialog.as_ref().unwrap().buttons[0].enabled);
        assert_eq!(open.dialog.as_ref().unwrap().selected, 0);
    }
    #[test]
    fn gemini_unavailable_choice_cannot_be_chosen_and_shows_its_reason_only_when_focused() {
        let mut m = gemini_auth_manager();
        m.show_login("gemini:gem-main");
        let parenthetical = format!("({GEMINI_REASON})");
        // Cursor on an available option: no reason anywhere.
        let c = paint_dialog(&mut m);
        assert!(!screen_text(&c).contains("retired by Google"));
        // Move the cursor onto the struck-through option.
        m.key(key_event(KeyCode::Up));
        assert_eq!(m.dialog.as_ref().unwrap().selected, 0);
        let c = paint_dialog(&mut m);
        let text = screen_text(&c);
        assert!(text.contains("(retired by Google"), "{text}");
        let _ = parenthetical;
        let at = (0..c.cells.len() - 1)
            .find(|&i| c.cells[i].glyph == "(" && c.cells[i + 1].glyph == "r")
            .unwrap();
        assert_eq!(c.cells[at].fg, crate::widgets::MUTED, "darker grey");
        assert!(!c.cells[at].strike, "the reason itself is not struck");
        // Enter on it does nothing: no dialog change, no request.
        assert!(m.key(key_event(KeyCode::Enter)).is_empty());
        assert!(matches!(m.form, Form::GeminiAuth(..)));
        // Leaving it hides the reason again.
        m.key(key_event(KeyCode::Down));
        assert!(!screen_text(&paint_dialog(&mut m)).contains("retired by Google"));
    }
    #[test]
    fn gemini_workspace_choice_requests_google_mode_then_runs_the_login_plan() {
        let mut m = gemini_auth_manager();
        m.show_login("gemini:gem-main");
        let sent = m.submit(1, vec![]);
        let [Intent::Api(token, args)] = &sent[..] else {
            panic!("{sent:?}")
        };
        assert_eq!(
            args,
            &strings(&["account", "auth", "gemini:gem-main", "--mode", "google"])
        );
        let out = m.response(*token, Ok(json!({"account_id":"gemini:gem-main","provider":"gemini","auth_mode":"google","api_key_present":false,"auth_required":true,"auth_failed":false,"auth_failure_reason":null,"changed":true,"login_required":true,"login":{"action":"exec","argv":["sessionsation","account","login","gemini:gem-main"],"credentials_in_client":false}})));
        assert!(
            matches!(&out[..], [Intent::LaunchLogin(argv)] if argv == &strings(&["sessionsation","account","login","gemini:gem-main"]))
        );
        assert!(matches!(m.form, Form::LoginWait(_)));
        assert_eq!(
            m.dialog.as_ref().unwrap().body[0],
            "Complete the Gemini CLI login in the opened pane…"
        );
    }
    #[test]
    fn gemini_add_flow_asks_for_the_auth_mode_before_any_login_plan() {
        let mut m = gemini_auth_manager();
        m.add_for("gemini");
        let sent = m.submit(0, vec!["new".into()]);
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        let out = m.response(token, Ok(json!({"login":{"action":"exec","argv":["sessionsation","account","login","gemini:new"],"credentials_in_client":false},"auth":{"logged_in":false}})));
        assert!(
            out.iter().all(|i| !matches!(i, Intent::LaunchLogin(_))),
            "the plan waits for the choice"
        );
        assert!(matches!(&m.form, Form::GeminiAuth(key, _) if key == "gemini:new"));
        assert_eq!(m.dialog.as_ref().unwrap().buttons[2].label, "API key");
    }
    #[test]
    fn gemini_without_auth_modes_keeps_the_plain_login_flow() {
        let mut m = triple_providers();
        m.show_login("gemini:gem-main");
        assert!(matches!(m.form, Form::Login(_)));
        let sent = m.submit(0, vec![]);
        assert!(
            matches!(&sent[..], [Intent::Api(_, args)] if args == &strings(&["account", "login", "gemini:gem-main"]))
        );
        let mut m = triple_providers();
        m.add_for("gemini");
        let sent = m.submit(0, vec!["new".into()]);
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        let out = m.response(token, Ok(json!({"login":{"action":"exec","argv":["sessionsation","account","login","gemini:new"],"credentials_in_client":false},"auth":{"logged_in":false}})));
        assert!(matches!(&out[..], [Intent::LaunchLogin(_)]));
    }
    #[test]
    fn gemini_api_key_goes_only_through_stdin_and_never_renders_or_logs() {
        const KEY: &str = "AIzaSECRETkey-0123456789";
        let mut m = gemini_auth_manager();
        m.show_login("gemini:gem-main");
        assert!(m.submit(2, vec![]).is_empty());
        assert!(matches!(&m.form, Form::GeminiKey(key) if key == "gemini:gem-main"));
        for ch in KEY.chars() {
            m.key(key_event(KeyCode::Char(ch)));
        }
        // The masked field draws a mask per character, never a clear character.
        let screen = screen_text(&paint_dialog(&mut m));
        assert!(screen.contains(&"•".repeat(KEY.len())), "{screen}");
        for part in ["AIza", "SECRET", "0123", "key-"] {
            assert!(!screen.contains(part), "key text on screen: {part}");
        }
        // Debug output of the dialog never holds the key either.
        assert!(!format!("{:?}", m.dialog).contains("SECRET"));
        // Enter on Save key submits.
        let sent = m.key(key_event(KeyCode::Enter));
        let [Intent::ApiInput(token, args, secret)] = &sent[..] else {
            panic!("{sent:?}")
        };
        assert_eq!(
            args,
            &strings(&[
                "account",
                "auth",
                "gemini:gem-main",
                "--mode",
                "api-key",
                "--key-stdin"
            ])
        );
        assert!(args.iter().all(|a| !a.contains("SECRET")));
        assert_eq!(secret.0, json!({"api_key": KEY}));
        assert!(!format!("{:?}", sent[0]).contains("SECRET"));
        // Progress is shown, and no dialog or field state keeps the key.
        let d = m.dialog.as_ref().unwrap();
        assert!(d.busy && d.fields.is_empty() && d.body[0].contains("Saving and verifying"));
        assert!(!format!("{:?}", m.dialog).contains("SECRET"));
        let screen = screen_text(&paint_dialog(&mut m));
        assert!(!screen.contains("SECRET") && screen.contains("Saving and verifying"));
        // Valid: success note.
        let out = m.response(*token, Ok(json!({"account_id":"gemini:gem-main","provider":"gemini","auth_mode":"api_key","api_key_present":true,"auth_required":false,"auth_failed":false,"auth_failure_reason":null,"validation":{"state":"valid","code":null,"reason":null}})));
        assert!(out.iter().all(|i| matches!(i, Intent::Api(..))));
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.title, "Gemini API key");
        assert!(d.body[0].contains("saved and verified"));
    }
    #[test]
    fn gemini_api_key_validation_outcomes_and_errors_are_reported() {
        let respond = |validation: Value| {
            let mut m = gemini_auth_manager();
            m.show_gemini_key("gemini:gem-main");
            let sent = m.submit(0, vec!["k".into()]);
            let Intent::ApiInput(token, ..) = sent[0] else {
                panic!()
            };
            m.response(token, Ok(json!({"validation": validation})));
            m
        };
        let m = respond(
            json!({"state":"auth_failed","code":"API_KEY_INVALID","reason":"API key not valid. Please pass a valid API key."}),
        );
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.title, "Gemini API key");
        assert!(d.body[1].contains("API key not valid"));
        let m = respond(json!({"state":"inconclusive","code":"VALIDATION_TIMEOUT","reason":null}));
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.title, "Gemini API key");
        assert!(d.body[0].contains("saved but could not be verified (VALIDATION_TIMEOUT)"));
        // A rejected request uses the existing error presentation.
        let mut m = gemini_auth_manager();
        m.show_gemini_key("gemini:gem-main");
        let sent = m.submit(0, vec!["k".into()]);
        let Intent::ApiInput(token, ..) = sent[0] else {
            panic!()
        };
        m.response(token, Err("ACCOUNT_BUSY: try later".into()));
        assert!(m.dialog.is_none());
        assert_eq!(m.note, "ACCOUNT_BUSY: try later");
        // An empty key cannot be submitted.
        let mut m = gemini_auth_manager();
        m.show_gemini_key("gemini:gem-main");
        m.dialog.as_mut().unwrap().selected = 0;
        assert!(m.key(key_event(KeyCode::Enter)).is_empty());
        assert!(matches!(m.form, Form::GeminiKey(_)));
    }
    #[test]
    fn secret_text_input_masks_graphemes_cursor_and_debug() {
        let mut input = crate::widgets::TextInput::new(String::new()).secret();
        input.insert("abXé");
        let mut c = Canvas::new(30, 3);
        input.paint_focused(&mut c, 0, 0, 20, true, true);
        let text = screen_text(&c);
        assert!(text.contains("> ••••"), "{text}");
        assert!(!text.contains(['a', 'b', 'X', 'é']));
        assert!(!format!("{input:?}").contains("abX"));
        // Editing still works on the real value.
        input.edit(KeyCode::Backspace);
        assert_eq!(input.value, "abX");
    }
    #[test]
    fn gemini_details_show_the_auth_mode_and_key_presence() {
        let mut a = gemini("gem-main");
        let lines = usage_lines(&a, 0);
        assert!(fact(&lines, "Auth").is_none(), "older backend: no row");
        a.details.insert("auth_mode".into(), json!("api_key"));
        a.details.insert("api_key_present".into(), json!(true));
        let lines = usage_lines(&a, 0);
        assert_eq!(fact(&lines, "Auth").as_deref(), Some("API key"));
        assert_eq!(fact(&lines, "API key").as_deref(), Some("saved"));
        a.details.insert("auth_mode".into(), json!("google"));
        a.details.insert("api_key_present".into(), json!(false));
        let lines = usage_lines(&a, 0);
        assert_eq!(fact(&lines, "Auth").as_deref(), Some("Google"));
        assert!(fact(&lines, "API key").is_none());
    }
    #[test]
    fn gemini_sessions_use_claude_actions_native_label_and_status_gating() {
        let s = gemini_session(json!("idle"));
        let lines = Manager::session_detail_lines(&s);
        assert_eq!(fact(&lines, "Account").as_deref(), Some("✦ gem-main"));
        assert_eq!(fact(&lines, "Status").as_deref(), Some("running · idle"));
        assert!(lines.join("\n").contains("Native session  gem-native-1"));
        let labels: Vec<_> = crate::session_list::actions(&s, true)
            .into_iter()
            .map(|(r, _)| r.label)
            .collect();
        assert_eq!(
            labels[..3],
            ["Focus session", "Session details", "Rename session"]
        );
        // Detached running Gemini session: Add to HERDR is disabled with the reason.
        let mut d = gemini_session(json!("idle"));
        d["attachment"] = json!({"host":"tmux"});
        let rows = crate::session_list::actions(&d, true);
        assert_eq!(rows[0].0.label, "Add to HERDR");
        assert!(!rows[0].0.enabled);
        assert_eq!(rows[0].0.preview.as_deref(), Some("(no native attach)"));
        let mut st = gemini_session(json!(null));
        st["availability"] = json!("stopped");
        st["capabilities"]["resume"] = json!(true);
        let rows = crate::session_list::actions(&st, true);
        assert_eq!(rows[0].0.label, "Start session");
        assert!(rows[0].0.enabled);
    }
    #[test]
    fn gemini_busy_only_when_the_backend_reports_it() {
        let mut m = triple_providers();
        let sent = m.read_sessions();
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        let mk = |id: &str, extra: Value| {
            let mut v = json!({"id":id,"name":id,"provider":"gemini","account_id":"gemini:gem-main","availability":"running","last_accessed":1});
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            v
        };
        m.response(
            token,
            Ok(json!({"sessions":[
                mk("busy", json!({"native_status":"busy"})),
                mk("none", json!({})),
            ]})),
        );
        let act = |n: &str| m.sessions.iter().find(|s| id(s) == n).unwrap()["activity"].clone();
        assert_eq!(act("busy"), "busy");
        assert!(act("none").is_null());
        // No observe round trip for Gemini details.
        m.go(Page::Session("none".into()));
        assert!(
            m.refresh_live()
                .iter()
                .all(|i| !matches!(i, Intent::Api(_, a) if a.contains(&"observe".to_owned())))
        );
    }
    #[test]
    fn stale_resume_error_is_explained_without_marking_live() {
        assert_eq!(
            crate::messages::describe("SESSION_STALE: gone"),
            "This session's process is no longer verified. Refresh Manage sessions; nothing was started or resumed."
        );
        assert!(crate::messages::describe("SESSION_ALREADY_RUNNING").contains("now running"));
    }
    fn claude_add(m: &mut Manager, name: &str) -> (u64, Vec<Intent>) {
        m.add_for("claude");
        let sent = m.submit(0, vec![name.into()]);
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        let plan = json!({"login":{"action":"exec","argv":["sessionsation","account","login",format!("claude:{name}")],"credentials_in_client":false},"auth":{"logged_in":false}});
        let out = m.response(token, Ok(plan));
        (token, out)
    }
    fn polled(m: &mut Manager, auth_required: bool) -> Vec<Intent> {
        m.login_wait.as_mut().unwrap().last_poll = None;
        let sent = m.refresh_live();
        let [Intent::Api(token, args)] = &sent[..] else {
            panic!("expected one poll, got {sent:?}")
        };
        assert_eq!(args, &strings(&["account", "list"]));
        m.response(
            *token,
            Ok(json!([{"id":"claude:new","selectable":true,"provider":"claude","auth_required":auth_required}])),
        )
    }
    #[test]
    fn claude_login_plan_launches_a_pane_and_polls_until_signed_in() {
        let mut m = dual_providers();
        let (_, out) = claude_add(&mut m, "new");
        assert!(
            matches!(&out[..], [Intent::LaunchLogin(argv)] if argv == &strings(&["sessionsation","account","login","claude:new"]))
        );
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.title, "Claude login");
        assert_eq!(
            d.body[0],
            "Complete the Claude Code login in the opened pane…"
        );
        assert!(d.body.iter().all(|l| !l.contains("sessionsation account")));
        // Not yet signed in: keeps waiting, never more than one poll in flight.
        assert!(polled(&mut m, true).is_empty());
        assert!(matches!(m.form, Form::LoginWait(_)));
        let first = {
            m.login_wait.as_mut().unwrap().last_poll = None;
            m.refresh_live()
        };
        assert_eq!(first.len(), 1);
        assert!(m.refresh_live().is_empty());
        let Intent::Api(token, _) = first[0] else {
            panic!()
        };
        m.response(token, Ok(json!([{"id":"claude:new","selectable":true,"provider":"claude","auth_required":false}])));
        assert_eq!(m.dialog.as_ref().unwrap().title, "Claude login");
        assert!(
            m.dialog.as_ref().unwrap().body[0].ends_with("is signed in for claude-new."),
            "{:?}",
            m.dialog.as_ref().unwrap().body
        );
        assert!(m.login_wait.is_none());
        assert_eq!(m.page, Page::Account("claude:new".into()));
    }
    #[test]
    fn claude_login_wait_is_cancellable_bounded_and_falls_back_to_the_command() {
        let mut m = dual_providers();
        claude_add(&mut m, "new");
        m.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(m.login_wait.is_none() && m.dialog.is_none());
        assert!(
            m.refresh_live()
                .iter()
                .all(|i| !matches!(i, Intent::Api(_, a) if a[0] == "account"))
        );
        let mut m = dual_providers();
        claude_add(&mut m, "new");
        m.login_wait.as_mut().unwrap().started =
            std::time::Instant::now() - LOGIN_LIMIT - std::time::Duration::from_secs(1);
        assert!(m.refresh_live().is_empty());
        assert_eq!(m.dialog.as_ref().unwrap().title, "Claude login");
        assert!(m.dialog.as_ref().unwrap().body[0].starts_with("Login not completed: timed out"));
        let mut m = dual_providers();
        claude_add(&mut m, "new");
        m.login_launched(Err("HERDR could not open the login pane".into()));
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.title, "Claude login");
        assert_eq!(d.body[2], "sessionsation account login claude:new");
        assert!(m.login_wait.is_none());
    }
    fn pane_screen(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/login-screens/{name}.txt",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }
    #[test]
    fn a_login_in_progress_offers_its_wrapped_link_fullscreen() {
        for (width, provider, key) in [
            (80, "claude", "claude:claude-main"),
            (120, "claude", "claude:claude-main"),
            (80, "gemini", "gemini:gem-main"),
            (120, "gemini", "gemini:gem-main"),
        ] {
            let mut m = triple_providers();
            let out = m.begin_login(key);
            assert!(matches!(&out[..], [Intent::ReadLoginPane(k)] if k == key));
            assert!(m.dialog.as_ref().unwrap().links().is_empty());
            // No login pane at all: the ordinary Login confirmation, never a stuck wait.
            let mut none = triple_providers();
            none.begin_login(key);
            none.login_screen(key.into(), Ok(None));
            assert!(matches!(none.form, Form::Login(_) | Form::GeminiAuth(..)));
            // Pane open, link not printed yet: waiting, no option.
            m.login_screen(key.into(), Ok(Some(pane_screen("waiting-80"))));
            let d = m.dialog.as_ref().unwrap();
            assert!(d.links().is_empty() && !d.has_link_option());
            assert_eq!(d.body[0], "A login is already in progress.");
            assert!(d.body[1].starts_with("Waiting for"));
            // The screen changes: the rebuilt link appears with the option.
            m.login_screen(
                key.into(),
                Ok(Some(pane_screen(&format!("{provider}-{width}")))),
            );
            let expected = std::fs::read_to_string(format!(
                "{}/tests/fixtures/login-screens/{provider}.url",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            let d = m.dialog.as_mut().unwrap();
            assert_eq!(d.links(), vec![expected.trim_end().to_owned()]);
            // Painting adds the View link fullscreen option.
            let mut canvas = crate::render::Canvas::new(100, 30);
            d.paint(&mut canvas, &Palette::default());
            assert!(d.has_link_option());
            // A replaced link is followed.
            m.login_screen(key.into(), Ok(Some(pane_screen("replaced-80"))));
            assert_eq!(m.dialog.as_ref().unwrap().links().len(), 1);
            let newest = pane_screen("replaced-80");
            assert_eq!(
                m.dialog.as_ref().unwrap().links()[0],
                crate::login_link::extract(&newest).unwrap()
            );
        }
    }
    #[test]
    fn the_login_pane_is_reread_on_a_timer_and_stops_with_the_dialog() {
        let mut m = triple_providers();
        m.begin_login("claude:claude-main");
        m.login_screen(
            "claude:claude-main".into(),
            Ok(Some(pane_screen("waiting-80"))),
        );
        assert!(m.login_link_poll().is_empty(), "not before the interval");
        m.login_link.as_mut().unwrap().last =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(3));
        assert!(matches!(
            &m.login_link_poll()[..],
            [Intent::ReadLoginPane(_)]
        ));
        assert!(m.login_link_poll().is_empty(), "one read at a time");
        m.dismiss();
        assert!(m.login_link.is_none() && m.login_link_poll().is_empty());
    }
    #[test]
    fn opened_login_pane_closes_the_popup_so_the_link_is_visible() {
        let mut m = dual_providers();
        claude_add(&mut m, "new");
        let out = m.login_launched(Ok(()));
        assert!(matches!(&out[..], [Intent::Exit]));
        assert!(m.login_wait.is_none() && m.dialog.is_none());
        assert!(
            m.login_launched(Ok(())).is_empty(),
            "no wait, nothing to close"
        );
    }
    #[test]
    fn cancelling_the_wait_says_it_stopped_waiting() {
        let mut m = dual_providers();
        claude_add(&mut m, "new");
        m.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(m.note.starts_with("Stopped waiting for sign-in"));
    }
    #[test]
    fn codex_login_dialog_ends_right_after_its_last_option() {
        let mut m = dual_providers();
        m.go(Page::Account("codex:account1".into()));
        let sent = m.mutate(strings(&["account", "login", "codex:account1"]));
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        m.dialog
            .as_mut()
            .unwrap()
            .paint(&mut Canvas::new(100, 40), &Palette::default());
        assert!(m.dialog.as_ref().unwrap().size.unwrap().height >= 16);
        let out = m.response(token, Ok(json!({"login":{"verificationUri":"https://example.invalid/device","userCode":"ABCD-EFGH"}})));
        assert!(out.is_empty());
        let d = m.dialog.as_mut().unwrap();
        assert_eq!(d.buttons[0].label, dialog::VIEW_LINK);
        assert_eq!(d.buttons[1].label, "Continue in background");
        let mut c = Canvas::new(100, 40);
        d.paint(&mut c, &Palette::default());
        let rows: Vec<String> = c
            .cells
            .chunks(c.width)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect())
            .collect();
        let option = rows
            .iter()
            .position(|r| r.contains("Continue in background"))
            .unwrap();
        let bottom = rows.iter().rposition(|r| r.contains('└')).unwrap();
        // Last option, one blank row, the dialog's own hint row, then the border.
        assert_eq!(bottom, option + 3, "the frame ends right after its hints");
        assert!(
            rows[option + 2].contains("Esc: Close"),
            "{}",
            rows[option + 2]
        );
    }
    #[test]
    fn codex_login_link_opens_fullscreen_and_every_back_key_returns_to_the_dialog() {
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let mut m = dual_providers();
        let sent = m.mutate(strings(&["account", "login", "codex:account1"]));
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        let link = "https://example.invalid/device?user_code=ABCD-EFGH&x=%2F";
        m.response(
            token,
            Ok(json!({"login":{"verificationUri":link,"userCode":"ABCD-EFGH"}})),
        );
        let d = m.dialog.as_mut().unwrap();
        assert!(
            d.has_link_option(),
            "the login link offers the fullscreen view"
        );
        assert_eq!(d.buttons[d.selected].label, "Continue in background");
        m.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        for code in [KeyCode::Esc, KeyCode::Enter] {
            assert!(
                m.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                    .is_empty()
            );
            let d = m.dialog.as_mut().unwrap();
            assert!(d.link_open());
            // Left never leaves the link view.
            m.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
            assert!(m.dialog.as_ref().unwrap().link_open());
            let mut c = Canvas::new(80, 24);
            m.paint(&mut c, 22);
            let text: String = c.cells[2 * 80..3 * 80]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect();
            assert_eq!(text.trim_end(), link);
            // An armed close never survives into, or is armed by, the link view.
            m.close_armed = Some(std::time::Instant::now());
            assert!(m.key(KeyEvent::new(code, KeyModifiers::NONE)).is_empty());
            assert!(m.close_armed.is_none());
            let d = m.dialog.as_ref().expect("back on the login dialog");
            assert!(!d.link_open() && matches!(m.form, Form::None));
            assert_eq!(d.title, "Codex login");
            assert_eq!(d.buttons[d.selected].label, dialog::VIEW_LINK);
        }
        assert!(m.key(esc).is_empty());
        assert!(m.dialog.is_none());
    }
    #[test]
    fn deregister_restore_and_delete_work_for_claude_when_supported() {
        let mut m = dual_providers();
        m.capabilities["providers"][1]["account_removal"] = json!(true);
        m.capabilities["providers"][1]["account_deletion"] = json!(true);
        let key = "claude:claude-main";
        m.go(Page::Account(key.into()));
        let label = |m: &Manager, l: &str| m.rows().iter().position(|r| r.label == l);
        let dereg = label(&m, "Deregister account").unwrap();
        assert!(m.rows()[dereg].enabled && m.rows()[label(&m, "Delete account").unwrap()].enabled);
        m.options.selected = dereg;
        let sent = m.activate();
        let Intent::Api(token, args) = &sent[0] else {
            panic!()
        };
        assert_eq!(args, &strings(&["account", "remove", key]));
        m.response(*token, Ok(json!({"can_remove":true,"revision":"r","stored_data":{"files":1,"bytes":2,"history_files":3},"affected_sessions":[]})));
        assert_eq!(m.dialog.as_ref().unwrap().title, "Deregister account");
        m.dialog = None;
        m.form = Form::None;
        let del = label(&m, "Delete account").unwrap();
        m.options.selected = del;
        let sent = m.activate();
        let Intent::Api(token, args) = &sent[0] else {
            panic!()
        };
        assert_eq!(args, &strings(&["account", "delete", key]));
        m.response(
            *token,
            Ok(json!({"can_delete":true,"affected_sessions":[]})),
        );
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.title, "Delete account");
        assert!(
            d.body
                .iter()
                .any(|l| l.contains("managed account home is removed")
                    && l.contains("adopted external directory is left in place"))
        );
        // Backend-published semantics replace the hard-coded wording.
        m.dialog = None;
        m.form = Form::None;
        m.options.selected = del;
        let sent = m.activate();
        let Intent::Api(token, _) = &sent[0] else {
            panic!()
        };
        m.response(*token, Ok(json!({"can_delete":true,"affected_sessions":[],"semantics":["Home removed; history kept."]})));
        let d = m.dialog.as_ref().unwrap();
        assert!(d.body.iter().any(|l| l == "Home removed; history kept."));
        assert!(!d.body.iter().any(|l| l.contains("adopted external")));
        // Restore uses the same removal capability.
        m.accounts
            .iter_mut()
            .find(|a| a.id == key)
            .unwrap()
            .details
            .insert("membership".into(), json!("removed"));
        m.dialog = None;
        m.form = Form::None;
        m.go(Page::Account(key.into()));
        let restore = label(&m, "Restore account").unwrap();
        m.options.selected = restore;
        let sent = m.activate();
        assert!(
            matches!(&sent[..], [Intent::Api(_, a)] if a == &strings(&["account", "restore", key]))
                || m.dialog.is_some()
        );
    }
    #[test]
    fn backend_reasons_and_deletion_effects_use_the_published_backend_fields() {
        let mut m = dual_providers();
        m.capabilities["providers"][1]["operations"] = json!({"account_removal": {"supported": false, "reason": "membership removal is unavailable"}});
        assert_eq!(
            m.provider_reason("claude", "account_removal").as_deref(),
            Some("membership removal is unavailable")
        );
        let managed = json!({"can_delete": true, "home_disposition": "deleted", "preserves_history": true, "adopted_config_home": null});
        assert_eq!(
            backend_text(&managed),
            [
                "The managed account home is removed.",
                "Conversation history is preserved."
            ]
        );
        let adopted = json!({"can_delete": true, "home_disposition": "left_in_place", "preserves_history": true, "adopted_config_home": "/external/claude"});
        assert_eq!(
            backend_text(&adopted),
            [
                "Conversation history is preserved.",
                "The adopted config directory is left in place."
            ]
        );
    }
    #[test]
    fn unsupported_claude_removal_and_deletion_show_the_backend_reason() {
        let mut m = dual_providers();
        m.capabilities["providers"][1]["account_removal_reason"] = json!("not yet available");
        m.go(Page::Account("claude:claude-main".into()));
        let rows = m.rows();
        let dereg = rows
            .iter()
            .find(|r| r.label == "Deregister account")
            .unwrap();
        assert!(!dereg.enabled);
        assert!(!m.delete_account_row("claude:claude-main").enabled);
        assert_eq!(
            m.provider_reason("claude", "account_removal").as_deref(),
            Some("not yet available")
        );
    }
    #[test]
    fn account_menu_starts_with_details_limits_rename_and_rename_is_capability_gated() {
        let mut m = dual_providers();
        let key = "codex:account1";
        m.go(Page::Account(key.into()));
        let labels: Vec<_> = m.rows().iter().map(|r| r.label.clone()).collect();
        assert_eq!(&labels[..2], ["Details", "Rename"]);
        assert_eq!(labels[3], "Limits");
        assert!(!m.rows()[1].enabled, "no account_rename capability");
        m.capabilities["providers"][0]["account_rename"] = json!(true);
        assert!(m.rows()[1].enabled);
        m.options.selected = 1;
        m.activate();
        let d = m.dialog.as_mut().unwrap();
        assert_eq!(d.title, "Rename account");
        d.fields[0].input.insert("Work");
        let sent = m.submit(0, vec!["  Work ".into()]);
        assert!(
            matches!(&sent[..], [Intent::Api(_, a)] if a == &strings(&["account","rename",key,"--display-name","Work"]))
        );
        m.dialog = None;
        m.form = Form::None;
        m.options.selected = 1;
        m.activate();
        let sent = m.submit(0, vec!["".into()]);
        assert!(
            matches!(&sent[..], [Intent::Api(_, a)] if a == &strings(&["account","rename",key,"--display-name",""]))
        );
        // Remaining actions keep their behaviour after the shifted rows.
        m.dialog = None;
        m.form = Form::None;
        let disable = m.rows().iter().position(|r| r.label == "Disable").unwrap();
        m.options.selected = disable;
        m.activate();
        assert_eq!(m.dialog.as_ref().unwrap().title, "Disable account");
    }
    fn backend_rename_manager(mismatch: bool, running: u64) -> Manager {
        let mut m = dual_providers();
        m.capabilities["providers"][0]["account_rename"] = json!(true);
        m.capabilities["account_names"] =
            json!({"rename_backend": {"command": "account rename-backend"}});
        let blocked = running > 0;
        m.accounts[0]
            .details
            .insert("generation".into(), json!("g7"));
        m.accounts[0]
            .details
            .insert("name_mismatch".into(), json!(mismatch));
        m.accounts[0]
            .details
            .insert("running_sessions".into(), json!(running));
        m.accounts[0].details.insert(
            "backend_rename".into(),
            json!({"available": !blocked, "reason": if blocked { json!("ACCOUNT_SESSIONS_RUNNING") } else { Value::Null }}),
        );
        m.go(Page::Account("codex:account1".into()));
        m
    }
    fn popup_buttons(m: &Manager) -> Vec<(String, bool)> {
        m.dialog
            .as_ref()
            .unwrap()
            .buttons
            .iter()
            .map(|b| (b.label.clone(), b.enabled))
            .collect()
    }
    #[test]
    fn backend_rename_popup_follows_text_mismatch_and_running_sessions() {
        let mut m = backend_rename_manager(true, 0);
        assert_eq!(m.rows()[1].label, "Rename ⚠");
        assert!(m.rows()[1].warn);
        assert_eq!(m.rows()[1].preview.as_deref(), Some("(name ≠ folder)"));
        m.options.selected = 1;
        m.activate();
        // Unchanged text with a mismatch: Update backend is the only enabled rename.
        assert_eq!(
            popup_buttons(&m),
            [
                ("Update backend ⚠".to_owned(), true),
                ("Rename only".to_owned(), false),
                ("Cancel".to_owned(), true)
            ]
        );
        let current = m.dialog.as_ref().unwrap().fields[0].input.value.clone();
        let sent = m.submit(0, vec![current]);
        assert_eq!(
            api(&sent).1,
            strings(&[
                "account",
                "rename-backend",
                "codex:account1",
                "--expected-generation",
                "g7"
            ])
        );
        // Changed text: both rename options, no warning.
        m.dialog = None;
        m.form = Form::None;
        m.options.selected = 1;
        m.activate();
        m.dialog.as_mut().unwrap().paste("x");
        m.sync_rename_dialog();
        assert_eq!(
            popup_buttons(&m)[..2],
            [
                ("Rename & update backend".to_owned(), true),
                ("Rename only".to_owned(), true)
            ]
        );
        let sent = m.submit(0, vec!["New".into()]);
        assert_eq!(
            api(&sent).1,
            strings(&[
                "account",
                "rename-backend",
                "codex:account1",
                "New",
                "--expected-generation",
                "g7"
            ])
        );
        m.dialog = None;
        m.form = Form::None;
        m.options.selected = 1;
        m.activate();
        m.dialog.as_mut().unwrap().paste("x");
        m.sync_rename_dialog();
        let sent = m.submit(1, vec!["New".into()]);
        assert_eq!(
            api(&sent).1,
            strings(&[
                "account",
                "rename",
                "codex:account1",
                "--display-name",
                "New",
                "--expected-generation",
                "g7"
            ])
        );
    }
    #[test]
    fn backend_rename_is_disabled_with_a_reason_while_sessions_run_and_empty_text_disables_both() {
        let mut m = backend_rename_manager(false, 2);
        assert_eq!(m.rows()[1].label, "Rename", "no mismatch, no warning");
        m.options.selected = 1;
        m.activate();
        assert_eq!(
            popup_buttons(&m)[..2],
            [
                ("Rename & update backend".to_owned(), false),
                ("Rename only".to_owned(), false)
            ]
        );
        m.dialog.as_mut().unwrap().fields[0].input = crate::widgets::TextInput::new(String::new());
        m.sync_rename_dialog();
        assert!(!popup_buttons(&m)[1].1);
        m.dialog.as_mut().unwrap().paste("fresh");
        m.sync_rename_dialog();
        let b = &m.dialog.as_ref().unwrap().buttons;
        assert!(!b[0].enabled && b[0].preview.as_deref() == Some("(sessions running)"));
        assert!(b[1].enabled, "rename only stays available");
        // A mismatch with running sessions greys Update backend with its reason.
        let mut m = backend_rename_manager(true, 1);
        m.options.selected = 1;
        m.activate();
        let b = &m.dialog.as_ref().unwrap().buttons;
        assert_eq!(
            (b[0].label.as_str(), b[0].enabled),
            ("Update backend ⚠", false)
        );
        assert_eq!(b[0].preview.as_deref(), Some("(sessions running)"));
    }
    #[test]
    fn a_1_29_backend_keeps_the_old_rename_even_with_unknown_fields() {
        let mut m = dual_providers();
        m.capabilities["providers"][0]["account_rename"] = json!(true);
        m.accounts[0]
            .details
            .insert("name_mismatch".into(), json!(true));
        m.accounts[0]
            .details
            .insert("future_field".into(), json!({"x": 1}));
        m.go(Page::Account("codex:account1".into()));
        assert_eq!(m.rows()[1].label, "Rename");
        m.options.selected = 1;
        m.activate();
        assert!(matches!(m.form, Form::AccountRename(_)));
    }
    #[test]
    fn a_backend_rename_follows_the_new_account_id() {
        let mut m = backend_rename_manager(true, 0);
        m.options.selected = 1;
        m.activate();
        let (token, _) = api(&m.submit(0, vec!["fresh".into()]));
        m.dialog.as_mut().unwrap();
        let out = m.response(token, Ok(json!({"account_id":"codex:fresh","previous_account_id":"codex:account1","generation":"g8"})));
        assert_eq!(m.page, Page::Account("codex:fresh".into()));
        assert!(!out.is_empty(), "{out:?}");
    }
    #[test]
    fn same_name_under_two_providers_stays_distinct_by_id() {
        let mut m = dual_providers();
        m.accounts.push(
            serde_json::from_value(json!({"id":"claude:account1","selectable":true})).unwrap(),
        );
        assert_eq!(m.account("claude:account1").unwrap().provider(), "claude");
        assert_eq!(m.account("codex:account1").unwrap().provider(), "codex");
        assert_eq!(m.provider_of("claude:account1"), "claude");
    }
    #[test]
    fn display_names_replace_short_names_everywhere_with_fallback() {
        let mut m = dual_providers();
        m.accounts[0]
            .details
            .insert("display_name".into(), json!("Team Work"));
        assert_eq!(account_title(&m.accounts[0]), "Team Work");
        assert_eq!(account_title(&m.accounts[2]), "account2");
        assert!(m.rows().iter().any(|r| r.label == "Team Work"));
        m.sessions = vec![
            json!({"id":"s","name":"S","account_id":"codex:account1","availability":"running"}),
        ];
        m.derive_session_fields();
        assert_eq!(m.sessions[0]["account_label"], "Team Work");
        m.accounts[0].details.remove("display_name");
        m.derive_session_fields();
        assert!(m.sessions[0].get("account_label").is_none());
        m.accounts[0]
            .details
            .insert("display_name".into(), json!("Team Work"));
        let lines = usage_lines(&m.accounts[0], 0);
        assert_eq!(fact(&lines, "Account").as_deref(), Some("⏣ Team Work"));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Display name") && l.ends_with("Team Work"))
        );
    }
    #[test]
    fn claude_native_status_drives_activity_ordering_and_busy_styling() {
        let mut m = dual_providers();
        let sent = m.read_sessions();
        let Intent::Api(token, _) = sent[0] else {
            panic!()
        };
        let mk = |id: &str, acct: &str, status: Value, last: i64| json!({"id":id,"name":id,"account_id":acct,"availability":"running","native_status":status,"last_accessed":last});
        m.response(token, Ok(json!({"sessions":[
            mk("a-idle-codex","codex:account1",Value::Null,5),
            mk("b-idle-claude","claude:claude-main",json!("idle"),9),
            mk("c-busy-claude","claude:claude-main",json!("busy"),1),
            {"id":"d-busy-codex","name":"d-busy-codex","account_id":"codex:account1","availability":"running","activity":"busy","last_accessed":2},
        ]})));
        let order: Vec<_> = m.sessions.iter().map(|s| id(s).to_owned()).collect();
        assert_eq!(
            order,
            [
                "d-busy-codex",
                "c-busy-claude",
                "b-idle-claude",
                "a-idle-codex"
            ]
        );
        assert_eq!(m.sessions[1]["activity"], "busy");
        assert_eq!(m.sessions[2]["activity"], "idle");
        // The shared renderer shows the busy spinner for both providers.
        let mut c = Canvas::new(80, 20);
        let mut options = Options::default();
        crate::session_list::paint(&mut c, &mut options, &m.sessions, true, 19, 0, 15);
        let rows: Vec<String> = c
            .cells
            .chunks(c.width)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect())
            .collect();
        let line = |name: &str| rows.iter().find(|r| r.contains(name)).cloned().unwrap();
        assert!(line("c-busy-claude").contains(crate::session_list::claude_spinner(0)));
        assert!(line("d-busy-codex").contains(crate::session_list::CODEX_FILL_PULSE_FRAMES[0]));
        assert!(!line("b-idle-claude").contains(crate::session_list::claude_spinner(0)));
    }

    fn tmux_claude(add: Value) -> Value {
        let mut s = claude_session("running");
        s["attachment"] = json!({"host":"tmux"});
        s["capabilities"]["add_to_herdr"] = add;
        s
    }
    fn placed(sessions: Vec<Value>) -> Manager {
        let mut m = Manager {
            page: Page::Session("c".into()),
            sessions,
            host: json!({"session":"named","socket":"/chosen.sock"}),
            focused_space: Some("w2".into()),
            ..Default::default()
        };
        m.capabilities = json!({"resume":{"version":1},"resume_placement":{"version":1}});
        m
    }
    fn api(intents: &[Intent]) -> (u64, Vec<String>) {
        let Intent::Api(token, args) = &intents[0] else {
            panic!("expected an API request, got {intents:?}")
        };
        (*token, args.clone())
    }
    fn refreshes(intents: &[Intent]) -> bool {
        intents
            .iter()
            .any(|i| matches!(i, Intent::Api(_, a) if a == &strings(&["state"])))
    }
    fn state_token(intents: &[Intent]) -> u64 {
        intents
            .iter()
            .find_map(|i| match i {
                Intent::Api(t, a) if a == &strings(&["state"]) => Some(*t),
                _ => None,
            })
            .expect("a state refresh")
    }
    fn space_choices() -> Value {
        json!({"session_id":"c","generation":"g","herdr_session":"named","current_space_id":null,"spaces":[{"id":"w1","name":"Origin","current":false},{"id":"w2","name":"Work","current":false}],"pane":null})
    }
    #[test]
    fn add_to_herdr_replaces_focus_and_shows_the_backend_reason_text() {
        let mut s = tmux_claude(json!(false));
        s["capability_reasons"] =
            json!({"add_to_herdr":"INTERACTIVE_ELSEWHERE: Open in another terminal"});
        let rows = crate::session_list::actions(&s, true);
        assert_eq!(rows[0].0.label, "Add to HERDR");
        assert!(!rows[0].0.enabled);
        assert_eq!(rows[0].1, crate::session_list::Action::AddToHerdr);
        let mut canvas = Canvas::new(80, 24);
        let mut m = placed(vec![s.clone()]);
        m.paint(&mut canvas, 23);
        let text: String = canvas.cells.iter().map(|c| c.glyph.as_str()).collect();
        assert_eq!(
            rows[0].0.preview.as_deref(),
            Some("(open in another terminal)")
        );
        assert!(
            text.contains("(open in another terminal)"),
            "hovered note shows"
        );
        assert!(!text.contains("INTERACTIVE_ELSEWHERE"));
        assert!(m.activate().is_empty(), "disabled row sends nothing");
        // Codex shares the row; a HERDR pane or an older backend keeps Focus Session.
        let mut codex = json!({"id":"x","availability":"running","attachment":{"host":"tmux"},"capabilities":{"add_to_herdr":true}});
        assert_eq!(
            crate::session_list::actions(&codex, true)[0].0.label,
            "Add to HERDR"
        );
        codex["attachment"] = json!({"host":"herdr"});
        assert_eq!(
            crate::session_list::actions(&codex, true)[0].0.label,
            "Focus session"
        );
        let mut old = tmux_claude(json!(true));
        old["capabilities"]
            .as_object_mut()
            .unwrap()
            .remove("add_to_herdr");
        assert_eq!(
            crate::session_list::actions(&old, true)[0].0.label,
            "Focus session"
        );
    }
    #[test]
    fn add_to_herdr_picks_a_space_names_it_and_offers_verified_focus() {
        let mut m = placed(vec![tmux_claude(json!(true))]);
        let (token, args) = api(&m.activate());
        assert_eq!(
            args,
            strings(&[
                "spaces",
                "--session",
                "c",
                "--expected-generation",
                "g",
                "--herdr-session",
                "named",
                "--herdr-socket",
                "/chosen.sock"
            ])
        );
        m.response(token, Ok(space_choices()));
        assert_eq!(m.page, Page::Spaces("c".into()));
        assert_eq!(m.options.selected, 2, "defaults to the focused Space");
        let rows = m.rows();
        assert_eq!(rows[1].label, "Origin");
        assert_eq!(rows[1].preview.as_deref(), Some("(w1)"));
        assert_eq!(rows[1].preview_gap, 1);
        assert_eq!(rows.last().unwrap().label, "Cancel");
        assert!(rows[rows.len() - 2].separator);
        assert_eq!(rows[0].label, "[create new space]");
        assert!(!rows[0].enabled, "disabled until the backend advertises it");
        assert_eq!(
            rows[0].preview.as_deref(),
            Some("(not available for adding yet)")
        );
        assert!(m.activate().is_empty());
        let dialog = m.dialog.as_ref().unwrap();
        assert_eq!(dialog.title, "Add to HERDR");
        assert_eq!(
            dialog.fields[0].input.value, "Work",
            "name defaults to the session name"
        );
        // Cancelling the name dialog mutates nothing.
        assert!(m.submit(1, vec![]).is_empty());
        m.activate();
        let (token, args) = api(&m.submit(0, vec!["Renamed".into()]));
        assert_eq!(
            args,
            strings(&[
                "session",
                "add-to-herdr",
                "--session",
                "c",
                "--expected-generation",
                "g",
                "--herdr-session",
                "named",
                "--herdr-socket",
                "/chosen.sock",
                "--space",
                "w2",
                "--name",
                "Renamed"
            ])
        );
        let sent = m.response(token, Ok(json!({"state":"attached","session_id":"c","generation":"g2","host":"herdr","herdr_session":"named","space_id":"w2","tab_id":"t","pane_id":"p","target":"herdr/named/w2:p","name":"Renamed","job_id":"j","attachment":{"host":"herdr"},"starts_agent":false,"sends_input":false})));
        assert!(refreshes(&sent), "attached refreshes the list");
        assert_eq!(m.page, Page::Sessions);
        let dialog = m.dialog.as_ref().unwrap();
        assert_eq!(dialog.title, "Added to HERDR");
        assert_eq!(dialog.buttons[0].label, "Focus tab");
        let focus = m.submit(0, vec![]);
        assert!(
            matches!(&focus[..], [Intent::Focus(s)] if s["id"] == "c" && s["generation"] == "g2")
        );
    }
    #[test]
    fn unconfirmed_attachment_is_reported_honestly_and_never_retried() {
        let mut m = placed(vec![tmux_claude(json!(true))]);
        let (token, _) = api(&m.activate());
        m.response(token, Ok(space_choices()));
        m.activate();
        let (token, _) = api(&m.submit(0, vec!["Work".into()]));
        let sent = m.response(token, Ok(json!({"state":"attach_unconfirmed","session_id":"c","generation":null,"attachment":null,"starts_agent":false,"sends_input":false})));
        assert!(refreshes(&sent));
        assert!(
            sent.iter()
                .all(|i| matches!(i, Intent::Api(_, a) if a[0] != "session" && a[0] != "resume")),
            "never retried"
        );
        let dialog = m.dialog.as_ref().unwrap();
        assert_eq!(dialog.title, "Attachment not confirmed");
        assert_eq!(dialog.buttons.len(), 1);
        assert!(dialog.body.join(" ").contains("Nothing was retried"));
        assert!(m.submit(0, vec![]).is_empty());
    }
    #[test]
    fn mismatched_space_lists_are_refused() {
        let mut m = placed(vec![tmux_claude(json!(true))]);
        let (token, _) = api(&m.activate());
        let mut other = space_choices();
        other["herdr_session"] = json!("someone-else");
        m.response(token, Ok(other));
        assert_eq!(m.page, Page::Session("c".into()));
        assert!(m.note.contains("Session changed"));
    }
    #[test]
    fn start_session_offers_space_and_name_only_when_placement_is_advertised() {
        let mut stopped = claude_session("stopped");
        stopped["capabilities"]["resume"] = json!(true);
        for advertised in [true, false] {
            let mut m = placed(vec![stopped.clone()]);
            if !advertised {
                m.capabilities = json!({"resume":{"version":1}});
            }
            let (token, args) = api(&m.activate());
            assert_eq!(args[0], "session");
            let mut fresh = stopped.clone();
            fresh["original_host"] =
                json!({"session":"named","socket":"/chosen.sock","workspace_id":"w1"});
            let sent = m.response(token, Ok(fresh));
            if !advertised {
                assert!(matches!(&sent[..], [Intent::Resume(s)] if s["id"] == "c"));
                continue;
            }
            let (token, args) = api(&sent);
            assert_eq!(args[0], "spaces");
            assert!(args.contains(&"named".to_string()));
            m.response(token, Ok(space_choices()));
            assert_eq!(
                m.options.selected, 1,
                "remembered Space beats the focused one"
            );
            m.activate();
            assert_eq!(m.dialog.as_ref().unwrap().title, "Start session");
            assert_eq!(m.dialog.as_ref().unwrap().fields[0].input.value, "Work");
            let (token, args) = api(&m.submit(0, vec!["Fresh".into()]));
            assert_eq!(
                args,
                strings(&[
                    "resume",
                    "--session",
                    "c",
                    "--expected-generation",
                    "g",
                    "--herdr-session",
                    "named",
                    "--space",
                    "w1",
                    "--name",
                    "Fresh"
                ])
            );
            let sent = m.response(token, Ok(json!({"state":"started"})));
            assert!(refreshes(&sent));
            assert_eq!(m.dialog.as_ref().unwrap().title, "Session start requested");
        }
    }
    #[test]
    fn create_new_space_names_then_confirms_for_start_and_move_when_advertised() {
        let mut stopped = claude_session("stopped");
        stopped["capabilities"]["resume"] = json!(true);
        let mut m = placed(vec![stopped.clone()]);
        m.capabilities["new_space"] = json!({"resume": true, "move_space": true});
        let (token, _) = api(&m.activate());
        let sent = m.response(token, Ok(stopped.clone()));
        let (token, _) = api(&sent);
        m.response(token, Ok(space_choices()));
        assert!(m.rows()[0].enabled);
        m.options.selected = 0;
        assert!(m.activate().is_empty());
        assert_eq!(m.dialog.as_ref().unwrap().title, "New Space name");
        // A label that exists already is refused, as in the new-session flow.
        assert!(m.submit(0, vec!["Work".into()]).is_empty());
        assert_eq!(m.dialog.as_ref().unwrap().title, "New Space name");
        assert!(m.submit(0, vec!["Fresh Space".into()]).is_empty());
        assert_eq!(m.dialog.as_ref().unwrap().title, "Start session");
        let (_, args) = api(&m.submit(0, vec!["Fresh".into()]));
        assert_eq!(
            args[args.len() - 4..],
            strings(&["--new-space", "Fresh Space", "--name", "Fresh"])
        );
        assert!(!args.contains(&"--space".to_string()));

        let running = json!({"id":"stable","name":"Example","generation":"g1","availability":"running","attachment":{"host":"herdr"},"capabilities":{"move_space":true}});
        let mut m = Manager {
            page: Page::Spaces("stable".into()),
            sessions: vec![running.clone()],
            space_session: running,
            space_choices: space_choices(),
            ..Default::default()
        };
        m.capabilities = json!({"new_space":{"move_space":true}});
        m.options.selected = 0;
        assert!(m.activate().is_empty());
        assert!(m.submit(0, vec!["Fresh Space".into()]).is_empty());
        assert_eq!(m.dialog.as_ref().unwrap().title, "Move to Space");
        let sent = m.submit(0, vec![]);
        let Intent::Api(_, args) = &sent[0] else {
            panic!()
        };
        assert_eq!(args[0], "move-space");
        assert_eq!(args[5..7], strings(&["--new-space", "Fresh Space"]));
        assert_eq!(args[7], "--request-id");
    }
    #[test]
    fn session_details_list_earlier_ids_forks_and_jobs_only_when_known() {
        let mut s = claude_session("running");
        s["previous_conversation_ids"] = json!(["old-1", "old-2"]);
        s["forked_from_conversation_id"] = json!("parent");
        s["native"] = json!({"kind":"bg","job_id":"job-7"});
        let lines = Manager::session_detail_lines(&s);
        assert_eq!(
            fact(&lines, "Status").as_deref(),
            Some("running · idle · background")
        );
        let ids = lines.iter().position(|l| l == "Identifiers").unwrap();
        let rest = lines[ids..].join("\n");
        for part in ["old-1 · old-2", "Forked from", "parent", "Job", "job-7"] {
            assert!(rest.contains(part), "{part}");
        }
        let plain = Manager::session_detail_lines(&claude_session("running")).join("\n");
        assert!(!plain.contains("Earlier") && !plain.contains("Forked") && !plain.contains("Job"));
        let mut codex =
            json!({"id":"x","previous_conversation_ids":["a"],"forked_from_conversation_id":"b"});
        let text = Manager::session_detail_lines(&codex).join("\n");
        assert!(text.contains("Earlier IDs") && text.contains("Forked from"));
        codex["previous_conversation_ids"] = json!([]);
        assert!(
            !Manager::session_detail_lines(&codex)
                .join("\n")
                .contains("Earlier")
        );
    }
    #[test]
    fn merged_sessions_refresh_and_select_the_survivor_from_any_call() {
        let survivor = "11111111-1111-4111-8111-111111111111";
        assert_eq!(
            merged_survivor(&format!(
                "SESSION_MERGED: {survivor} (renamed). Refresh the list."
            ))
            .as_deref(),
            Some(survivor)
        );
        assert_eq!(
            merged_survivor("SESSION_MERGED: abc").as_deref(),
            Some("abc")
        );
        assert_eq!(merged_survivor("STALE_TARGET: nope"), None);
        let sessions = vec![
            json!({"id":"dead","generation":"g","name":"Dead","availability":"stopped"}),
            json!({"id":survivor,"generation":"g","name":"Alive","availability":"stopped"}),
        ];
        let mut m = Manager {
            page: Page::Session("dead".into()),
            sessions: sessions.clone(),
            sessions_known: true,
            ..Default::default()
        };
        let sent = m.preview(
            "Resume session",
            Manager::target(&sessions[0], "session"),
            Purpose::Resume("dead".into()),
        );
        let (token, _) = api(&sent);
        let sent = m.response(token, Err(format!("SESSION_MERGED: {survivor}")));
        assert_eq!(m.page, Page::Sessions);
        assert!(m.dialog.is_none());
        let token = state_token(&sent);
        m.response(
            token,
            Ok(json!({"sessions":[sessions[1].clone()],"capabilities":{}})),
        );
        let row = m.options.selected;
        assert_eq!(
            crate::session_list::session_index(&m.sessions, row).map(|i| id(&m.sessions[i])),
            Some(survivor)
        );
        // Failures outside a tracked request (focus) follow the same path.
        let mut m = Manager {
            page: Page::Session("dead".into()),
            sessions,
            sessions_known: true,
            ..Default::default()
        };
        let sent = m.failure(format!("SESSION_MERGED: {survivor}"));
        assert_eq!(m.page, Page::Sessions);
        assert!(refreshes(&sent));
        assert!(m.failure("HERDR_ATTACHMENT_MISMATCH".into()).is_empty());
        assert_eq!(m.note, "HERDR_ATTACHMENT_MISMATCH");
    }
    /// The rows of the dialog painted alone on a `width`×`height` terminal.
    fn dialog_rows(m: &mut Manager, width: usize, height: usize) -> Vec<String> {
        let mut c = Canvas::new(width, height);
        m.dialog
            .as_mut()
            .unwrap()
            .paint(&mut c, &Palette::default());
        c.cells
            .chunks(width)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect())
            .collect()
    }
    /// Asserts the open dialog keeps everything inside its own frame: the title on
    /// the top border, every option between the borders, the whole body without
    /// scrolling, and its controls centered on the row above the bottom border.
    fn assert_inside_frame(m: &mut Manager, width: usize, height: usize, what: &str) {
        let rows = dialog_rows(m, width, height);
        let screen = rows.join("\n");
        let top = rows.iter().position(|r| r.contains('┌')).expect(what);
        let bottom = rows.iter().rposition(|r| r.contains('└')).expect(what);
        let chars = |y: usize| rows[y].chars().collect::<Vec<_>>();
        let left = chars(top).iter().position(|c| *c == '┌').unwrap();
        let right = chars(top).iter().position(|c| *c == '┐').unwrap();
        let d = m.dialog.as_ref().unwrap();
        assert!(rows[top].contains(&d.title), "{what}: title\n{screen}");
        // The body scrolls only when the frame already has the most rows it can.
        if d.wrapped > d.visible {
            assert_eq!(
                bottom - top + 1,
                height - 4,
                "{what}: body scrolls\n{screen}"
            );
            assert!(screen.contains("PgUp/PgDn: Scroll"), "{what}\n{screen}");
        } else {
            assert!(!screen.contains("PgUp"), "{what}\n{screen}");
        }
        for b in d
            .buttons
            .iter()
            .filter(|b| !b.separator && !b.label.is_empty())
        {
            let y = rows
                .iter()
                .position(|r| {
                    r.contains(&format!("○ {}", b.label)) || r.contains(&format!("● {}", b.label))
                })
                .unwrap_or_else(|| panic!("{what}: option {} missing\n{screen}", b.label));
            assert!(
                top < y && y + 2 < bottom,
                "{what}: {} outside the frame\n{screen}",
                b.label
            );
        }
        // Controls: centered on the last row inside the frame, nowhere else.
        let hint: String = chars(bottom - 1)[left + 2..right - 1].iter().collect();
        assert!(hint.contains("Esc"), "{what}: hint row {hint:?}\n{screen}");
        let lead = hint.chars().take_while(|c| *c == ' ').count();
        let trail = hint.chars().rev().take_while(|c| *c == ' ').count();
        assert!(
            lead.abs_diff(trail) <= 1,
            "{what}: hint not centered {hint:?}"
        );
        assert_eq!(screen.matches("Esc").count(), 1, "{what}\n{screen}");
    }
    fn login_manager() -> Manager {
        let mut m = quad_providers();
        let providers = m.capabilities["providers"].as_array_mut().unwrap();
        providers.iter_mut().find(|p| p["id"] == "gemini").unwrap()["auth_modes"] =
            gemini_auth_modes();
        m
    }
    const LOGIN_TITLES: [(&str, &str); 4] = [
        ("codex:account1", "Codex login"),
        ("claude:claude-main", "Claude login"),
        ("gemini:gem-main", "Gemini login"),
        ("antigravity:agy-main", "Antigravity login"),
    ];
    #[test]
    fn every_login_popup_is_titled_provider_login() {
        for (key, title) in LOGIN_TITLES {
            assert_eq!(
                crate::client::login_title(key.split(':').next().unwrap()),
                title
            );
            let mut m = login_manager();
            let out = m.begin_login(key);
            assert_eq!(
                m.dialog.as_ref().unwrap().title,
                title,
                "{key}: first popup"
            );
            if !out.is_empty() {
                dialog_rows(&mut m, 80, 24);
                m.login_screen(key.into(), Ok(None));
                assert_eq!(m.dialog.as_ref().unwrap().title, title, "{key}: confirm");
                let mut m = login_manager();
                m.begin_login(key);
                m.login_screen(key.into(), Ok(Some(pane_screen("waiting-80"))));
                assert_eq!(
                    m.dialog.as_ref().unwrap().title,
                    title,
                    "{key}: in progress"
                );
            }
            // The wait, the result and the failure popups of a launched login.
            let mut m = login_manager();
            m.mutation = strings(&["account", "login", key]);
            assert_eq!(m.mutation_login_title(), title);
            m.login_wait = Some(LoginWait {
                key: key.into(),
                argv: strings(&["sessionsation", "account", "login", key]),
                started: std::time::Instant::now(),
                last_poll: None,
            });
            m.form = Form::LoginWait(key.into());
            m.login_launched(Err("HERDR could not open the login pane".into()));
            assert_eq!(m.dialog.as_ref().unwrap().title, title, "{key}: failure");
        }
        let mut m = login_manager();
        m.mutation = strings(&["fixture-only"]);
        assert_eq!(m.mutation_login_title(), "Provider login");
    }
    /// The popups after the small "Checking for a login in progress…" frame kept
    /// that frame and drew their options below its border (Gemini, Claude,
    /// Antigravity). Every login popup now fits its own frame at 60, 80 and 120.
    #[test]
    fn login_popups_keep_their_content_inside_their_frames() {
        for (width, height) in [(60, 20), (80, 24), (120, 30)] {
            for (key, _) in LOGIN_TITLES {
                let what = format!("{key} {width}x{height}");
                let mut m = login_manager();
                if m.begin_login(key).is_empty() {
                    assert_inside_frame(&mut m, width, height, &format!("{what} confirm"));
                    continue;
                }
                assert_inside_frame(&mut m, width, height, &format!("{what} checking"));
                m.login_screen(key.into(), Ok(None));
                assert_inside_frame(&mut m, width, height, &format!("{what} confirm"));
                if key.starts_with("gemini:") {
                    // The three options, then the masked API-key entry.
                    assert!(matches!(m.form, Form::GeminiAuth(..)));
                    m.dialog.as_mut().unwrap().selected = 2;
                    m.key(key_event(KeyCode::Enter));
                    assert!(matches!(m.form, Form::GeminiKey(_)), "{what}");
                    m.paste("AIzaFixtureKey");
                    assert_inside_frame(&mut m, width, height, &format!("{what} key"));
                    let screen = dialog_rows(&mut m, width, height).join("\n");
                    assert!(!screen.contains("AIzaFixtureKey"), "the key stays masked");
                }
                // A login already running: waiting, then its link (one more option).
                let mut m = login_manager();
                m.begin_login(key);
                dialog_rows(&mut m, width, height);
                m.login_screen(key.into(), Ok(Some(pane_screen("waiting-80"))));
                assert_inside_frame(&mut m, width, height, &format!("{what} waiting"));
                m.login_screen(key.into(), Ok(Some(pane_screen("claude-80"))));
                dialog_rows(&mut m, width, height);
                assert!(m.dialog.as_ref().unwrap().has_link_option(), "{what}");
                assert_inside_frame(&mut m, width, height, &format!("{what} link"));
            }
        }
    }
    /// With a dialog open the main popup's footer keeps its rows but shows no
    /// controls (neither the dialog's nor the page's); the dialog shows its own.
    /// The Esc close band still belongs to the footer and draws there.
    #[test]
    fn a_dialog_owns_its_hints_and_the_footer_keeps_only_the_close_band() {
        for (width, height) in [(60, 20), (80, 24), (120, 30)] {
            let mut m = setup();
            let screen = |m: &mut Manager| {
                let mut c = crate::render::Field::new(width, height, 1967).paint(4060);
                m.paint_at(&mut c, height - 1, 100);
                c.cells
                    .chunks(width)
                    .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>())
                    .collect::<Vec<_>>()
            };
            let rows = screen(&mut m);
            assert!(
                rows[height - 1].contains("Esc: Close"),
                "the page's own footer"
            );
            m.show(
                "Deregister account",
                vec!["Credentials remain recoverable.".into()],
                &["Confirm", "Cancel"],
                Form::None,
            );
            let rows = screen(&mut m);
            let bottom = rows.iter().rposition(|r| r.contains('└')).unwrap();
            assert!(rows[bottom - 1].contains("Esc: Cancel"), "{rows:#?}");
            for row in &rows[bottom + 1..] {
                assert!(
                    !row.contains("Esc") && !row.contains("Enter"),
                    "{width}: {row}"
                );
            }
            // The close band (from an Esc on Home) still draws on the footer rows.
            m.close_band = Some(std::time::Instant::now() - std::time::Duration::from_millis(400));
            let rows = screen(&mut m);
            assert!(
                rows[height - 1].contains(crate::close_band::MESSAGE),
                "{rows:#?}"
            );
            // Esc belongs to the dialog first: it closes the dialog, not the popup.
            m.close_band = None;
            m.key(key_event(KeyCode::Esc));
            assert!(m.dialog.is_none());
            assert!(screen(&mut m)[height - 1].contains("Esc: Close"));
        }
    }
}
#[cfg(test)]
mod audit_tests {
    use super::*;
    fn session(id: &str, generation: &str, activity: &str, accessed: i64) -> Value {
        json!({"id":id,"name":id,"account_id":"codex:x","availability":"running","activity":activity,
            "generation":generation,"last_accessed":accessed,"capabilities":{"rename":true}})
    }
    fn api(intents: &[Intent]) -> Option<(u64, Vec<String>)> {
        intents.iter().find_map(|i| match i {
            Intent::Api(token, args) => Some((*token, args.clone())),
            _ => None,
        })
    }
    #[test]
    fn a_rename_adopts_the_new_generation_and_a_stale_observe_is_silent() {
        let mut m = Manager {
            sessions_known: true,
            sessions: vec![session("a", "g1", "idle", 1)],
            ..Manager::default()
        };
        m.page = Page::Session("a".into());
        let sent = m.mutate(strings(&[
            "rename",
            "--session",
            "a",
            "--expected-generation",
            "g1",
            "--name",
            "renamed",
        ]));
        let (token, _) = api(&sent).unwrap();
        m.response(
            token,
            Ok(json!({"id":"a","name":"renamed","generation":"g2"})),
        );
        assert_eq!(m.sessions[0]["generation"], "g2");
        assert_eq!(m.sessions[0]["name"], "renamed");
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.title, "Saved");
        assert!(d.size.is_none(), "the info dialog fits its own content");
        m.dialog = None;
        m.form = Form::None;
        m.pending.clear();
        // The next observe reads the new generation.
        m.observe_after = None;
        let (token, args) = m
            .refresh()
            .into_iter()
            .find_map(|i| match i {
                Intent::Api(t, a) if a[0] == "observe" => Some((t, a)),
                _ => None,
            })
            .expect("observe");
        assert!(
            args.windows(2)
                .any(|w| w == ["--expected-generation", "g2"])
        );
        // A background observe that still raced the change leaves no notice.
        let intents = m.response(token, Err("STALE_TARGET".into()));
        assert!(m.note.is_empty(), "{}", m.note);
        assert!(
            api(&intents).is_some_and(|(_, a)| a[0] == "state")
                || m.pending.values().any(|p| matches!(p, Purpose::State)),
            "the sessions are read again"
        );
    }
    #[test]
    fn a_finished_operation_rereads_sessions() {
        let mut m = Manager {
            sessions_known: true,
            sessions: vec![session("a", "g1", "idle", 1)],
            ..Manager::default()
        };
        m.page = Page::Sessions;
        m.operations =
            vec![json!({"id":"o","kind":"restart","phase":"dispatching","source_session_id":"a"})];
        m.sequence += 1;
        let token = m.sequence;
        m.pending.insert(token, Purpose::Operations);
        let intents = m.response(
            token,
            Ok(json!([{"id":"o","kind":"restart","phase":"complete","source_session_id":"a"}])),
        );
        assert!(api(&intents).is_some_and(|(_, a)| a[0] == "state"));
    }
    #[test]
    fn first_sessions_entry_over_a_cached_list_starts_on_the_first_row() {
        let mut m = Manager::default();
        m.show_cached(&json!([
            session("old", "g", "idle", 9),
            session("busy", "g", "idle", 1),
        ]));
        m.page = Page::Home;
        m.options.selected = 1;
        m.activate();
        assert_eq!(m.page, Page::Sessions);
        let first = m.rows().iter().position(|r| !r.separator).unwrap();
        assert_eq!(m.options.selected, first);
        // Live state reorders (a busy session ranks first): the cursor stays put.
        m.sequence += 1;
        let token = m.sequence;
        m.pending.insert(token, Purpose::State);
        m.response(
            token,
            Ok(json!({"sessions":[session("old","g","idle",9),session("busy","g","busy",1)]})),
        );
        assert_eq!(m.options.selected, first);
        // Once the user moves, the selection follows its session through updates.
        let old = m.sessions.iter().position(|s| s["id"] == "old").unwrap();
        let row = crate::session_list::session_row(&m.sessions, old);
        m.options.selected = row;
        m.sequence += 1;
        let token = m.sequence;
        m.pending.insert(token, Purpose::State);
        m.response(
            token,
            Ok(json!({"sessions":[session("old","g","busy",9),session("busy","g","idle",1)]})),
        );
        let chosen = crate::session_list::session_index(&m.sessions, m.options.selected).unwrap();
        assert_eq!(m.sessions[chosen]["id"], "old");
    }
    #[test]
    fn the_status_lane_does_not_repeat_the_open_dialog() {
        let mut m = Manager {
            page: Page::Home,
            dialog: Some(Dialog::new(
                "Operation progress",
                vec!["Moving the existing terminal.".into()],
                &["Continue in background"],
            )),
            note: "Moving the existing terminal.".into(),
            ..Manager::default()
        };
        let mut c = crate::render::Field::new(90, 24, 1).paint(0);
        m.paint_at(&mut c, 23, 100);
        let rows: Vec<String> = c
            .cells
            .chunks(90)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect())
            .collect();
        assert_eq!(
            rows.iter()
                .filter(|l| l.contains("Moving the existing terminal"))
                .count(),
            1,
            "{rows:#?}"
        );
    }
}
