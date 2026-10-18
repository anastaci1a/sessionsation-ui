//! Public-contract management reducer. Rendering and selection perform no I/O.
use crate::{
    client::Account,
    dialog::{self, Dialog, Field, Palette, Rule},
    render::{Canvas, Cell},
    usage::{AccountsLayout, Usage, short_name},
    widgets::{Options, Row, clip, hint_lines},
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
    Session(String),
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
    Operations,
    Details(String),
    Resume(String),
    Launches(&'static str),
    Launch(&'static str, String),
    Remove(String),
    Delete(String),
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
    Rename(Value),
    Confirm(Vec<String>),
    Progress,
    Operation(String),
}
pub struct Manager {
    pub page: Page,
    pub options: Options,
    pub accounts: Vec<Account>,
    pub sessions: Vec<Value>,
    pub operations: Vec<Value>,
    pub launches: BTreeMap<&'static str, Vec<Value>>,
    pub capabilities: Value,
    pub dialog: Option<Dialog>,
    pub palette: Palette,
    pub note: String,
    pub inline: bool,
    pub inventory_known: bool,
    pub sessions_known: bool,
    form: Form,
    sequence: u64,
    pending: BTreeMap<u64, Purpose>,
    readiness: BTreeMap<String, Value>,
    pub active_operation: Option<String>,
    foreground: Option<u64>,
    observe_after: Option<std::time::Instant>,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            page: Page::Home,
            options: Options::default(),
            accounts: vec![],
            sessions: vec![],
            operations: vec![],
            launches: BTreeMap::new(),
            capabilities: json!({}),
            dialog: None,
            palette: Palette::default(),
            note: String::new(),
            inline: false,
            inventory_known: false,
            sessions_known: false,
            form: Form::None,
            sequence: 0,
            pending: BTreeMap::new(),
            readiness: BTreeMap::new(),
            active_operation: None,
            foreground: None,
            observe_after: None,
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
        self.sessions.iter().find(|s| id(s) == key)
    }
    fn provider_cap(&self, key: &str) -> bool {
        self.capabilities["providers"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p["id"] == "codex" && p[key] == true))
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
    pub fn inventory(&mut self, accounts: Vec<Account>) {
        let selected = self.selected_account();
        self.accounts = accounts;
        self.inventory_known = true;
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
    pub fn title(&self) -> &str {
        match self.page {
            Page::Home => "",
            Page::Accounts => "Manage Accounts",
            Page::Account(_) => "Account",
            Page::Sessions => "Manage Sessions",
            Page::Session(_) => "Session",
            Page::Operations => "Operations",
            Page::Launch(..) => "Launch progress",
            Page::Operation(_) => "Operation",
        }
    }
    fn go(&mut self, page: Page) {
        self.dialog = None;
        self.form = Form::None;
        self.foreground = None;
        self.page = page;
        self.options = Options::default();
        self.options.selected = self.rows().iter().position(|r| r.enabled).unwrap_or(0);
        self.note.clear();
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
                    rows.extend(self.accounts.iter().map(|a| Row::yes(short_name(&a.id))));
                }
                rows
            }
            Page::Accounts => {
                std::iter::once(Row::yes("<add account>"))
                    .chain(self.accounts.iter().map(|a| {
                        Row::yes(short_name(&a.id)).with_preview(Usage::account(a).status)
                    }))
                    .collect()
            }
            Page::Account(key) => {
                let a = self.account(key);
                let removed =
                    a.is_some_and(|a| a.details.get("membership").is_some_and(|v| v == "removed"));
                let mut rows = vec![
                    Row::yes("Usage details"),
                    Row::gate("Limits", !removed && self.provider_cap("account_limits")),
                    Row::separator(),
                    Row::gate("Log in", !removed && self.provider_cap("enrollment")),
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
                    Row::gate("Delete account", self.provider_cap("account_deletion")),
                ];
                if !a.is_some_and(|a| a.details.get("auth_required").is_some_and(|v| v == true)) {
                    rows.remove(3);
                }
                rows
            }
            Page::Sessions => crate::session_list::rows(&self.sessions, self.sessions_known),
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
                    Row::yes("Progress"),
                    Row::gate(
                        "Transfer operation",
                        receipt.is_some_and(|r| r["operation_id"].is_string()),
                    ),
                ]
            }
            Page::Operations => self
                .operations
                .iter()
                .map(|o| {
                    Row::yes(format!(
                        "{}: {}",
                        o["kind"].as_str().unwrap_or("operation"),
                        o["phase"].as_str().unwrap_or("unknown")
                    ))
                    .with_selected_preview(id(o))
                })
                .chain(self.launches.iter().flat_map(|(kind, rs)| {
                    rs.iter().map(|r| {
                        Row::yes(format!(
                            "{}: {}",
                            if *kind == "launch" {
                                "New session"
                            } else {
                                "Resume"
                            },
                            r["phase"].as_str().unwrap_or("unknown")
                        ))
                        .with_preview(r["request"]["name"].as_str().unwrap_or(""))
                    })
                }))
                .collect(),
            Page::Operation(key) => {
                let o = self
                    .operations
                    .iter()
                    .find(|o| id(o) == key)
                    .cloned()
                    .unwrap_or(Value::Null);
                let phase = o["phase"].as_str().unwrap_or("");
                vec![
                    Row::yes("Progress"),
                    Row::gate("Respond yes", phase.starts_with("awaiting")),
                    Row::gate("Respond no", phase.starts_with("awaiting")),
                    Row::separator(),
                    Row::gate(
                        "Cancel operation",
                        !terminal(phase) && o["can_cancel"] != false,
                    ),
                    Row::gate("Recover", matches!(phase, "failed" | "cancelled")),
                ]
            }
        }
    }
    fn operation_keys(&self) -> Vec<(String, String)> {
        self.operations
            .iter()
            .map(|o| ("operation".into(), id(o).into()))
            .chain(self.launches.iter().flat_map(|(kind, rs)| {
                rs.iter().map(move |r| {
                    (
                        (*kind).into(),
                        r["request"]["request_id"].as_str().unwrap_or("").into(),
                    )
                })
            }))
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
        if matches!(self.page, Page::Sessions | Page::Session(_))
            && !self.pending.values().any(|p| matches!(p, Purpose::State))
        {
            out.extend(self.request(strings(&["state"]), Purpose::State));
        }
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
        self.dialog = Some(Dialog::new(title, body, buttons));
        self.form = form;
    }
    fn mutate(&mut self, args: Vec<String>) -> Vec<Intent> {
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
        let mut args = Self::target(&s, "switch");
        args.extend(strings(&["--to", account]));
        self.confirm("Switch account",vec![format!("Move {} to {} while retaining conversation history? The backend waits for safe idle; it never steers.",s["name"].as_str().unwrap_or("session"),account)],args);
    }
    fn dismiss(&mut self) {
        if matches!(self.form, Form::Progress | Form::Operation(_)) {
            self.go(Page::Home);
        }
        self.dialog = None;
        self.form = Form::None;
        self.foreground = None;
    }
    fn confirm(&mut self, title: &str, body: Vec<String>, args: Vec<String>) {
        self.show(title, body, &["Confirm", "Cancel"], Form::Confirm(args));
    }
    pub fn key(&mut self, key: KeyEvent) -> Vec<Intent> {
        if let Some(d) = &mut self.dialog {
            match d.key(key) {
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
            KeyCode::Up | KeyCode::Down => {
                self.options
                    .step(&self.rows(), false, key.code == KeyCode::Up);
                vec![]
            }
            KeyCode::Left => {
                self.options.back = true;
                vec![]
            }
            KeyCode::Right => {
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
        match self.page.clone() {
            Page::Home => return vec![Intent::Exit],
            Page::Account(key) => self.account_back(&key),
            Page::Session(key) => {
                self.go(Page::Sessions);
                self.options.selected = self
                    .sessions
                    .iter()
                    .position(|s| id(s) == key)
                    .map(|i| i + 1)
                    .unwrap_or(0)
            }
            Page::Operation(_) | Page::Launch(..) => self.go(Page::Operations),
            Page::Operations => self.go(Page::Sessions),
            _ => self.go(Page::Home),
        }
        vec![]
    }
    fn add(&mut self) {
        self.show(
            "Add Account",
            vec!["Codex account name: 1–12 letters, digits, hyphens or underscores.".into()],
            &["Add account", "Cancel"],
            Form::Add,
        );
        self.dialog
            .as_mut()
            .unwrap()
            .fields
            .push(Field::new("Name", "").validated(Rule::AccountName));
    }
    fn activate(&mut self) -> Vec<Intent> {
        if !self
            .rows()
            .get(self.options.selected)
            .is_some_and(|r| r.enabled)
        {
            return vec![];
        }
        let index = self.options.selected;
        match self.page.clone() {
            Page::Home => match index {
                0 => return vec![Intent::New],
                1 => {
                    self.go(Page::Sessions);
                    return self.refresh();
                }
                2 if self.inline => self.add(),
                2 => self.go(Page::Accounts),
                4 => return vec![Intent::Settings],
                i if i >= 5 => {
                    if let Some(a) = self.accounts.get(i - 5) {
                        self.go(Page::Account(a.id.clone()));
                    }
                }
                _ => {}
            },
            Page::Accounts => {
                if index == 0 {
                    self.add()
                } else if let Some(a) = self.accounts.get(index - 1) {
                    self.go(Page::Account(a.id.clone()));
                }
            }
            Page::Account(key) => {
                let Some(a) = self.account(&key).cloned() else {
                    self.note = "Account is no longer available".into();
                    return vec![];
                };
                let index =
                    if a.details.get("auth_required").is_some_and(|v| v == true) || index < 3 {
                        index
                    } else {
                        index + 1
                    };
                match index {
                    0 => {
                        let mut body = vec![a.id.clone(), Usage::account(&a).status];
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
                    1 => {
                        return self.preview(
                            "Account limits",
                            strings(&["account", "limits", &key]),
                            Purpose::Limits(key),
                        );
                    }
                    3 => {
                        self.show(
                            "Log in",
                            vec![format!("Start provider login for {key}?")],
                            &["Log in", "Cancel"],
                            Form::Login(key),
                        );
                    }
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
                if index == 0 {
                    return vec![Intent::New];
                }
                let offset = if self.sessions_known { 1 } else { 2 };
                if let Some(session) = index.checked_sub(offset).and_then(|i| self.sessions.get(i))
                {
                    self.go(Page::Session(id(session).into()));
                } else {
                    self.go(Page::Operations);
                }
                return self.refresh();
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
                    crate::session_list::Action::Focus => return vec![Intent::Focus(s)],
                    crate::session_list::Action::Details => {
                        return self.preview(
                            "Session details",
                            Self::target(&s, "session"),
                            Purpose::Details(key),
                        );
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
                                .validated(Rule::Name),
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
                if let Some(o) = self.operations.get(index) {
                    self.go(Page::Operation(id(o).into()));
                } else if let Some((kind, key)) = self
                    .launches
                    .iter()
                    .flat_map(|(kind, rs)| {
                        rs.iter().map(move |r| {
                            (
                                *kind,
                                r["request"]["request_id"].as_str().unwrap_or("").to_owned(),
                            )
                        })
                    })
                    .nth(index.saturating_sub(self.operations.len()))
                {
                    self.go(Page::Launch(kind, key));
                }
                return self.refresh();
            }
            Page::Launch(kind, key) => {
                if index == 0 {
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
                    self.go(Page::Operation(op));
                    return self.refresh();
                }
            }
            Page::Operation(key) => {
                let Some(o) = self.operations.iter().find(|o| id(o) == key).cloned() else {
                    return vec![];
                };
                match index {
                    0 => self.show(
                        "Operation progress",
                        vec![
                            o["phase"].as_str().unwrap_or("unknown").into(),
                            o["detail"].as_str().unwrap_or("").into(),
                        ],
                        &["Continue in background"],
                        Form::Operation(key.clone()),
                    ),
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
            Form::None | Form::Progress | Form::Operation(_) => {
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
                    || name.len() > 12
                    || !name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                {
                    self.dialog.as_mut().unwrap().body =
                        vec!["Use 1–12 letters, digits, hyphens or underscores.".into()];
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
                | Purpose::Details(_)
                | Purpose::Resume(_)
        ) && (self.foreground != Some(token) || !matches!(self.form, Form::Loading))
        {
            return vec![];
        }
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                self.note = e.clone();
                if matches!(self.form, Form::Loading) && self.foreground == Some(token) {
                    self.show("Could not load", vec![e.clone()], &["Close"], Form::None);
                }
                if matches!(purpose, Purpose::Mutation)
                    && matches!(self.form, Form::Progress)
                    && self.foreground == Some(token)
                {
                    self.show("Operation needs review",vec![e,"No automatic retry. Refresh and inspect the original operation before trying again.".into()],&["Close"],Form::None);
                }
                return vec![];
            }
        };
        match purpose {
            Purpose::State => {
                let operations_selected = self.page == Page::Sessions
                    && self.options.selected == self.rows().len().saturating_sub(1);
                let chosen = if self.page == Page::Sessions
                    && self.sessions_known
                    && self.options.selected >= 1
                {
                    self.sessions
                        .get(self.options.selected.saturating_sub(1))
                        .map(|s| id(s).to_owned())
                } else {
                    None
                };
                self.sessions = value["sessions"].as_array().cloned().unwrap_or_default();
                self.sessions_known = true;
                self.sessions.sort_by_key(|s| {
                    (
                        match s["availability"].as_str() {
                            Some("running") => 0,
                            Some("saved") => 1,
                            _ => 2,
                        },
                        s["name"].as_str().unwrap_or("").to_lowercase(),
                    )
                });
                if let Some(key) = chosen {
                    self.options.selected = self
                        .sessions
                        .iter()
                        .position(|s| id(s) == key)
                        .map(|i| i + 1)
                        .unwrap_or(0);
                }
                if operations_selected {
                    self.options.selected = self.rows().len().saturating_sub(1);
                }
                self.capabilities = value["capabilities"].clone();
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
                if self.page == Page::Launch(kind, key.clone()) {
                    let body = vec![
                        format!("Request: {key}"),
                        format!("Phase: {}", value["phase"].as_str().unwrap_or("unknown")),
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
                        o["phase"].as_str().unwrap_or("unknown").into(),
                        o["detail"].as_str().unwrap_or("").into(),
                    ];
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
            Purpose::Resume(key) => {
                if self.page == Page::Session(key) {
                    self.dialog = None;
                    self.form = Form::None;
                    self.foreground = None;
                    return vec![Intent::Resume(value)];
                }
            }
            Purpose::Details(key) => {
                if self.page != Page::Session(key) {
                    return vec![];
                }
                let body = vec![
                    format!("Name: {}", value["name"].as_str().unwrap_or("")),
                    format!("Account: {}", value["account_id"].as_str().unwrap_or("")),
                    format!(
                        "Directory: {}",
                        value["directory"].as_str().unwrap_or("unavailable")
                    ),
                    format!("Session: {}", id(&value)),
                    format!(
                        "Conversation: {}",
                        value["conversation_id"].as_str().unwrap_or("")
                    ),
                    format!("Generation: {}", value["generation"].as_str().unwrap_or("")),
                ];
                self.show("Session details", body, &["Close"], Form::None);
            }
            Purpose::Limits(key) => {
                if self.page != Page::Account(key.clone()) {
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
        let rows = self.rows();
        if self.page == Page::Home && self.inline {
            let l = layout.as_ref().unwrap();
            let longest = rows
                .iter()
                .take(5)
                .map(|r| unicode_width::UnicodeWidthStr::width(r.label.as_str()))
                .max()
                .unwrap_or(0);
            let top_left = canvas.width.saturating_sub(longest + 2) / 2;
            for (i, row) in rows.iter().take(5).enumerate() {
                row.paint(
                    canvas,
                    top_left,
                    3 + i,
                    self.options.selected == i && !self.options.back,
                    self.options.back,
                );
            }
            l.paint(
                canvas,
                &self.accounts,
                9,
                self.options.selected.checked_sub(5),
                self.options.back,
            );
            self.options.paint_back(canvas, top_left, 3, "Close");
        } else {
            let selected = self.selected_account();
            let usage = if self.page == Page::Accounts {
                selected
                    .as_ref()
                    .and_then(|key| self.account(key))
                    .map(Usage::account)
            } else {
                None
            };
            let preview_height = usage.as_ref().map(Usage::height).unwrap_or(0);
            let bottom = if preview_height > 0 {
                footer.saturating_sub(preview_height + 2)
            } else {
                footer.saturating_sub(3)
            };
            let detached_id = self.page == Page::Operations
                && !self.options.back
                && self.note.is_empty()
                && rows.get(self.options.selected).is_some_and(|row| {
                    row.preview_selected_only
                        && row
                            .preview_position(canvas.width, Options::left(canvas.width, &rows))
                            .is_none()
                        && row.preview.as_deref().is_some_and(|p| {
                            unicode_width::UnicodeWidthStr::width(p) + 2 <= canvas.width
                        })
                })
                && controls_start.saturating_sub(4) >= 4;
            let bottom = if detached_id {
                bottom.min(controls_start.saturating_sub(4))
            } else {
                bottom
            };
            let (left, capacity) = if self.page == Page::Sessions {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                crate::session_list::paint(
                    canvas,
                    &mut self.options,
                    &self.sessions,
                    self.sessions_known,
                    bottom,
                    now,
                )
            } else {
                self.options.paint(canvas, &rows, 3, bottom)
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
            if detached_id
                && !self.options.back
                && let Some(row) = rows.get(self.options.selected)
                && row.preview_selected_only
                && row.preview_position(canvas.width, left).is_none()
                && let Some(preview) = row.preview.as_deref()
            {
                let width = unicode_width::UnicodeWidthStr::width(preview);
                let y = if rows.len() > capacity {
                    bottom + 2
                } else {
                    3 + rows.len().saturating_sub(self.options.scroll).min(capacity) + 1
                };
                if width + 2 <= canvas.width && y + 1 < controls_start && self.note.is_empty() {
                    canvas.text(
                        (canvas.width - width) / 2,
                        y,
                        preview,
                        Cell {
                            fg: [188; 3],
                            ..Cell::default()
                        },
                        false,
                    );
                }
            }
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
                    u.paint(
                        canvas,
                        canvas.width.saturating_sub(u.width(bar, u.digits())) / 2,
                        footer - preview_height - 1,
                        bar,
                        u.digits(),
                    );
                }
            }
            self.options
                .paint_range(canvas, rows.len(), capacity, bottom);
        }
        if !self.note.is_empty() {
            canvas.text(
                1,
                if self.page == Page::Accounts {
                    footer.saturating_sub(5)
                } else {
                    footer.saturating_sub(1)
                },
                &clip(&self.note, canvas.width.saturating_sub(2)),
                Cell::default(),
                false,
            );
        }
        crate::widgets::paint_hints(canvas, &hints);
        if let Some(d) = &mut self.dialog {
            d.paint(canvas, &self.palette);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                assert_eq!(c.cells[6 * width + (width - a.len()) / 2].glyph, "1");
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
        let sent = m.submit(0, vec!["codex-account1".into()]);
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
        assert!(row(11).contains("5h"));
        assert!(row(12).contains("wkly"));
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
        let s =
            json!({"id":"stable","name":"Test","generation":"old","capabilities":{"stop":true}});
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
