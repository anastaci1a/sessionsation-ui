//! Public-contract management reducer. Rendering and selection perform no I/O.
use crate::{
    client::Account,
    dialog::{self, Dialog, Field, Palette, Rule},
    render::{Canvas, Cell},
    usage::{AccountsLayout, Usage, account_title},
    widgets::{NAME_MAX_CHARS, Options, Row, hint_lines},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Accounts,
    Account(String),
    Sessions,
    Archives,
    Session(String),
    Spaces(String),
    Operations,
    Operation(String),
    Launch(&'static str, String),
}
#[derive(Clone, Debug)]
pub enum Intent {
    Api(u64, Vec<String>),
    New,
    Settings,
    Resume(Value),
    Switch(Value),
    Focus(Value),
    /// Run a backend login plan in a HERDR pane; the plan is never logged.
    LaunchLogin(Vec<String>),
    Exit,
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
    Limits(String),
    Mutation,
    LoginPoll(String),
    Observe(String),
    ActionCheck(Value, crate::session_list::Action),
}
#[derive(Clone, Debug)]
enum Form {
    None,
    Loading,
    AddProvider(Vec<&'static str>),
    Add(String),
    Login(String),
    LoginWait(String),
    AccountRename(String),
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
    Placement(Value, String, Place),
    Added(Value),
    Confirm(Vec<String>),
    Progress,
    Operation(String),
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
    pub dialog: Option<Dialog>,
    pub palette: Palette,
    pub note: String,
    pub inline: bool,
    pub usage_focus: bool,
    pub inventory_known: bool,
    pub sessions_known: bool,
    form: Form,
    provider_filter: Option<String>,
    sequence: u64,
    pending: BTreeMap<u64, Purpose>,
    readiness: BTreeMap<String, Value>,
    space_choices: Value,
    space_session: Value,
    deletion_previews: BTreeMap<String, Value>,
    pub active_operation: Option<String>,
    foreground: Option<u64>,
    details_open: bool,
    session_return: Option<String>,
    observe_after: Option<std::time::Instant>,
    history: Vec<Location>,
    popup_size: Option<crate::popup::Size>,
    login_wait: Option<LoginWait>,
    mutation: Vec<String>,
    /// The UI's own HERDR server: {"session", "socket"}.
    pub host: Value,
    /// The one focused Space of that server, when unambiguous.
    pub focused_space: Option<String>,
    place: Option<Place>,
    merge_target: Option<(String, u8)>,
}
/// Automatic provider login in progress: the backend plan runs in a HERDR pane
/// while the account is polled until it no longer needs authentication.
pub struct LoginWait {
    key: String,
    argv: Vec<String>,
    started: std::time::Instant,
    last_poll: Option<std::time::Instant>,
}
const LOGIN_POLL: std::time::Duration = std::time::Duration::from_secs(2);
const LOGIN_LIMIT: std::time::Duration = std::time::Duration::from_secs(600);
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
            dialog: None,
            palette: Palette::default(),
            note: String::new(),
            inline: false,
            usage_focus: false,
            inventory_known: false,
            sessions_known: false,
            form: Form::None,
            provider_filter: None,
            sequence: 0,
            pending: BTreeMap::new(),
            readiness: BTreeMap::new(),
            space_choices: Value::Null,
            space_session: Value::Null,
            deletion_previews: BTreeMap::new(),
            active_operation: None,
            foreground: None,
            details_open: false,
            session_return: None,
            observe_after: None,
            history: vec![],
            popup_size: None,
            login_wait: None,
            mutation: vec![],
            host: Value::Null,
            focused_space: None,
            place: None,
            merge_target: None,
        }
    }
}
fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).into()).collect()
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
/// Usage details body. Missing windows and unknown state are stated, never 0%.
fn usage_lines(a: &Account, now: i64) -> Vec<String> {
    let usage = Usage::account(a);
    let mut body = vec![a.id.clone()];
    if let Some(name) = a
        .details
        .get("display_name")
        .and_then(Value::as_str)
        .map(clean)
        .filter(|n| !n.is_empty())
    {
        body.push(format!("Display name: {name}"));
    }
    body.extend([
        format!("Provider: {}", crate::client::provider_label(a.provider())),
        format!("Status: {}", usage.status),
    ]);
    if let Some(state) = crate::usage::usage_state(a) {
        body.push(format!("Usage state: {}", clean(state)));
    }
    if let Some(source) = a.usage["source"]
        .as_str()
        .or_else(|| a.details.get("usage_source").and_then(Value::as_str))
    {
        body.push(format!("Source: {}", clean(source)));
    }
    if let Some(semantics) = a.usage["semantics"].as_str() {
        body.push(format!("Semantics: {}", clean(semantics)));
    }
    body.push(format!(
        "Observed: {}",
        relative(now, a.usage["observed_at"].as_i64())
    ));
    for (window, label) in [("short", "Short window"), ("weekly", "Weekly")] {
        let w = &a.usage[window];
        let used = w["used_percent"].as_f64().filter(|n| n.is_finite());
        body.push(match (w.is_object(), used) {
            (true, Some(used)) => format!(
                "{label} ({} minutes): {used}% used; resets {}",
                w["duration_minutes"],
                w["resets_at"]
                    .as_i64()
                    .map_or("at an unknown time".into(), |t| format!(
                        "at {}",
                        crate::status::local_time(t)
                    ))
            ),
            (_, None) if crate::usage::usage_state(a) == Some("unsupported") => {
                format!("{label}: usage unsupported")
            }
            (true, None) => format!("{label}: usage unknown"),
            (false, _) => format!("{label}: unavailable"),
        });
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
        "unknown" => "checking",
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
fn terminal(phase: &str) -> bool {
    matches!(
        phase,
        "complete" | "completed" | "cancelled" | "failed" | "recovered"
    )
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
            row.with_selected_preview(format!("({reason})"))
                .preview_gap(1)
        }
    }
    fn rename_account_row(&self, key: &str) -> Row {
        let provider = self.provider_of(key);
        let row = Row::gate("Rename", self.provider_cap(&provider, "account_rename"));
        if self.provider_cap(&provider, "account_rename") {
            row
        } else {
            let reason = self
                .provider_reason(&provider, "account_rename")
                .unwrap_or_else(|| "unsupported".into());
            row.with_selected_preview(format!("({reason})"))
                .preview_gap(1)
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
            row.with_selected_preview(format!("({reason})"))
                .preview_gap(1)
        }
    }
    /// Providers the backend reports as implemented and that this UI has a flow for.
    fn add_providers(&self) -> Vec<&'static str> {
        ["codex", "claude", "gemini"]
            .into_iter()
            .filter(|id| {
                self.capabilities["providers"].as_array().is_some_and(|ps| {
                    ps.iter()
                        .any(|p| p["id"] == *id && p["implemented"] == true)
                })
            })
            .collect()
    }
    fn mixed_providers(&self) -> bool {
        self.accounts
            .iter()
            .any(|a| a.provider() != self.accounts[0].provider())
    }
    /// Provider filter is meaningful only when more than one provider is listed.
    fn filter_row(&self) -> bool {
        self.mixed_providers()
    }
    fn active_filter(&self) -> Option<&str> {
        self.provider_filter
            .as_deref()
            .filter(|_| self.filter_row())
    }
    /// Accounts shown on Manage Accounts, after the provider filter.
    fn visible_accounts(&self) -> Vec<&Account> {
        self.accounts
            .iter()
            .filter(|a| self.active_filter().is_none_or(|f| a.provider() == f))
            .collect()
    }
    /// Row index of the first account on Manage Accounts.
    fn account_base(&self) -> usize {
        2 + usize::from(self.filter_row())
    }
    fn cycle_filter(&mut self) {
        let mut providers: Vec<String> = vec![];
        for a in &self.accounts {
            if !providers.iter().any(|p| p == a.provider()) {
                providers.push(a.provider().to_owned());
            }
        }
        providers.sort();
        self.provider_filter = match self.active_filter() {
            None => providers.first().cloned(),
            Some(current) => providers
                .iter()
                .position(|p| p == current)
                .and_then(|i| providers.get(i + 1))
                .cloned(),
        };
        self.options.selected = 1;
    }
    fn filter_label(&self) -> String {
        format!(
            "Provider: {}",
            self.active_filter()
                .map_or("All", crate::client::provider_label)
        )
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
            row.with_selected_preview(format!("({reason})"))
                .preview_gap(1)
        } else {
            row
        }
    }
    fn selected_account(&self) -> Option<String> {
        match &self.page {
            Page::Home if self.options.selected >= 5 => self
                .accounts
                .get(self.options.selected - 5)
                .map(|a| a.id.clone()),
            Page::Accounts if self.options.selected >= self.account_base() => self
                .visible_accounts()
                .get(self.options.selected - self.account_base())
                .map(|a| a.id.clone()),
            Page::Account(id) => Some(id.clone()),
            _ => None,
        }
    }
    fn usage_selectable(&self) -> bool {
        self.page == Page::Home
            && self.inline
            && self
                .selected_account()
                .as_deref()
                .and_then(|id| self.account(id))
                .is_some()
    }
    fn show_usage(&mut self, a: &Account) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        self.show("Details", usage_lines(a, now), &["Close"], Form::None);
    }
    fn show_login(&mut self, key: &str) {
        self.show(
            "Login",
            vec![format!("Start provider login for {key}?")],
            &["Login", "Cancel"],
            Form::Login(key.into()),
        );
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
                self.show_login(&key)
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
        let selected = self.selected_account();
        self.accounts = accounts;
        self.derive_session_fields();
        self.inventory_known = true;
        self.validate_dialog_names();
        if let Some(id) = selected {
            if let Some(index) = self.accounts.iter().position(|a| a.id == id) {
                match self.page {
                    Page::Home if self.inline => self.options.selected = 5 + index,
                    Page::Accounts => {
                        self.options.selected = self
                            .visible_accounts()
                            .iter()
                            .position(|a| a.id == id)
                            .map_or(0, |i| self.account_base() + i)
                    }
                    _ => {}
                }
            } else if matches!(self.page, Page::Home | Page::Accounts) {
                self.options.selected = 0;
            }
        }
    }
    fn validate_dialog_names(&mut self) {
        let excluded = match &self.form {
            // Names are unique across providers, so every existing name is taken.
            Form::Add(_) => self
                .accounts
                .iter()
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
            Page::Home => "",
            Page::Accounts => "Manage Accounts",
            Page::Account(key) => {
                return format!(
                    "Account: {}",
                    key.split_once(':').map_or(key.as_str(), |(_, name)| name)
                );
            }
            Page::Sessions => "Manage Sessions",
            Page::Archives => "Archived Sessions",
            Page::Session(key) | Page::Spaces(key) => {
                return format!(
                    "Session: {}",
                    self.session(key)
                        .and_then(|s| s["name"].as_str())
                        .unwrap_or("Unnamed")
                );
            }
            Page::Operations => "Operations",
            Page::Launch(..) => "Launch progress",
            Page::Operation(_) => "Operation",
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
        self.options = Options::default();
        self.options.selected = self.rows().iter().position(|r| !r.separator).unwrap_or(0);
        self.note.clear();
    }
    fn focus(&self) -> Option<Focus> {
        let index = self.options.selected;
        match &self.page {
            Page::Home | Page::Accounts if self.selected_account().is_some() => {
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
            Some(Focus::Account(key)) => {
                if self.page == Page::Home {
                    self.accounts
                        .iter()
                        .position(|a| a.id == key)
                        .map(|i| i + 5)
                } else {
                    self.visible_accounts()
                        .iter()
                        .position(|a| a.id == key)
                        .map(|i| i + self.account_base())
                }
            }
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
                .position(|entry| entry == &(kind.clone(), key.clone())),
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
        self.usage_focus = location.usage_focus && self.usage_selectable();
        true
    }
    fn account_back(&mut self, key: &str) {
        self.go(if self.inline {
            Page::Home
        } else {
            Page::Accounts
        });
        self.options.selected = if self.inline {
            self.accounts
                .iter()
                .position(|a| a.id == key)
                .map(|i| i + 5)
        } else {
            self.visible_accounts()
                .iter()
                .position(|a| a.id == key)
                .map(|i| i + self.account_base())
        }
        .unwrap_or(0);
    }
    pub fn rows(&self) -> Vec<Row> {
        match &self.page {
            Page::Home => {
                let mut rows = vec![
                    Row::yes("New Session"),
                    Row::yes("Manage Sessions"),
                    Row::yes(if self.inline {
                        "Add Account"
                    } else {
                        "Manage Accounts"
                    }),
                    Row::separator(),
                    Row::yes("Settings"),
                ];
                if self.inline {
                    rows.extend(
                        self.accounts
                            .iter()
                            .map(|a| Row::yes(account_title(a)).account()),
                    );
                }
                rows
            }
            Page::Accounts => {
                let column = self
                    .accounts
                    .iter()
                    .map(|a| unicode_width::UnicodeWidthStr::width(account_title(a).as_str()))
                    .max()
                    .unwrap_or(0)
                    + 4;
                let visible = self.visible_accounts();
                let tagged = self.accounts.iter().any(|a| a.provider() != "codex");
                std::iter::once(Row::yes("Add Account"))
                    .chain(self.filter_row().then(|| Row::yes(self.filter_label())))
                    .chain((!visible.is_empty()).then(Row::separator))
                    .chain(visible.into_iter().map(|a| {
                        let enabled =
                            a.details.get("enabled").and_then(Value::as_bool) == Some(true);
                        let state = if enabled {
                            if Usage::account(a).status != "Ready"
                                || a.details
                                    .get("switch_thresholds")
                                    .is_some_and(|v| v["needs_plan_review"] == true)
                            {
                                "Enabled*"
                            } else {
                                "Enabled"
                            }
                        } else {
                            "Disabled"
                        };
                        Row::yes(account_title(a)).with_column_preview(
                            if tagged {
                                format!(
                                    "{:<11} {state}",
                                    crate::client::provider_label(a.provider())
                                )
                            } else {
                                state.to_owned()
                            },
                            column,
                            enabled,
                        )
                    }))
                    .collect()
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
                let mut rows = vec![
                    Row::yes("Details"),
                    self.limits_account_row(key, removed),
                    self.rename_account_row(key),
                    Row::separator(),
                    Row::gate(
                        "Login",
                        !removed && self.provider_cap(&self.provider_of(key), "enrollment"),
                    ),
                    Row::gate(
                        if a.is_some_and(|a| a.details.get("enabled").is_some_and(|v| v == false)) {
                            "Enable"
                        } else {
                            "Disable"
                        },
                        !removed,
                    ),
                    Row::separator(),
                    self.remove_account_row(key, removed),
                    self.delete_account_row(key),
                ];
                if !a.is_some_and(|a| a.details.get("auth_required").is_some_and(|v| v == true)) {
                    rows.remove(4);
                }
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
                .map(|s| {
                    Row::yes(crate::usage::display_name(
                        s["name"].as_str().unwrap_or("Unnamed"),
                    ))
                    .with_selected_preview(format!("({})", id(s)))
                    .preview_gap(1)
                })
                .collect(),
            Page::Spaces(_) => self.space_choices["spaces"]
                .as_array()
                .map(|spaces| {
                    let rows = spaces
                        .iter()
                        .map(|space| {
                            let row = Row::gate(
                                space["name"].as_str().unwrap_or("Unnamed Space"),
                                space["current"] != true,
                            );
                            if space["current"] == true {
                                row.with_selected_preview("(current)").preview_gap(1)
                            } else {
                                row
                            }
                        })
                        .collect::<Vec<_>>();
                    crate::popup::with_cancel(rows)
                        .into_iter()
                        .map(Row::popup_option)
                        .collect()
                })
                .unwrap_or_else(|| vec![Row::yes("Cancel").popup_option()]),
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
            Page::Operations => {
                let entries = self.operation_entries();
                let width = entries.iter().map(|e| e.2.len()).max().unwrap_or(0);
                entries
                    .into_iter()
                    .map(|(kind, key, label, record)| {
                        let label = format!("{label}:");
                        let width = width + 1;
                        let row = Row::yes(format!(
                            "{label:<width$} {}",
                            if kind == "operation" {
                                operation_status(record)
                            } else {
                                launch_status(record)
                            }
                        ));
                        row.with_selected_preview(format!("({key})")).preview_gap(1)
                    })
                    .collect()
            }
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
                    Row::gate(
                        "Recover",
                        o["can_recover"].as_bool().unwrap_or(
                            phase == "failed"
                                && matches!(
                                    o["kind"].as_str(),
                                    Some(
                                        "account_delete"
                                            | "account"
                                            | "restart"
                                            | "directory"
                                            | "pause"
                                            | "terminate"
                                    )
                                ),
                        ),
                    ),
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
    fn operation_keys(&self) -> Vec<(String, String)> {
        self.operation_entries()
            .into_iter()
            .map(|(kind, key, _, _)| (kind.into(), key.into()))
            .collect()
    }
    fn retain_operation_focus(&mut self, chosen: Option<(String, String)>) {
        if let Some(chosen) = chosen {
            self.options.selected = self
                .operation_keys()
                .iter()
                .position(|k| *k == chosen)
                .unwrap_or(0);
        }
    }
    fn launch_record(&self, kind: &str, key: &str) -> Option<&Value> {
        self.launches
            .get(kind)?
            .iter()
            .find(|r| r["request"]["request_id"] == key)
    }
    pub fn refresh(&mut self) -> Vec<Intent> {
        let mut out = vec![];
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
        out.extend(self.refresh_sessions());
        if (self.active_operation.is_some()
            || matches!(self.form, Form::Operation(_) | Form::OperationApproval(..))
            || matches!(self.page, Page::Operations | Page::Operation(_)))
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
    pub fn read_sessions(&mut self) -> Vec<Intent> {
        if !self.pending.values().any(|p| matches!(p, Purpose::State)) {
            self.request(strings(&["state"]), Purpose::State)
        } else {
            vec![]
        }
    }
    pub fn refresh_sessions(&mut self) -> Vec<Intent> {
        if matches!(self.page, Page::Sessions | Page::Session(_))
            && !self.pending.values().any(|p| matches!(p, Purpose::State))
        {
            self.request(strings(&["state"]), Purpose::State)
        } else {
            vec![]
        }
    }
    /// Result of opening the login pane. Only a failure is surfaced, with the
    /// command a person can run themselves.
    pub fn login_launched(&mut self, result: Result<(), String>) -> Vec<Intent> {
        if let Err(e) = result
            && let Some(wait) = self.login_wait.take()
            && matches!(&self.form, Form::LoginWait(k) if *k == wait.key)
        {
            self.dialog = None;
            self.show(
                "Login pane could not open",
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
            self.login_wait = None;
            self.dialog = None;
            self.show(
                "Login not completed",
                vec![
                    "Timed out waiting for sign-in. Choose Login on the account page to try again."
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
            "Login complete",
            vec![format!("{label} is signed in for {}.", account_label(&key))],
            &["Close"],
            Form::None,
        );
    }
    pub fn refresh_live(&mut self) -> Vec<Intent> {
        let login = self.login_poll();
        if !login.is_empty() {
            return login;
        }
        if matches!(
            self.page,
            Page::Operations | Page::Operation(_) | Page::Launch(..)
        ) || matches!(self.form, Form::Operation(_) | Form::OperationApproval(..))
        {
            self.refresh()
        } else {
            self.refresh_sessions()
        }
    }
    /// Continuation, branch and native-kind facts; absent facts are simply not shown.
    fn identity_lines(value: &Value) -> Vec<String> {
        let text = |v: &Value| v.as_str().map(clean).filter(|s| !s.is_empty());
        let mut lines = vec![];
        if let Some(ids) = value["previous_conversation_ids"].as_array() {
            let ids: Vec<_> = ids.iter().filter_map(&text).collect();
            if !ids.is_empty() {
                lines.push(format!("Earlier native session IDs: {}", ids.join(", ")));
            }
        }
        if let Some(parent) = text(&value["forked_from_conversation_id"]) {
            lines.push(format!("Forked from: {parent}"));
        }
        if value["native"].is_object() {
            let kind = match text(&value["native"]["kind"]).as_deref() {
                Some("bg" | "background") => "background".to_owned(),
                Some(other) => other.to_owned(),
                None => "unknown".to_owned(),
            };
            lines.push(format!("Native kind: {kind}"));
            if let Some(job) = text(&value["native"]["job_id"]) {
                lines.push(format!("Native job ID: {job}"));
            }
        }
        lines
    }
    fn claude_detail_lines(value: &Value) -> Vec<String> {
        let text = |v: &Value| v.as_str().map(clean).filter(|s| !s.is_empty());
        let provider = if crate::session_list::is_gemini(value) {
            "gemini"
        } else {
            "claude"
        };
        let status = match value["availability"].as_str() {
            Some("running") => match value["native_status"].as_str() {
                Some("idle") => "running (idle)",
                Some("busy") => "running (busy)",
                _ => "running",
            },
            Some("stopped") => "stopped",
            Some("stale") => "stale",
            _ => "unknown",
        };
        let pid = value["attachment"]["pid"]
            .as_u64()
            .or_else(|| value["pid"].as_u64())
            .filter(|_| value["availability"] == "running");
        let mut lines = vec![
            format!("Provider: {}", crate::client::provider_label(provider)),
            format!(
                "Managed account: {}",
                text(&value["account_id"]).unwrap_or_else(|| "unknown".into())
            ),
            format!(
                "{} session ID (native): {}",
                if provider == "gemini" {
                    "Gemini"
                } else {
                    "Claude"
                },
                text(&value["conversation_id"]).unwrap_or_else(|| "unknown".into())
            ),
            format!(
                "Directory: {}",
                text(&value["directory"]).unwrap_or_else(|| "unknown".into())
            ),
            format!("Status: {status}"),
            format!(
                "Sessionsation name: {}",
                text(&value["name"]).unwrap_or_else(|| "unknown".into())
            ),
            format!(
                "Native name: {}",
                text(&value["native_name"]).unwrap_or_else(|| "unknown".into())
            ),
        ];
        lines.splice(3..3, Self::identity_lines(value));
        if let Some(pid) = pid {
            lines.push(format!("Process ID: {pid}"));
        }
        lines.push(format!("Session: {}", id(value)));
        lines
    }
    fn session_detail_lines(value: &Value) -> Vec<String> {
        if crate::session_list::is_native_provider(value) {
            return Self::claude_detail_lines(value);
        }
        let mut lines = vec![
            format!("Name: {}", value["name"].as_str().unwrap_or("")),
            format!("Account: {}", value["account_id"].as_str().unwrap_or("")),
            format!(
                "Created: {}",
                value["conversation_created_at"]
                    .as_str()
                    .unwrap_or("Unknown")
            ),
            format!(
                "Saved history: {}",
                value["history_file_bytes"]
                    .as_u64()
                    .map(|n| format!("{n} bytes"))
                    .unwrap_or_else(|| "Unknown".into())
            ),
            format!(
                "Directory: {}",
                value["directory"].as_str().unwrap_or("unavailable")
            ),
            format!("Session: {}", id(value)),
            format!(
                "Conversation: {}",
                value["conversation_id"].as_str().unwrap_or("")
            ),
            format!("Generation: {}", value["generation"].as_str().unwrap_or("")),
        ];
        lines.splice(7..7, Self::identity_lines(value));
        lines
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
        dialog.size = self.dialog.as_ref().and_then(|d| d.size);
        if matches!(form, Form::Loading) {
            dialog.minimum_height = 16;
            dialog.minimum_width = 76;
        }
        self.dialog = Some(dialog);
        self.form = form;
    }
    fn mutate(&mut self, args: Vec<String>) -> Vec<Intent> {
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
        if s["capabilities"]["switch_account"] != true {
            self.note = "The session is not currently available for account switching. Review its state in Manage Sessions.".into();
            return;
        }
        let mut args = Self::target(&s, "switch");
        args.extend(strings(&["--to", account]));
        self.confirm("Switch account",vec![format!("Move {} to {} while retaining conversation history? The backend waits for safe idle; it never steers.",s["name"].as_str().unwrap_or("session"),account)],args);
    }
    fn dismiss(&mut self) {
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
        self.login_wait = None;
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
            match d.key_with_wrap(key, wrap) {
                dialog::Event::None => return vec![],
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
        match key.code {
            KeyCode::Home | KeyCode::End => {
                self.options
                    .boundary(&self.rows(), false, key.code == KeyCode::End);
                if !self.usage_selectable() {
                    self.usage_focus = false;
                }
                vec![]
            }
            KeyCode::Up | KeyCode::Down => {
                self.options
                    .step_with_wrap(&self.rows(), false, key.code == KeyCode::Up, wrap);
                if !self.usage_selectable() {
                    self.usage_focus = false;
                }
                vec![]
            }
            KeyCode::Left if matches!(self.page, Page::Spaces(_)) => vec![],
            KeyCode::Left => {
                if self.usage_focus {
                    self.usage_focus = false;
                } else {
                    self.options.back = true;
                }
                vec![]
            }
            KeyCode::Right => {
                if !self.options.back && self.usage_selectable() {
                    self.usage_focus = true;
                }
                self.options.back = false;
                vec![]
            }
            KeyCode::Esc => self.back(),
            KeyCode::Enter | KeyCode::Char(' ') => {
                if self.options.back {
                    self.back()
                } else {
                    self.activate()
                }
            }
            _ => vec![],
        }
    }
    pub fn paste(&mut self, text: &str) {
        if let Some(d) = &mut self.dialog {
            d.paste(text);
        }
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
                "Add Account",
                vec!["Choose the provider for the new account.".into()],
                &buttons,
                Form::AddProvider(providers),
            );
        } else {
            self.add_for(providers.first().copied().unwrap_or("codex"));
        }
    }
    fn add_for(&mut self, provider: &str) {
        let label = crate::client::provider_label(provider);
        let mut body = vec![format!(
            "{label} account name: 1–{NAME_MAX_CHARS} letters, digits, hyphens or underscores."
        )];
        if matches!(provider, "claude" | "gemini") {
            body.push(format!(
                "Login runs {label}'s own interactive sign-in in a terminal."
            ));
        }
        self.show(
            "Add Account",
            body,
            &["Add account", "Cancel"],
            Form::Add(provider.to_owned()),
        );
        self.dialog
            .as_mut()
            .unwrap()
            .fields
            .push(Field::new("Name", "").validated(Rule::AccountName));
        self.validate_dialog_names();
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
            Page::Home => match index {
                0 => return vec![Intent::New],
                1 => {
                    self.enter(Page::Sessions);
                    return self.refresh();
                }
                2 if self.inline => self.add(),
                2 => self.enter(Page::Accounts),
                4 => return vec![Intent::Settings],
                i if i >= 5 => {
                    if let Some(a) = self.accounts.get(i - 5) {
                        self.enter(Page::Account(a.id.clone()));
                    }
                }
                _ => {}
            },
            Page::Accounts => {
                if index == 0 {
                    self.add()
                } else if self.filter_row() && index == 1 {
                    self.cycle_filter();
                } else if let Some(key) = index
                    .checked_sub(self.account_base())
                    .and_then(|i| self.visible_accounts().get(i).map(|a| a.id.clone()))
                {
                    self.enter(Page::Account(key));
                }
            }
            Page::Account(key) => {
                let Some(a) = self.account(&key).cloned() else {
                    self.note = "Account is no longer available".into();
                    return vec![];
                };
                if let Some(operation) = a.details.get("deletion").filter(|v| v["id"].is_string()) {
                    return self.deletion_operation(operation.clone());
                }
                let index =
                    if a.details.get("auth_required").is_some_and(|v| v == true) || index < 4 {
                        index
                    } else {
                        index + 1
                    };
                match index {
                    0 => self.show_usage(&a),
                    1 => {
                        return self.preview(
                            "Account limits",
                            strings(&["account", "limits", &key]),
                            Purpose::Limits(key),
                        );
                    }
                    2 if self.provider_cap(&self.provider_of(&key), "account_rename") => {
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
                    4 => self.show_login(&key),
                    5 => {
                        if a.details.get("enabled").is_some_and(|v| v == false) {
                            return self.mutate(strings(&["account", "enable", &key]));
                        } else {
                            self.confirm("Disable account",vec![format!("Disable {key} for future selection? Existing agents keep running.")],strings(&["account","disable",&key]));
                        }
                    }
                    7 => {
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
                    8 => {
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
                    self.enter(Page::Operations);
                }
                return self.refresh();
            }
            Page::Archives => {
                if let Some(s) = self.archives.get(index) {
                    self.enter(Page::Session(id(s).to_owned()));
                }
            }
            Page::Spaces(_) => {
                if let Some(space) = self.space_choices["spaces"]
                    .as_array()
                    .and_then(|spaces| spaces.get(index))
                {
                    if space["current"] == true {
                        return vec![];
                    }
                    if let Some(place) = self.place {
                        let space_id = space["id"].as_str().unwrap_or("").to_owned();
                        let space_name = space["name"].as_str().unwrap_or("the selected Space");
                        let session = self.space_session.clone();
                        let name = session["name"].as_str().unwrap_or("").to_owned();
                        let (title, body, button) = match place {
                            Place::Add => (
                                "Add to HERDR",
                                format!(
                                    "Open this running session in a new tab in {space_name}. HERDR only views the existing session: no agent is started and no input is sent."
                                ),
                                "Add to HERDR",
                            ),
                            Place::Start => (
                                "Start Session",
                                format!(
                                    "Start this saved session in a new tab in {space_name}. The conversation is resumed; no message is sent."
                                ),
                                "Start Session",
                            ),
                        };
                        self.show(
                            title,
                            vec![body],
                            &[button, "Cancel"],
                            Form::Placement(session, space_id, place),
                        );
                        self.dialog
                            .as_mut()
                            .unwrap()
                            .fields
                            .push(Field::new("Name", name.clone()).validated(Rule::Rename(name)));
                        self.validate_dialog_names();
                        return vec![];
                    }
                    let mut args = Self::target(&self.space_session, "move-space");
                    args.extend(strings(&[
                        "--space",
                        space["id"].as_str().unwrap_or(""),
                        "--request-id",
                        &uuid::Uuid::new_v4().to_string(),
                    ]));
                    self.confirm("Move to Space", vec![format!("Move this terminal to a new tab in {}? Its running process, account, directory and history stay unchanged. Split siblings stay in place; an empty source tab or Space may close.",space["name"].as_str().unwrap_or("the selected Space"))], args);
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
                if s["availability"] == "running"
                    && !crate::session_list::is_native_provider(&s)
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
                if let Some((kind, key)) = self.operation_keys().get(index).cloned() {
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
                        self.show(
                            if terminal(o["phase"].as_str().unwrap_or("")) {
                                "Operation details"
                            } else {
                                "Operation progress"
                            },
                            vec![
                                crate::messages::operation_phase(
                                    o["kind"].as_str().unwrap_or(""),
                                    o["phase"].as_str().unwrap_or("unknown"),
                                ),
                                o["detail"].as_str().unwrap_or("").into(),
                            ],
                            &[if terminal(o["phase"].as_str().unwrap_or("")) {
                                "Close"
                            } else {
                                "Continue in background"
                            }],
                            Form::Operation(key.clone()),
                        );
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
                    crate::session_list::Action::Archive => (
                        "archive",
                        "Archive Session",
                        "Compress and move saved conversation history to sessionsation-managed Archived Sessions. No agent is started; project files and account credentials stay in place.",
                    ),
                    crate::session_list::Action::Restore => (
                        "restore-session",
                        "Restore Session",
                        "Restore the saved conversation for later resume. This does not start an agent or send a message.",
                    ),
                    _ => (
                        "delete-session",
                        "Delete Session",
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
                return self.refresh_sessions();
            }
            crate::session_list::Action::Rename => {
                let claude = crate::session_list::is_native_provider(&s);
                self.show(
                    "Rename session",
                    vec![if claude {
                        if crate::session_list::is_gemini(&s) {
                            "Changes the Sessionsation name only. Gemini CLI has no native rename."
                        } else {
                            "Changes the Sessionsation name only. Claude's own title is applied the next time this session is launched or resumed."
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
                self.confirm(
                    "Session confirmation",
                    vec![format!(
                        "{}: {command}. Conversation history is retained.",
                        s["name"].as_str().unwrap_or("session")
                    )],
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
        if let Form::AddProvider(providers) = self.form.clone() {
            self.dialog = None;
            self.form = Form::None;
            if let Some(provider) = providers.get(button) {
                self.add_for(provider);
            }
            return vec![];
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
            | Form::LoginWait(_)
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
                args.extend(strings(&["--space", &space, "--name", value(0)]));
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
            Form::AddProvider(_) => return vec![],
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
                if matches!(purpose, Purpose::State) {
                    for session in &mut self.sessions {
                        session["activity"] = json!("unknown");
                    }
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
            Purpose::DeleteAvailability(key) => {
                self.deletion_previews.insert(key, value);
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
                let rank = |s: &Value| {
                    if s["availability"] == "running" {
                        if s["activity"] == "busy" { 0 } else { 1 }
                    } else {
                        2
                    }
                };
                self.sessions.sort_by(|a, b| {
                    rank(a)
                        .cmp(&rank(b))
                        .then_with(|| {
                            b["last_accessed"]
                                .as_i64()
                                .cmp(&a["last_accessed"].as_i64())
                        })
                        .then_with(|| id(a).cmp(id(b)))
                });
                if initial_sessions {
                    self.options = Options::default();
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
                    }
                }
            }
            Purpose::Launches(kind) => {
                let chosen = (self.page == Page::Operations)
                    .then(|| self.operation_keys().get(self.options.selected).cloned())
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
                    .flatten();
                self.operations = value.as_array().cloned().unwrap_or_default();
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
                    d.body = vec![
                        crate::messages::operation_phase(
                            o["kind"].as_str().unwrap_or(""),
                            o["phase"].as_str().unwrap_or("unknown"),
                        ),
                        o["detail"].as_str().unwrap_or("").into(),
                    ];
                    if terminal(o["phase"].as_str().unwrap_or("")) {
                        d.finish();
                    }
                }
                if let Some(key) = &self.active_operation
                    && let Some(o) = self.operations.iter().find(|o| id(o) == key)
                    && terminal(o["phase"].as_str().unwrap_or(""))
                {
                    self.active_operation = None;
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
                    if let Some(index) = preferred.and_then(|want| {
                        self.space_choices["spaces"]
                            .as_array()?
                            .iter()
                            .position(|space| space["id"] == want.as_str())
                    }) {
                        self.options.selected = index;
                    }
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
                            "Its confirmed state appears in Manage Sessions after a refresh. Nothing is retried automatically.".into(),
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
                } else if matches!(self.provider_of(&key).as_str(), "claude" | "gemini") {
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
                self.note = "Operation acknowledged; refresh to see current state.".into();
                if let Some(argv) = login_plan(&value["login"]["argv"]) {
                    if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                        // The plan is a backend-owned exec argv; the UI only hands it
                        // to a HERDR pane and polls the account. It handles no tokens.
                        let key = match &self.mutation[..] {
                            [a, b, key] if a == "account" && (b == "add" || b == "login") => {
                                Some(key.clone())
                            }
                            _ => None,
                        };
                        if let Some(key) = key {
                            let label =
                                crate::client::provider_label(&self.provider_of(&key)).to_owned();
                            self.dialog = None;
                            self.show(
                                "Complete provider login",
                                vec![
                                    format!("Complete the {label} login in the opened pane…"),
                                    "This dialog closes when sign-in succeeds. Cancel only stops waiting; it does not cancel the login.".into(),
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
                        self.show(
                            "Complete provider login",
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
                        self.show("Complete provider login",vec![url.into(),format!("Code: {}",login["userCode"].as_str().unwrap_or("unavailable")),"Complete login in your browser. Closing this dialog does not cancel enrollment.".into()],&["Continue in background"],Form::None);
                    }
                } else if value["phase"].is_string() {
                    self.active_operation = Some(id(&value).into());
                    if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                        self.form = Form::Operation(id(&value).into());
                        if terminal(value["phase"].as_str().unwrap_or(""))
                            && let Some(d) = &mut self.dialog
                        {
                            d.finish();
                        }
                    }
                    return self.refresh();
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
                title: "Choose Space",
                description: "Choose a destination Space for this session.",
                rows: &rows,
                editor: None,
                note: &self.note,
                note_accent: false,
                large: false,
                fit_description: true,
                browse_hint: false,
            };
            if self
                .popup_size
                .is_none_or(|s| s.viewport != (canvas.width, canvas.height))
            {
                self.popup_size = Some(view.measure((canvas.width, canvas.height)));
            }
            self.popup_size =
                Some(view.grow_for_selection(self.popup_size.unwrap(), self.options.selected));
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
        let hints = hint_lines(
            &[
                "▲/▼/◀/▶: Move",
                "Enter/Space: Select",
                if self.page == Page::Home {
                    "Esc: Close"
                } else {
                    "Esc: Back"
                },
            ],
            canvas.width,
        );
        let controls_start = canvas.height.saturating_sub(hints.len());
        let list_controls = if self.note.is_empty() {
            controls_start
        } else {
            controls_start.saturating_sub(2)
        };
        let layout = AccountsLayout::fit(&self.accounts, canvas.width, footer.saturating_sub(10));
        let was = self.inline;
        // Unknown inventory is not evidence of overflow. Render the empty inline
        // layout immediately; switch to Manage Accounts only when rows won't fit.
        self.inline = layout.is_some();
        if self.inline && !was && self.page == Page::Accounts && self.dialog.is_none() {
            let selected = self.selected_account();
            self.go(Page::Home);
            if let Some(key) = selected {
                self.options.selected = self
                    .accounts
                    .iter()
                    .position(|a| a.id == key)
                    .map(|i| i + 5)
                    .unwrap_or(0);
            }
        }
        if !self.inline && was && self.page == Page::Home && self.options.selected >= 5 {
            self.options.selected = 2;
        }
        if self.usage_focus && !self.usage_selectable() {
            self.usage_focus = false;
        }
        let rows = self.rows();
        if self.page == Page::Home && self.inline {
            let l = layout.as_ref().unwrap();
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
            l.paint(
                canvas,
                &self.accounts,
                9,
                self.options.selected.checked_sub(5),
                self.options.back,
                self.usage_focus,
            );
            self.options.paint_back(canvas, top_left, 3, "Close");
        } else {
            let selected = self.selected_account();
            let usage = if self.page == Page::Accounts {
                selected
                    .as_ref()
                    .and_then(|key| self.account(key))
                    .filter(|a| {
                        !self.options.back
                            && a.details.get("enabled").and_then(Value::as_bool) != Some(false)
                    })
                    .map(Usage::account)
            } else {
                None
            };
            let preview_height = usage.as_ref().map(Usage::height).unwrap_or(0);
            let menu_controls = if preview_height > 0 {
                list_controls.saturating_sub(preview_height + 1)
            } else {
                list_controls
            };
            let bottom = menu_controls.saturating_sub(1);
            let (left, capacity) = if self.page == Page::Sessions {
                crate::session_list::paint_with_archives(
                    canvas,
                    &mut self.options,
                    &self.sessions,
                    self.sessions_known,
                    bottom,
                    now,
                    menu_controls,
                    !self.archives.is_empty(),
                )
            } else {
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
            if let Some(u) = usage {
                let bar = 24.min(canvas.width.saturating_sub(20));
                if bar < 12 {
                    canvas.text(
                        1,
                        footer.saturating_sub(3),
                        "Widen to view usage",
                        Cell::default(),
                        false,
                    );
                }
                if bar >= 12 {
                    u.paint_preview(
                        canvas,
                        canvas.width.saturating_sub(u.width(bar, u.digits())) / 2,
                        if rows.len() > capacity {
                            3 + capacity + 2
                        } else {
                            3 + rows.len().saturating_sub(self.options.scroll).min(capacity) + 1
                        },
                        bar,
                        u.digits(),
                    );
                }
            }
        }
        if !self.note.is_empty() {
            crate::widgets::paint_status(canvas, controls_start, &self.note);
        }
        crate::widgets::paint_hints(canvas, &hints);
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
                    assert_eq!(m.dialog.as_ref().unwrap().title, "Session confirmation");
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
    fn archived_ids_share_list_wide_preview_fit_and_unbolded_gray() {
        use super::*;
        let mut m = setup();
        m.page = Page::Archives;
        m.archives = vec![json!({"id":"short-id","name":"Saved"})];
        m.archives.extend((0..20).map(
            |i| json!({"id":format!("archive-{i}-with-a-longer-stable-session-id"),"name":"Other"}),
        ));
        let rows = m.rows();
        assert_eq!(rows[0].preview.as_deref(), Some("(short-id)"));
        assert!(rows.iter().all(|r| r.preview_selected_only
            && r.preview_fg == crate::widgets::SECONDARY
            && r.preview_gap == 1));
        assert!(Options::previews_detached(40, &rows));
        let mut c = Canvas::new(40, 24);
        m.options.paint_menu(&mut c, &rows, 3, 18, 23);
        let first = c.cells.iter().position(|c| c.glyph == "(").unwrap();
        assert!(first / 40 > 3);
        assert!(
            c.cells[first..first + 10]
                .iter()
                .all(|c| c.fg == crate::widgets::SECONDARY && !c.bold)
        );
        assert!(!Options::previews_detached(120, &rows));
        let mut c = Canvas::new(120, 24);
        m.options.paint_menu(&mut c, &rows, 3, 18, 23);
        assert_eq!(
            c.cells.iter().position(|c| c.glyph == "(").unwrap() / 120,
            3
        );
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
        m.options.selected = 2;
        m.key(enter);
        assert_eq!(m.page, Page::Accounts);
        m.options.selected = 3;
        m.options.scroll = 1;
        m.key(enter);
        assert_eq!(m.page, Page::Account("codex:account2".into()));
        m.accounts.reverse();
        m.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        m.key(enter);
        assert_eq!(m.page, Page::Accounts);
        assert_eq!(m.options.selected, 2);
        assert_eq!(m.options.scroll, 1);
        assert!(!m.options.back);
        m.key(escape);
        assert_eq!(m.page, Page::Home);
        assert_eq!(m.options.selected, 2);

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
        assert_eq!(m.page, Page::Operations);
        m.options.selected = 1;
        m.key(enter);
        assert_eq!(m.page, Page::Operation("o1".into()));
        m.operations
            .push(json!({"id":"o3","kind":"switch","created_at":3}));
        m.key(escape);
        assert_eq!(m.page, Page::Operations);
        assert_eq!(
            m.operation_keys()[m.options.selected],
            ("operation".into(), "o1".into())
        );
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
        assert_eq!(m.options.selected, 0);
        assert!(!m.rows()[0].enabled);
        assert!(m.activate().is_empty());
        m.options.selected = 1;
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
            let sent = m.preview("Start Session", vec![], Purpose::Resume("saved".into()));
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
    fn operations_share_creation_order_status_columns_and_id_presentation() {
        let mut m = setup();
        m.page = Page::Operations;
        m.operations = vec![
            json!({"id":"old","kind":"switch","phase":"complete","created_at":10,"updated_at":1000}),
            json!({"id":"new","kind":"account_delete","phase":"running","created_at":20,"updated_at":1}),
            json!({"id":"legacy","kind":"switch","phase":"complete","updated_at":2000}),
        ];
        m.launches.insert("launch",vec![json!({"request":{"request_id":"12345678-1234-4234-8234-123456789abc"},"phase":"running","created_at":30})]);
        m.launches.insert(
            "resume-launch",
            vec![json!({"request":{"request_id":"resume-id"},"phase":"running","created_at":15})],
        );
        let keys = m.operation_keys();
        assert_eq!(
            keys.iter().map(|(_, key)| key.as_str()).collect::<Vec<_>>(),
            vec![
                "12345678-1234-4234-8234-123456789abc",
                "new",
                "resume-id",
                "old",
                "legacy"
            ]
        );
        let rows = m.rows();
        assert!(rows[0].label.ends_with("complete"));
        assert!(
            rows.iter()
                .all(|r| !r.label.split_once(':').unwrap().0.ends_with(' '))
        );
        assert!(
            rows.iter()
                .all(|r| r.label.rfind(' ').unwrap() + 1 == rows[0].label.rfind(' ').unwrap() + 1)
        );
        assert!(rows.iter().all(|r| r.preview_gap == 1
            && r.preview_selected_only
            && r.preview.as_ref().unwrap().starts_with('(')
            && r.preview.as_ref().unwrap().ends_with(')')));
        assert!(Options::previews_detached(60, &rows));
        m.operations[0]["updated_at"] = json!(999999);
        assert_eq!(m.operation_keys(), keys);
        m.options.selected = 0;
        m.activate();
        assert_eq!(
            m.page,
            Page::Launch("launch", "12345678-1234-4234-8234-123456789abc".into())
        );
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
                .any(|s| s == "Name: Cached")
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
                .any(|s| s == "Name: Fresh")
        );
        assert!(
            m.dialog
                .as_ref()
                .unwrap()
                .body
                .iter()
                .any(|s| s == "Generation: new")
        );
        let sent = m.refresh_live();
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
                .any(|s| s == "Name: Fresh")
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
            m.mutate(strings(&[command, "--session", "two"]));
            m.submit(0, vec![]);
            assert_eq!(m.page, Page::Sessions);
            assert_eq!(m.options.selected, 0);
        }
    }
    #[test]
    fn stale_target_uses_centered_status_lane_and_reserves_menu_space() {
        let mut m = setup();
        m.page = Page::Accounts;
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
        assert_eq!(m.dialog.as_ref().unwrap().buttons[0].label, "Close");
        m.form = Form::Operation("done".into());
        m.dialog = Some(Dialog::progress("Working"));
        let requests = m.request(strings(&["operations"]), Purpose::Operations);
        let Intent::Api(token, _) = requests[0] else {
            panic!()
        };
        m.response(token, Ok(json!([{"id":"done","phase":"complete"}])));
        assert_eq!(m.dialog.as_ref().unwrap().buttons[0].label, "Close");
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
        assert_eq!(m.dialog.as_ref().unwrap().buttons[0].label, "Close");
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
    #[test]
    fn disabled_accounts_have_no_hover_usage_and_login_requires_attention() {
        let mut m = setup();
        m.accounts[0].details.insert("enabled".into(), json!(false));
        m.accounts[1]
            .details
            .insert("auth_required".into(), json!(true));
        m.accounts.extend((3..9).map(account));
        m.page = Page::Accounts;
        let rows = m.rows();
        assert_eq!(rows[2].preview.as_deref(), Some("Disabled"));
        assert_eq!(rows[2].preview_fg, crate::widgets::MUTED);
        assert_eq!(rows[3].preview.as_deref(), Some("Enabled*"));
        m.options.selected = 2;
        let mut c = Canvas::new(40, 18);
        m.paint(&mut c, 17);
        for y in 7..16 {
            let row = c.cells[y * 40..(y + 1) * 40]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>();
            assert!(!row.contains("Disabled"));
            assert!(!row.contains('━'));
        }
    }
    fn account(n: usize) -> Account {
        serde_json::from_value(json!({"id":format!("codex:account{n}"),"selectable":true,"enabled":true,"usage_fresh":true,"usage":{"observed_at":100,"short":{"duration_minutes":300,"used_percent":25},"weekly":{"duration_minutes":10080,"used_percent":35}},"switch_thresholds":{"customized":true,"short_percent":80,"weekly_percent":90}})).unwrap()
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
    #[test]
    fn home_usage_focus_limits_returns_to_same_selection() {
        let mut m = setup();
        m.inline = true;
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
                "Login",
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
            m.inline = true;
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
    fn account_status_column_and_preview_follow_list_or_pagination() {
        let mut m = setup();
        m.accounts[0].id = "codex:a".into();
        m.accounts[1].details.insert("enabled".into(), json!(false));
        m.page = Page::Accounts;
        let rows = m.rows();
        assert_eq!(
            rows[2].preview_position(60, 5),
            rows[3].preview_position(60, 5)
        );
        assert_eq!(rows[2].preview.as_deref(), Some("Enabled"));
        assert!(rows[2].preview_accent);
        assert!(!rows[3].preview_accent);
        m.inventory((0..30).map(account).collect());
        m.page = Page::Accounts;
        m.options.selected = 10;
        let mut c = Canvas::new(60, 18);
        m.paint(&mut c, 17);
        let row = |y: usize| {
            c.cells[y * 60..(y + 1) * 60]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
        };
        let range = (0..18).find(|y| row(*y).contains(" of ")).unwrap();
        assert!(!row(range + 1).contains('━'));
        assert!(row(range + 2).contains("5h"));
        assert!(row(range + 3).contains("wk"));
    }
    #[test]
    fn selected_operation_id_falls_below_list_only_when_it_fits() {
        let mut m = setup();
        let a = "11111111-1111-4111-8111-111111111111";
        let b = "22222222-2222-4222-8222-222222222222";
        m.operations = vec![
            json!({"id":a,"kind":"switch","phase":"complete"}),
            json!({"id":b,"kind":"switch","phase":"failed"}),
        ];
        m.go(Page::Operations);
        for (width, height, visible) in [
            (120, 24, true),
            (45, 16, true),
            (30, 16, false),
            (45, 10, true),
            (45, 9, false),
            (45, 8, false),
        ] {
            let mut c = Canvas::new(width, height);
            m.paint(&mut c, height - 1);
            let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
            assert_eq!(text.contains(a), visible, "{width}x{height}");
            assert!(!text.contains(b));
            if width == 45 && height == 16 {
                assert!(c.cells[5 * width..6 * width].iter().all(|c| c.glyph == " "));
                assert_eq!(c.cells[6 * width + (width - a.len() - 2) / 2].glyph, "(");
            }
        }
    }
    #[test]
    fn scrolling_operations_reserve_counter_id_and_control_gaps() {
        let mut m = setup();
        let a = "11111111-1111-4111-8111-111111111111";
        m.operations = (0..10)
            .map(|_| json!({"id":a,"kind":"switch","phase":"complete"}))
            .collect();
        m.go(Page::Operations);
        let mut c = Canvas::new(45, 16);
        m.paint(&mut c, 15);
        let line = |y: usize| {
            c.cells[y * 45..(y + 1) * 45]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
        };
        assert!(line(10).contains("1–7 of 10"));
        assert_eq!(line(10).find('1'), Some(18));
        assert_eq!(
            c.cells[10 * 45 + 18].fg,
            c.cells[14 * 45..15 * 45]
                .iter()
                .find(|c| c.glyph == "M")
                .unwrap()
                .fg
        );
        assert_eq!(line(11).trim(), "");
        assert!(line(12).contains(a));
        assert_eq!(line(13).trim(), "");
        assert!(line(14).contains("Move"));
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
        assert_eq!(m.rows()[2].label, "Add Account");
        m.options.selected = 2;
        m.activate();
        assert_eq!(m.dialog.as_ref().unwrap().title, "Add Account");
        m.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        m.inventory(vec![account(1), account(2)]);
        m.paint(&mut Canvas::new(80, 30), 29);
        assert_eq!(m.rows()[2].label, "Add Account");
        assert_eq!(m.options.selected, 2);
        m.inventory((0..30).map(account).collect());
        m.paint(&mut Canvas::new(80, 30), 29);
        assert_eq!(m.rows()[2].label, "Manage Accounts");
        assert_eq!(m.options.selected, 2);
    }
    #[test]
    fn inline_fit_compact_preview_and_resize_preserve_account_navigation() {
        let mut m = setup();
        m.paint(&mut Canvas::new(80, 30), 29);
        assert!(m.inline);
        assert_eq!(m.rows()[2].label, "Add Account");
        m.options.selected = 6;
        m.paint(&mut Canvas::new(40, 16), 14);
        assert!(!m.inline);
        assert_eq!(m.options.selected, 2);
        assert_eq!(m.rows()[2].label, "Manage Accounts");
        m.activate();
        assert_eq!(m.page, Page::Accounts);
        m.options.selected = 3;
        let mut canvas = Canvas::new(60, 16);
        m.paint(&mut canvas, 14);
        let row = |n: usize| {
            canvas.cells[n * 60..(n + 1) * 60]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
        };
        assert!(row(7).trim().is_empty());
        assert!(row(8).contains("5h"));
        assert!(row(9).contains("wk"));
        assert!(m.rows().iter().all(|r| !r.account));
        assert!(row(6).contains("●"));
        assert!(!row(6).contains("▶"));
        let name_x = row(6).find("account2").unwrap();
        assert_eq!(canvas.cells[6 * 60 + name_x].fg, [238; 3]);
        for y in [8, 9] {
            let percent = row(y).chars().position(|c| c == '%').unwrap();
            assert!(!canvas.cells[y * 60 + percent].bold);
            assert_eq!(canvas.cells[y * 60 + percent].fg, crate::widgets::SELECTED);
        }
        assert!(!row(13).contains('━'));
        m.paint(&mut Canvas::new(100, 40), 39);
        assert_eq!(m.page, Page::Home);
        assert_eq!(m.options.selected, 6);
        m.activate();
        assert_eq!(m.page, Page::Account("codex:account2".into()));
        m.paint(&mut Canvas::new(40, 16), 14);
        m.options.selected = 4;
        m.activate();
        m.dialog
            .as_mut()
            .unwrap()
            .body
            .push("Preserved input state".into());
        m.paint(&mut Canvas::new(100, 40), 39);
        assert!(
            m.dialog
                .as_ref()
                .unwrap()
                .body
                .contains(&"Preserved input state".into())
        );
        m.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        m.back();
        assert_eq!(m.page, Page::Home);
        assert_eq!(m.options.selected, 6);
    }
    #[test]
    fn grouped_actions_skip_separators_and_confirmation_binds_original_generation() {
        let mut m = setup();
        m.go(Page::Account("codex:account1".into()));
        assert!(m.rows()[3].separator && m.rows()[5].separator);
        m.options.selected = 1;
        m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(m.options.selected, 2);
        m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(m.options.selected, 4);
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
    fn add_account_chooses_provider_then_runs_the_provider_flow() {
        let mut m = dual_providers();
        m.add();
        let d = m.dialog.as_ref().unwrap();
        let buttons: Vec<_> = d.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(buttons, ["Codex", "Claude Code", "Cancel"]);
        assert!(d.fields.is_empty());
        m.submit(1, vec![]);
        let d = m.dialog.as_ref().unwrap();
        assert!(d.body[0].starts_with("Claude Code account name"));
        // Names are unique across providers: a Codex name is taken for Claude too.
        assert!(d.fields[0].excluded.contains(&"account1".to_owned()));
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
    fn claude_account_login_uses_the_account_provider_capability() {
        let mut m = dual_providers();
        m.accounts[1]
            .details
            .insert("auth_required".into(), json!(true));
        m.go(Page::Account("claude:claude-main".into()));
        let login = m.rows().iter().position(|r| r.label == "Login").unwrap();
        assert!(m.rows()[login].enabled);
        m.options.selected = login;
        m.activate();
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
                .find(|r| r.label == "Login")
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
    fn accounts_page_shows_provider_tags_and_filters_by_provider() {
        let mut m = dual_providers();
        m.go(Page::Accounts);
        let rows = m.rows();
        assert_eq!(rows[1].label, "Provider: All");
        let tagged: Vec<_> = rows[3..]
            .iter()
            .map(|r| (r.label.clone(), r.preview.clone().unwrap()))
            .collect();
        assert_eq!(tagged.len(), 3);
        assert!(tagged[0].1.starts_with("Codex"));
        assert!(tagged[1].1.starts_with("Claude Code"));
        for expect in ["Claude Code", "Codex", "All"] {
            m.options.selected = 1;
            m.activate();
            assert_eq!(m.rows()[1].label, format!("Provider: {expect}"));
        }
        m.options.selected = 1;
        m.activate();
        assert_eq!(m.rows()[1].label, "Provider: Claude Code");
        assert_eq!(m.rows().len(), 4);
        m.options.selected = 3;
        m.activate();
        assert_eq!(m.page, Page::Account("claude:claude-main".into()));
        // A Codex-only inventory has neither filter row nor tags.
        let mut m = setup();
        m.go(Page::Accounts);
        assert_eq!(m.rows()[0].label, "Add Account");
        assert!(m.rows().iter().all(|r| !r.label.starts_with("Provider")));
        assert!(
            m.rows()[2..]
                .iter()
                .all(|r| r.preview.as_deref().unwrap().starts_with("Enabled"))
        );
    }
    #[test]
    fn usage_details_label_source_semantics_age_and_missing_windows() {
        let lines = usage_lines(&claude("claude-main"), 400);
        let text = lines.join("\n");
        assert!(text.contains("Provider: Claude Code"));
        assert!(text.contains("Source: claude-code-statusline"));
        assert!(text.contains("Semantics: Claude Code statusline rate limits"));
        assert!(text.contains("Observed: 5m ago"));
        assert!(text.contains("40% used; resets at "));
        assert!(!text.contains("Unix"));
        assert!(text.contains("Weekly: unavailable"));
        assert!(!text.contains("Weekly (10080 minutes)"));
        let mut missing = claude("claude-new");
        missing.usage = json!({"state":"missing","observed_at":null,"short":null,"weekly":null});
        let text = usage_lines(&missing, 400).join("\n");
        assert!(text.contains("Status: Usage unavailable"));
        assert!(text.contains("Observed: unknown"));
        assert!(text.contains("Short window: unavailable"));
        assert!(!text.contains("0%"));
        let mut codex = account(1);
        codex.usage["weekly"] = Value::Null;
        assert!(
            usage_lines(&codex, 400)
                .join("\n")
                .contains("Weekly: unavailable")
        );
    }
    #[test]
    fn claude_session_details_use_native_labels_and_unknown_for_absent_values() {
        let mut live = claude_session("running");
        live["attachment"] =
            json!({"host":"herdr","target":"herdr/default/w:p","pid":4242,"process_start":1});
        live["native_name"] = json!("Native title");
        live["directory"] = json!("/work/example");
        let lines = Manager::session_detail_lines(&live);
        assert_eq!(
            lines,
            [
                "Provider: Claude Code",
                "Managed account: claude:claude-main",
                "Claude session ID (native): native-1",
                "Directory: /work/example",
                "Status: running (idle)",
                "Sessionsation name: Work",
                "Native name: Native title",
                "Process ID: 4242",
                "Session: c",
            ]
        );
        let stopped = Manager::session_detail_lines(&claude_session("stopped")).join("\n");
        assert!(stopped.contains("Status: stopped"));
        assert!(stopped.contains("Directory: unknown"));
        assert!(stopped.contains("Native name: unknown"));
        assert!(!stopped.contains("Process ID"));
        for (state, label) in [
            ("stale", "stale"),
            ("unknown", "unknown"),
            ("weird", "unknown"),
        ] {
            let text = Manager::session_detail_lines(&claude_session(state)).join("\n");
            assert!(text.contains(&format!("Status: {label}")));
        }
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
            labels,
            ["Focus Session", "Session Details", "Rename Session"]
        );
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
        assert_eq!(buttons, ["Codex", "Claude Code", "Gemini CLI", "Cancel"]);
        m.submit(2, vec![]);
        let d = m.dialog.as_ref().unwrap();
        assert!(d.body[0].starts_with("Gemini CLI account name"));
        assert!(d.body[1].contains("Gemini CLI's own interactive sign-in"));
        let sent = m.submit(0, vec!["gem-two".into()]);
        assert!(
            matches!(&sent[..], [Intent::Api(_, args)] if args == &strings(&["account", "add", "gemini:gem-two"]))
        );
        m.go(Page::Accounts);
        let rows = m.rows();
        assert!(rows.iter().any(|r| {
            r.preview
                .as_deref()
                .is_some_and(|p| p.starts_with("Gemini CLI"))
        }));
        let mut seen = vec![];
        for _ in 0..4 {
            m.options.selected = 1;
            m.activate();
            seen.push(m.rows()[1].label.clone());
        }
        assert_eq!(
            seen,
            [
                "Provider: Claude Code",
                "Provider: Codex",
                "Provider: Gemini CLI",
                "Provider: All"
            ]
        );
        // Unsupported usage is stated, never 0%; Limits is disabled with the backend reason.
        let u = Usage::account(&gemini("gem-main"));
        assert_eq!(u.status, "Usage unsupported");
        assert!(u.status_only());
        m.go(Page::Account("gemini:gem-main".into()));
        let rows = m.rows();
        let limits = rows.iter().find(|r| r.label == "Limits").unwrap();
        assert!(!limits.enabled);
        assert_eq!(
            limits.preview.as_deref(),
            Some("(Gemini CLI exposes no usage limits)")
        );
        let text = usage_lines(&m.accounts[3], 0).join("\n");
        assert!(text.contains("Provider: Gemini CLI"));
        assert!(text.contains("Status: Usage unsupported"));
        assert!(text.contains("Source: gemini-cli-unsupported"));
        assert!(text.contains("Short window: usage unsupported"));
        assert!(!text.contains('%'));
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
    #[test]
    fn gemini_sessions_use_claude_actions_native_label_and_status_gating() {
        let s = gemini_session(json!("idle"));
        let lines = Manager::session_detail_lines(&s).join("\n");
        assert!(lines.contains("Provider: Gemini CLI"));
        assert!(lines.contains("Gemini session ID (native): gem-native-1"));
        assert!(lines.contains("Status: running (idle)"));
        let labels: Vec<_> = crate::session_list::actions(&s, true)
            .into_iter()
            .map(|(r, _)| r.label)
            .collect();
        assert_eq!(
            labels,
            ["Focus Session", "Session Details", "Rename Session"]
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
        assert_eq!(rows[0].0.label, "Start Session");
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
            "This session's process is no longer verified. Refresh Manage Sessions; nothing was started or resumed."
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
        assert_eq!(d.title, "Complete provider login");
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
        assert_eq!(m.dialog.as_ref().unwrap().title, "Login complete");
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
        assert_eq!(m.dialog.as_ref().unwrap().title, "Login not completed");
        let mut m = dual_providers();
        claude_add(&mut m, "new");
        m.login_launched(Err("HERDR could not open the login pane".into()));
        let d = m.dialog.as_ref().unwrap();
        assert_eq!(d.title, "Login pane could not open");
        assert_eq!(d.body[2], "sessionsation account login claude:new");
        assert!(m.login_wait.is_none());
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
        assert_eq!(d.buttons[0].label, "Continue in background");
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
        assert_eq!(
            bottom,
            option + 1,
            "the frame ends right after the last option"
        );
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
        assert_eq!(&labels[..3], ["Details", "Limits", "Rename"]);
        assert!(!m.rows()[2].enabled, "no account_rename capability");
        m.capabilities["providers"][0]["account_rename"] = json!(true);
        assert!(m.rows()[2].enabled);
        m.options.selected = 2;
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
        m.options.selected = 2;
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
    #[test]
    fn display_names_replace_short_names_everywhere_with_fallback() {
        let mut m = dual_providers();
        m.accounts[0]
            .details
            .insert("display_name".into(), json!("Team Work"));
        assert_eq!(account_title(&m.accounts[0]), "Team Work");
        assert_eq!(account_title(&m.accounts[2]), "account2");
        m.inline = true;
        assert!(m.rows().iter().any(|r| r.label == "Team Work"));
        m.go(Page::Accounts);
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
        assert!(lines.iter().any(|l| l == "Display name: Team Work"));
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
        assert!(line("d-busy-codex").contains(crate::session_list::spinner(0)));
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
        assert!(text.contains("Open in another terminal"));
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
            "Focus Session"
        );
        let mut old = tmux_claude(json!(true));
        old["capabilities"]
            .as_object_mut()
            .unwrap()
            .remove("add_to_herdr");
        assert_eq!(
            crate::session_list::actions(&old, true)[0].0.label,
            "Focus Session"
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
        assert_eq!(m.options.selected, 1, "defaults to the focused Space");
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
                m.options.selected, 0,
                "remembered Space beats the focused one"
            );
            m.activate();
            assert_eq!(m.dialog.as_ref().unwrap().title, "Start Session");
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
    fn session_details_show_continuations_forks_and_native_kind_only_when_known() {
        let mut s = claude_session("running");
        s["previous_conversation_ids"] = json!(["old-1", "old-2"]);
        s["forked_from_conversation_id"] = json!("parent");
        s["native"] = json!({"kind":"bg","job_id":"job-7"});
        let text = Manager::session_detail_lines(&s).join("\n");
        for line in [
            "Earlier native session IDs: old-1, old-2",
            "Forked from: parent",
            "Native kind: background",
            "Native job ID: job-7",
        ] {
            assert!(text.contains(line), "{line}");
        }
        s["native"] = json!({"kind":"interactive"});
        let text = Manager::session_detail_lines(&s).join("\n");
        assert!(text.contains("Native kind: interactive") && !text.contains("job ID"));
        s["native"] = json!({});
        assert!(
            Manager::session_detail_lines(&s)
                .join("\n")
                .contains("Native kind: unknown")
        );
        let plain = Manager::session_detail_lines(&claude_session("running")).join("\n");
        assert!(
            !plain.contains("Earlier")
                && !plain.contains("Forked")
                && !plain.contains("Native kind")
        );
        let mut codex =
            json!({"id":"x","previous_conversation_ids":["a"],"forked_from_conversation_id":"b"});
        let text = Manager::session_detail_lines(&codex).join("\n");
        assert!(text.contains("Earlier native session IDs: a") && text.contains("Forked from: b"));
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
}
