//! Public-contract management reducer. Rendering and selection perform no I/O.
use crate::{
    client::Account,
    dialog::{self, Dialog, Field, Palette, Rule},
    render::{Canvas, Cell},
    usage::{AccountsLayout, Usage, short_name},
    widgets::{Options, Row, hint_lines},
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
    Exit,
}
#[derive(Clone, Debug)]
enum Purpose {
    State,
    Archives,
    Operations,
    Resume(String),
    Spaces(Value),
    Launches(&'static str),
    Launch(&'static str, String),
    Remove(String),
    Delete(String),
    DeleteAvailability(String),
    Limits(String),
    Mutation,
    Observe(String),
}
#[derive(Clone, Debug)]
enum Form {
    None,
    Loading,
    Add,
    Login(String),
    Limits(String),
    Remove(Value),
    Delete(Value),
    ResumeWarning(Value),
    Rename(Value),
    SessionMutation(Value, &'static str),
    SessionDetails(String),
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
}
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
        }
    }
}
fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).into()).collect()
}
fn id(v: &Value) -> &str {
    v["id"].as_str().unwrap_or("")
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
    fn provider_cap(&self, key: &str) -> bool {
        self.capabilities["providers"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p["id"] == "codex" && p[key] == true))
    }
    fn delete_account_row(&self, key: &str) -> Row {
        let preview = self.deletion_previews.get(key);
        let reason = if !self.provider_cap("account_deletion") {
            Some("unsupported")
        } else if preview.is_some_and(|p| p["owner_upgrade_required"] == true) {
            Some("owner update needed")
        } else if preview.is_some_and(|p| p["can_delete"] == false) {
            Some(
                if preview.is_some_and(|p| {
                    p["affected_sessions"]
                        .as_array()
                        .is_some_and(|ss| ss.iter().any(|s| s["running"] == true))
                }) {
                    "active sessions"
                } else {
                    "unavailable"
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
            Page::Accounts if self.options.selected > 0 => self
                .accounts
                .get(self.options.selected - 1)
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
        let mut body = vec![a.id.clone(), Usage::account(a).status];
        for (window, label) in [("short", "Short window"), ("weekly", "Weekly")] {
            if !a.usage[window].is_null() {
                body.push(format!(
                    "{label} ({} minutes): {}% used; resets at {} (Unix seconds)",
                    a.usage[window]["duration_minutes"],
                    a.usage[window]["used_percent"],
                    a.usage[window]["resets_at"]
                ));
            }
        }
        self.show("Usage details", body, &["Close"], Form::None);
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
            "Login required" if self.provider_cap("enrollment") => self.show_login(&key),
            "Deregistered" => self.show(
                "Restore account",
                vec![format!("Restore {key} to account selection?")],
                &["Restore", "Cancel"],
                Form::Confirm(strings(&["account", "restore", &key])),
            ),
            _ if !usage.status_only() && self.provider_cap("account_limits") => {
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
        self.inventory_known = true;
        self.validate_dialog_names();
        if let Some(id) = selected {
            if let Some(index) = self.accounts.iter().position(|a| a.id == id) {
                match self.page {
                    Page::Home if self.inline => self.options.selected = 5 + index,
                    Page::Accounts => self.options.selected = 1 + index,
                    _ => {}
                }
            } else if matches!(self.page, Page::Home | Page::Accounts) {
                self.options.selected = 0;
            }
        }
    }
    fn validate_dialog_names(&mut self) {
        let excluded = match &self.form {
            Form::Add => self
                .accounts
                .iter()
                .filter_map(|a| a.id.strip_prefix("codex:").map(str::to_owned))
                .collect(),
            Form::Rename(target) => self
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
            Page::Session(key) => {
                return format!(
                    "Session: {}",
                    self.session(key)
                        .and_then(|s| s["name"].as_str())
                        .unwrap_or("Unnamed")
                );
            }
            Page::Spaces(_) => "Choose Space",
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
            Some(Focus::Account(key)) => self
                .accounts
                .iter()
                .position(|a| a.id == key)
                .map(|i| i + if self.page == Page::Home { 5 } else { 1 }),
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
        let index = self.accounts.iter().position(|a| a.id == key);
        self.go(if self.inline {
            Page::Home
        } else {
            Page::Accounts
        });
        self.options.selected = index
            .map(|i| i + if self.inline { 5 } else { 1 })
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
                            .map(|a| Row::yes(short_name(&a.id)).account()),
                    );
                }
                rows
            }
            Page::Accounts => {
                let column = self
                    .accounts
                    .iter()
                    .map(|a| unicode_width::UnicodeWidthStr::width(short_name(&a.id).as_str()))
                    .max()
                    .unwrap_or(0)
                    + 4;
                std::iter::once(Row::yes("[add account]"))
                    .chain(self.accounts.iter().map(|a| {
                        let enabled =
                            a.details.get("enabled").and_then(Value::as_bool) == Some(true);
                        Row::yes(short_name(&a.id)).with_column_preview(
                            if enabled {
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
                    Row::yes("Usage details"),
                    Row::gate("Limits", !removed && self.provider_cap("account_limits")),
                    Row::separator(),
                    Row::gate("Login", !removed && self.provider_cap("enrollment")),
                    Row::gate(
                        if a.is_some_and(|a| a.details.get("enabled").is_some_and(|v| v == false)) {
                            "Enable"
                        } else {
                            "Disable"
                        },
                        !removed,
                    ),
                    Row::separator(),
                    Row::yes(if removed {
                        "Restore account"
                    } else {
                        "Deregister account"
                    }),
                    self.delete_account_row(key),
                ];
                if !a.is_some_and(|a| a.details.get("auth_required").is_some_and(|v| v == true)) {
                    rows.remove(3);
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
                })
                .collect(),
            Page::Spaces(_) => self.space_choices["spaces"]
                .as_array()
                .map(|spaces| {
                    spaces
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
                        .collect()
                })
                .unwrap_or_default(),
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
            && self.provider_cap("account_deletion")
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
            || matches!(self.form, Form::Operation(_))
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
                .filter(|s| s["availability"] == "running")
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
    pub fn refresh_live(&mut self) -> Vec<Intent> {
        if matches!(
            self.page,
            Page::Operations | Page::Operation(_) | Page::Launch(..)
        ) || matches!(self.form, Form::Operation(_))
        {
            self.refresh()
        } else {
            self.refresh_sessions()
        }
    }
    fn session_detail_lines(value: &Value) -> Vec<String> {
        vec![
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
        ]
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
        self.dialog = Some(dialog);
        self.form = form;
    }
    fn mutate(&mut self, args: Vec<String>) -> Vec<Intent> {
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
        self.dialog = Some(Dialog::progress("Operation progress"));
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
        self.show(
            "Add Account",
            vec!["Codex account name: 1–24 letters, digits, hyphens or underscores.".into()],
            &["Add account", "Cancel"],
            Form::Add,
        );
        self.dialog
            .as_mut()
            .unwrap()
            .fields
            .push(Field::new("Name", "").validated(Rule::AccountName));
        self.validate_dialog_names();
    }
    fn activate(&mut self) -> Vec<Intent> {
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
                } else if let Some(a) = self.accounts.get(index - 1) {
                    self.enter(Page::Account(a.id.clone()));
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
                    if a.details.get("auth_required").is_some_and(|v| v == true) || index < 3 {
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
                    3 => self.show_login(&key),
                    4 => {
                        if a.details.get("enabled").is_some_and(|v| v == false) {
                            return self.mutate(strings(&["account", "enable", &key]));
                        } else {
                            self.confirm("Disable account",vec![format!("Disable {key} for future selection? Existing agents keep running.")],strings(&["account","disable",&key]));
                        }
                    }
                    6 => {
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
                    7 => {
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
                match action.unwrap_or(crate::session_list::Action::Unavailable) {
                    crate::session_list::Action::Archive
                    | crate::session_list::Action::Delete
                    | crate::session_list::Action::Restore => {
                        let (command, title, text) = match action.unwrap() {
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
                        self.show(
                            "Rename session",
                            vec!["Keep the same session and conversation identities.".into()],
                            &["Rename", "Cancel"],
                            Form::Rename(s.clone()),
                        );
                        self.dialog.as_mut().unwrap().fields.push(
                            Field::new("Name", s["name"].as_str().unwrap_or(""))
                                .validated(Rule::Rename(s["name"].as_str().unwrap_or("").into())),
                        );
                        self.validate_dialog_names();
                    }

                    crate::session_list::Action::MoveSpace => {
                        return self.preview(
                            "Choose Space",
                            Self::target(&s, "spaces"),
                            Purpose::Spaces(s),
                        );
                    }
                    crate::session_list::Action::Switch => return vec![Intent::Switch(s)],
                    crate::session_list::Action::Resume => {
                        return self.preview(
                            "Resume session",
                            Self::target(&s, "session"),
                            Purpose::Resume(key),
                        );
                    }
                    crate::session_list::Action::Restart | crate::session_list::Action::Stop => {
                        let command = if action == Some(crate::session_list::Action::Restart) {
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
    fn submit(&mut self, button: usize, values: Vec<String>) -> Vec<Intent> {
        if button != 0 {
            self.dialog = None;
            self.form = Form::None;
            return vec![];
        }
        let form = self.form.clone();
        let value = |i: usize| values.get(i).map(String::as_str).unwrap_or("");
        let args = match form {
            Form::None | Form::Progress | Form::Operation(_) | Form::SessionDetails(_) => {
                self.dismiss();
                return vec![];
            }
            Form::Loading => {
                self.dismiss();
                return vec![];
            }
            Form::Confirm(args) => args,
            Form::Add => {
                let name = value(0);
                if name.is_empty()
                    || name.len() > 24
                    || !name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                {
                    self.dialog.as_mut().unwrap().body =
                        vec!["Use 1–24 letters, digits, hyphens or underscores.".into()];
                    return vec![];
                }
                strings(&["account", "add", &format!("codex:{name}")])
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
                return vec![Intent::Resume(session)];
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
        if matches!(
            purpose,
            Purpose::Remove(_)
                | Purpose::Delete(_)
                | Purpose::Limits(_)
                | Purpose::Resume(_)
                | Purpose::Spaces(_)
        ) && (self.foreground != Some(token) || !matches!(self.form, Form::Loading))
        {
            return vec![];
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
            Purpose::DeleteAvailability(key) => {
                self.deletion_previews.insert(key, value);
            }
            Purpose::Archives => {
                self.archives = value.as_array().cloned().unwrap_or_default();
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
                }
            }
            Purpose::Resume(key) => {
                if self.page == Page::Session(key) {
                    self.dialog = None;
                    self.form = Form::None;
                    self.foreground = None;
                    if value["resume_block_reason"] == "unfinished_history" {
                        self.show("Unfinished conversation", vec!["This conversation ended during a turn. Start from its saved history? Review unfinished work before continuing; the interrupted request will not be replayed automatically.".into()], &["Start Session", "Cancel"], Form::ResumeWarning(value));
                    } else {
                        return vec![Intent::Resume(value)];
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
                let mut body=vec![format!("Delete {expected}'s local account files and credentials?"),"All saved and archived conversations will be preserved outside the account directory. The upstream provider account is unchanged.".into(),"After confirmation, the CLI checks active sessions, preserves history and deletes local account data. Progress can continue in the background.".into()];
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
                if value["login"].is_object() {
                    if matches!(self.form, Form::Progress) && self.foreground == Some(token) {
                        let login = &value["login"];
                        let url = login["verificationUri"]
                            .as_str()
                            .or_else(|| login["verificationUrl"].as_str())
                            .unwrap_or("See provider login response");
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
                    self.show(
                        "Saved",
                        vec!["The backend acknowledged this change.".into()],
                        &["Close"],
                        Form::None,
                    );
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
        self.inline = self.inventory_known && layout.is_some();
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
        m.options.selected = 2;
        m.options.scroll = 1;
        m.key(enter);
        assert_eq!(m.page, Page::Account("codex:account2".into()));
        m.accounts.reverse();
        m.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        m.key(enter);
        assert_eq!(m.page, Page::Accounts);
        assert_eq!(m.options.selected, 1);
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
        let sent = m.activate();
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
        assert_eq!(rows[1].preview.as_deref(), Some("Disabled"));
        assert_eq!(rows[1].preview_fg, crate::widgets::MUTED);
        assert_eq!(rows[2].preview.as_deref(), Some("Enabled*"));
        m.options.selected = 1;
        let mut c = Canvas::new(40, 18);
        m.paint(&mut c, 17);
        for y in 6..16 {
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
            rows[1].preview_position(60, 5),
            rows[2].preview_position(60, 5)
        );
        assert_eq!(rows[1].preview.as_deref(), Some("Enabled"));
        assert!(rows[1].preview_accent);
        assert!(!rows[2].preview_accent);
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
        m.options.selected = 2;
        let mut canvas = Canvas::new(60, 16);
        m.paint(&mut canvas, 14);
        let row = |n: usize| {
            canvas.cells[n * 60..(n + 1) * 60]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
        };
        assert!(row(6).trim().is_empty());
        assert!(row(7).contains("5h"));
        assert!(row(8).contains("wk"));
        assert!(m.rows().iter().all(|r| !r.account));
        assert!(row(5).contains("●"));
        assert!(!row(5).contains("▶"));
        let name_x = row(5).find("account2").unwrap();
        assert_eq!(canvas.cells[5 * 60 + name_x].fg, [238; 3]);
        for y in [7, 8] {
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
        m.options.selected = 3;
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
        assert!(m.rows()[2].separator && m.rows()[4].separator);
        m.options.selected = 1;
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
        m.activate();
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
}
