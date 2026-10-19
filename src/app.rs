use crate::{
    client::{Account, Workspace},
    directory::Listing,
    draft::{Directory, Draft, Space, valid_name},
    preferences::{Mode, Preferences},
    render::{Accent, Canvas, Cell, Field},
    widgets::{NAME_MAX_CHARS, Options, Row, TextInput, UNSET},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Draft,
    Global,
    Space(String),
}

fn resume_invalid(error: &str) -> bool {
    [
        "INVALID_",
        "SESSION_ALREADY_RUNNING",
        "SESSION_SOURCE_CHANGED",
        "SESSION_NOT_FOUND",
        "SAVED_DIRECTORY_UNAVAILABLE",
        "SESSION_NAME_EXISTS",
        "ACCOUNT_UNAVAILABLE",
        "WORKSPACE_NOT_FOUND",
        "WORKSPACE_NAME_EXISTS",
        "SESSION_OPERATION_IN_PROGRESS",
    ]
    .iter()
    .any(|code| error.starts_with(code))
}
/// Receipt-read freshness errors are pending observations, not failed mutations.
pub fn reconciliation_message(error: &str) -> String {
    if error.starts_with("SESSION_STATE_UNVERIFIED") || error.starts_with("STALE_TARGET") {
        "Checking session state…".into()
    } else {
        format!(
            "{}; checking original request",
            crate::messages::describe(error)
        )
    }
}
fn resume_explanation(error: &str) -> String {
    let reason =
        if error.starts_with("SESSION_STATE_UNVERIFIED") || error.starts_with("STALE_TARGET") {
            "recheck needed"
        } else if error.starts_with("SESSION_SOURCE_CHANGED") {
            "source changed"
        } else if error.starts_with("SESSION_OPERATION_IN_PROGRESS") {
            "operation active"
        } else if error.starts_with("SESSION_ALREADY_RUNNING") {
            "already running"
        } else if error.starts_with("SESSION_NAME_EXISTS") {
            "name taken"
        } else if error.starts_with("ACCOUNT_UNAVAILABLE") {
            "account unavailable"
        } else if error.starts_with("SAVED_DIRECTORY_UNAVAILABLE") {
            "missing directory"
        } else if error.starts_with("WORKSPACE_") {
            "invalid Space"
        } else if error.starts_with("UNFINISHED_HISTORY_") {
            "confirmation needed"
        } else if error.starts_with("SESSION_NOT_FOUND") {
            "not found"
        } else {
            "verification needed"
        };
    format!("({reason})")
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Management,
    Summary,
    Config,
    Roots,
    Accounts,
    Spaces,
    Name,
    SpaceName,
    Directory(Target),
    NewDirectory(Target, PathBuf),
    Launching,
}
#[derive(Clone, Debug)]
pub enum Effect {
    Inspect(u64, String),
    Validate(u64, Value),
    Start(Value),
    Save,
    Exit,
    Api(u64, Vec<String>),
    FocusSession(Value, bool),
}
enum StartConfirmation {
    SharedDirectory(PathBuf),
    UnfinishedHistory,
}
#[derive(Clone)]
struct Saved {
    page: Page,
    options: Options,
    text: String,
    listing: Listing,
    listing_pending: bool,
    popup_size: Option<(Page, crate::popup::Size)>,
}
pub struct App {
    navigation: crate::widgets::Navigation,
    pub manager: crate::management::Manager,
    pub page: Page,
    pub options: Options,
    pub input: TextInput,
    pub listing: Listing,
    listing_pending: bool,
    pub draft: Draft,
    pub prefs: Preferences,
    pub accounts: Vec<Account>,
    pub spaces: Vec<Workspace>,
    pub initial: PathBuf,
    pub instance: String,
    pub host: Value,
    pub error: String,
    pub terminal_background: [u8; 3],
    pub pending: bool,
    pub request: Option<Value>,
    pub revision: u64,
    pub exit: bool,
    pub success: bool,
    pub launch_supported: bool,
    pub launch_background: bool,
    pub launch_dialog: bool,
    start_confirmation: Option<(crate::dialog::Dialog, StartConfirmation)>,
    pub resume: Option<Value>,
    resume_validation: Option<String>,
    switch: Option<Value>,
    new_draft: Option<(Draft, bool, bool)>,
    stack: Vec<Saved>,
    history: BTreeMap<PathBuf, usize>,
    auto_account: bool,
    auto_space: bool,
    pub dirty: bool,
    pending_child: Option<PathBuf>,
    popup_size: Option<(Page, crate::popup::Size)>,
    status: crate::status::Status,
}
impl App {
    pub fn new(
        prefs: Preferences,
        initial: PathBuf,
        instance: String,
        host: Value,
        space: Option<String>,
    ) -> Self {
        let draft = Draft {
            space: space.map(|id| Space::Existing {
                id,
                label: String::new(),
            }),
            ..Draft::default()
        };
        Self {
            navigation: crate::widgets::Navigation::default(),
            manager: crate::management::Manager::default(),
            page: Page::Home,
            options: Options::default(),
            input: TextInput::default(),
            listing: Listing::default(),
            listing_pending: false,
            draft,
            prefs,
            accounts: vec![],
            spaces: vec![],
            initial,
            instance,
            host,
            error: String::new(),
            terminal_background: [16; 3],
            pending: false,
            request: None,
            revision: 0,
            exit: false,
            success: false,
            launch_supported: false,
            launch_background: false,
            launch_dialog: false,
            start_confirmation: None,
            resume: None,
            resume_validation: None,
            switch: None,
            new_draft: None,
            stack: vec![],
            history: BTreeMap::new(),
            auto_account: true,
            auto_space: true,
            dirty: true,
            pending_child: None,
            popup_size: None,
            status: crate::status::Status::default(),
        }
    }
    pub fn management(&mut self, intents: Vec<crate::management::Intent>) -> Vec<Effect> {
        let mut effects = vec![];
        for intent in intents {
            use crate::management::Intent;
            match intent {
                Intent::Api(id, args) => effects.push(Effect::Api(id, args)),
                Intent::New => {
                    if self.request.is_some() && !self.success {
                        self.manager.note="A launch is still in progress; inspect Operations before starting another.".into();
                    } else {
                        self.restore_new_draft();
                        self.request = None;
                        self.success = false;
                        effects.extend(self.push(Page::Summary, String::new()));
                    }
                }
                Intent::Settings => effects.extend(self.push(Page::Config, String::new())),
                Intent::Focus(session) => effects.push(Effect::FocusSession(
                    session,
                    self.manager.capabilities["attachment_lookup"]["version"] == 1,
                )),
                Intent::Exit => {
                    self.exit = true;
                    effects.push(Effect::Exit);
                }
                Intent::Switch(session) => {
                    self.switch = Some(session);
                    effects.extend(self.push(Page::Accounts, String::new()));
                }
                Intent::Resume(session) => {
                    if self.request.is_some() && !self.success {
                        self.manager.note =
                            "Inspect the pending launch in Operations first.".into();
                        continue;
                    }
                    self.restore_new_draft();
                    self.new_draft = Some((self.draft.clone(), self.auto_account, self.auto_space));
                    let original = &session["original_host"];
                    let preferred = if original["session"] == self.host["session"]
                        && original["socket"] == self.host["socket"]
                    {
                        self.spaces.iter().find(|s| {
                            s.workspace_id == original["workspace_id"].as_str().unwrap_or("")
                        })
                    } else {
                        None
                    };
                    let space = preferred
                        .or_else(|| {
                            let mut selected = self.spaces.iter().filter(|s| s.focused);
                            selected.next().filter(|_| selected.next().is_none())
                        })
                        .map(|s| Space::Existing {
                            id: s.workspace_id.clone(),
                            label: s.label.clone(),
                        });
                    self.draft = Draft {
                        account: session["account_id"].as_str().map(str::to_owned),
                        space,
                        name: session["name"].as_str().unwrap_or("").into(),
                        directory: session["directory"]
                            .as_str()
                            .map(|p| Directory::Existing(p.into()))
                            .unwrap_or_default(),
                        revision: self.draft.revision + 1,
                        ..Draft::default()
                    };
                    self.resume = Some(session);
                    self.auto_account = false;
                    self.auto_space = false;
                    self.request = None;
                    self.success = false;
                    effects.extend(self.push(Page::Summary, String::new()));
                    effects.extend(self.revalidate());
                }
            }
        }
        if matches!(self.page, Page::Home | Page::Management) {
            self.page = if self.manager.page == crate::management::Page::Home {
                Page::Home
            } else {
                Page::Management
            };
        }
        self.dirty = true;
        effects
    }
    fn restore_new_draft(&mut self) {
        if let Some((mut draft, account, space)) = self.new_draft.take() {
            draft.revision = self.draft.revision + 1;
            draft.name_valid = false;
            draft.directory_valid = false;
            self.draft = draft;
            self.auto_account = account;
            self.auto_space = space;
        }
        self.resume = None;
    }
    fn name_valid(&self, name: &str) -> bool {
        if self.resume.is_some() {
            !name.is_empty()
                && name.chars().count() <= NAME_MAX_CHARS
                && name.trim() == name
                && !name.chars().any(char::is_control)
        } else {
            valid_name(name) && name.chars().count() <= NAME_MAX_CHARS
        }
    }
    fn ready(&self) -> bool {
        self.draft.account.is_some()
            && self.draft.space.is_some()
            && self.name_valid(&self.draft.name)
            && if self.resume.is_some() {
                !self
                    .resume_validation
                    .as_deref()
                    .is_some_and(resume_invalid)
            } else {
                self.draft.name_valid && self.draft.directory_valid
            }
            && self.draft.directory.path().is_some()
    }
    fn launch_supported(&self) -> bool {
        if self.resume.is_some() {
            self.manager.capabilities["resume"]["version"] == 1
        } else {
            self.launch_supported
        }
    }
    pub fn rows(&self) -> Vec<Row> {
        let path = |p: Option<&Path>| {
            p.map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| UNSET.into())
        };
        let mut rows: Vec<Row> = match &self.page {
            Page::Home | Page::Management => self.manager.rows(),
            Page::Summary => {
                let fields = [
                    (
                        "Account",
                        self.draft
                            .account
                            .as_deref()
                            .map(crate::usage::short_name)
                            .unwrap_or_else(|| UNSET.into()),
                    ),
                    (
                        "Space",
                        self.draft
                            .space
                            .as_ref()
                            .map(Space::label)
                            .filter(|s| !s.is_empty())
                            .unwrap_or(UNSET)
                            .to_owned(),
                    ),
                    (
                        "Name",
                        if self.draft.name.is_empty() {
                            UNSET.into()
                        } else {
                            crate::usage::display_name(&self.draft.name)
                        },
                    ),
                    ("Dir", path(self.draft.directory.path())),
                ];
                let width = fields
                    .iter()
                    .map(|(label, _)| label.len())
                    .max()
                    .unwrap_or(0);
                fields
                    .into_iter()
                    .enumerate()
                    .map(|(i, (label, value))| {
                        let mut row = Row::field(label, value, width);
                        row.enabled = i != 3
                            || self.resume.is_none()
                            || self.manager.capabilities["resume"]["directory_selection"] == true;
                        row
                    })
                    .collect()
            }
            Page::Config => vec![
                Row::yes(format!(
                    "Default Dir: {}",
                    if self.prefs.mode == Mode::Global {
                        "Global"
                    } else {
                        "Per-Space"
                    }
                )),
                Row::gate("Global root", self.prefs.mode == Mode::Global)
                    .with_preview(path(self.prefs.global_root.as_deref()))
                    .preview_gap(1),
                Row::gate("Space roots", self.prefs.mode == Mode::PerSpace),
            ],
            Page::Roots => self
                .spaces
                .iter()
                .map(|s| {
                    Row::yes(&s.label)
                        .with_preview(path(
                            self.prefs
                                .space_roots
                                .get(&Preferences::key(&self.instance, &s.workspace_id))
                                .map(PathBuf::as_path),
                        ))
                        .preview_gap(1)
                })
                .collect(),
            Page::Accounts => {
                use unicode_width::UnicodeWidthStr;
                let labels = crate::usage::picker_labels(&self.accounts);
                let preview_width = labels
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.as_str()))
                    .max()
                    .unwrap_or(0);
                let name_width = self
                    .accounts
                    .iter()
                    .map(|a| UnicodeWidthStr::width(crate::usage::short_name(&a.id).as_str()))
                    .max()
                    .unwrap_or(0);
                self.accounts
                    .iter()
                    .zip(labels)
                    .map(|(a, usage)| {
                        let column = 2 + name_width + 1 + preview_width
                            - UnicodeWidthStr::width(usage.as_str());
                        Row::gate(crate::usage::short_name(&a.id), a.selectable)
                            .with_column_preview(usage, column, false)
                            .preview_gap(1)
                            .preview_color(crate::widgets::SECONDARY)
                            .with_status(
                                crate::usage::Usage::account(a).status.clone(),
                                crate::usage::Usage::account(a).status == "Ready",
                            )
                    })
                    .collect()
            }
            Page::Spaces => std::iter::once(Row::yes("[create new space]"))
                .chain(self.spaces.iter().map(|s| {
                    Row::yes(&s.label)
                        .with_selected_preview(format!("({})", s.workspace_id))
                        .preview_gap(1)
                }))
                .collect(),
            Page::Name => vec![Row::gate("Continue", self.name_valid(&self.input.value))],
            Page::SpaceName => vec![Row::gate(
                "Use new space",
                valid_name(&self.input.value)
                    && !self.spaces.iter().any(|s| s.label == self.input.value),
            )],
            Page::NewDirectory(_, _) => vec![Row::gate(
                "Create directory",
                valid_name(&self.input.value) && self.listing.creatable,
            )],
            Page::Directory(_) => {
                std::iter::once(Row::gate("[use this directory]", self.listing.existing))
                    .chain((self.directory_action_count() == 3).then(|| Row::yes("[leave unset]")))
                    .chain(std::iter::once(Row::gate(
                        "<new>/",
                        self.listing.existing || self.listing.creatable,
                    )))
                    .chain(
                        (self.listing.children.is_empty() && !self.listing_pending)
                            .then(|| Row::placeholder("(none)")),
                    )
                    .chain(self.listing.children.iter().map(|p| {
                        Row::yes(format!(
                            "{}/",
                            p.file_name().unwrap_or_default().to_string_lossy()
                        ))
                    }))
                    .collect()
            }
            Page::Launching => vec![Row::gate("Starting session…", false)],
        };
        if Self::is_popup(&self.page) {
            if matches!(
                self.page,
                Page::Directory(_) | Page::Spaces | Page::Accounts | Page::Roots
            ) {
                rows = crate::popup::with_cancel(rows);
            } else {
                rows.push(Row::yes("Cancel"));
            }
            rows = rows.into_iter().map(Row::popup_option).collect();
        }
        rows
    }
    fn push(&mut self, page: Page, text: String) -> Vec<Effect> {
        self.stack.push(Saved {
            page: self.page.clone(),
            options: self.options.clone(),
            text: self.input.value.clone(),
            listing: self.listing.clone(),
            listing_pending: self.listing_pending,
            popup_size: self.popup_size.take(),
        });
        self.page = page;
        self.pending_child = None;
        self.options.selected = if self.page == Page::Accounts {
            self.opening_account_row()
        } else {
            0
        };
        if self.page == Page::Spaces
            && let Some(Space::Existing { id, .. }) = &self.draft.space
            && let Some(index) = self.spaces.iter().position(|s| s.workspace_id == *id)
        {
            self.options.selected = index + 1;
        }
        self.options.scroll = 0;
        self.options.back = false;
        self.input.value = text;
        self.input.cursor = self.input.value.len();
        self.input.max_chars = (self.page == Page::Name).then_some(NAME_MAX_CHARS);
        self.listing = Listing::default();
        self.error.clear();
        self.dirty = true;
        if self.page == Page::Summary && self.resume.is_none() {
            let intents = self.manager.read_sessions();
            self.management(intents)
        } else if matches!(self.page, Page::Directory(_) | Page::NewDirectory(..)) {
            self.inspect()
        } else {
            vec![]
        }
    }
    fn inspect(&mut self) -> Vec<Effect> {
        self.listing_pending = true;
        self.revision += 1;
        self.listing = Listing::default();
        vec![Effect::Inspect(
            self.revision,
            match &self.page {
                Page::NewDirectory(_, parent) => parent
                    .join(&self.input.value)
                    .to_string_lossy()
                    .into_owned(),
                _ => self.input.value.clone(),
            },
        )]
    }
    fn pop(&mut self) -> Vec<Effect> {
        self.revision += 1;
        let save = matches!(self.page, Page::Config | Page::Roots);
        if self.page == Page::Accounts {
            self.switch = None;
        }
        if self.page == Page::Summary && self.resume.is_some() {
            self.restore_new_draft();
        }
        if let Some(old) = self.stack.pop() {
            self.page = old.page;
            self.options = old.options;
            self.options.navigation = Default::default();
            self.options.preview_visible = false;
            self.input.value = old.text;
            self.input.cursor = self.input.value.len();
            self.input.max_chars = (self.page == Page::Name).then_some(NAME_MAX_CHARS);
            self.listing = old.listing;
            self.listing_pending = old.listing_pending;
            self.popup_size = old.popup_size;
            self.options.back = false;
            self.error.clear();
            self.dirty = true;
            if save {
                let mut e = vec![Effect::Save];
                e.extend(self.revalidate());
                e
            } else {
                vec![]
            }
        } else {
            self.exit = true;
            vec![Effect::Exit]
        }
    }
    pub fn revalidate(&mut self) -> Vec<Effect> {
        self.resume_validation = None;
        self.draft.changed();
        self.draft
            .derive(self.prefs.root(&self.instance, self.draft.space.as_ref()));
        self.draft.directory_valid = false;
        self.error.clear();
        self.dirty = true;
        if self.request.is_some() {
            return vec![];
        }
        if let Some(request) = self.creation_request() {
            self.pending = true;
            vec![Effect::Validate(self.draft.revision, request)]
        } else {
            self.pending = false;
            vec![]
        }
    }
    pub fn creation_request(&self) -> Option<Value> {
        if !self.name_valid(&self.draft.name) {
            return None;
        }
        let space = match self.draft.space.as_ref()? {
            Space::Existing { id, .. } => json!({"kind":"existing","id":id}),
            Space::New { label } => json!({"kind":"new","label":label}),
        };
        let (path, create, confirmed) = match &self.draft.directory {
            Directory::Unset => return None,
            Directory::Existing(p) => (p, vec![], true),
            Directory::Derived(p) => {
                let key = match (&self.prefs.mode, &self.draft.space) {
                    (Mode::Global, _) => "global".into(),
                    (_, Some(Space::Existing { id, .. })) => Preferences::key(&self.instance, id),
                    _ => String::new(),
                };
                let mut planned = self.prefs.root_plans.get(&key).cloned().unwrap_or_default();
                planned.push(p.clone());
                (p, planned, false)
            }
            Directory::Create { path, planned } => (path, planned.clone(), false),
        };
        if let Some(session) = &self.resume {
            let mut request = json!({"version":1,"request_id":uuid::Uuid::new_v4().to_string(),"session_id":session["id"],"expected_generation":session["generation"],"account_id":self.draft.account.as_ref()?,"name":self.draft.name,"host":self.host,"space":space});
            request["source_snapshot"] = session.clone();
            if session["confirm_unfinished"] == true {
                request["confirm_unfinished"] = json!(true);
            }
            if self.manager.capabilities["resume"]["directory_selection"] == true {
                request["directory"] =
                    json!({"path":path,"create":create,"confirmed_existing":confirmed});
            }
            return Some(request);
        }
        Some(
            json!({"version":1,"request_id":uuid::Uuid::new_v4().to_string(),"account_id":self.draft.account.as_ref()?,"name":self.draft.name,"host":self.host,"space":space,"directory":{"path":path,"create":create,"confirmed_existing":confirmed}}),
        )
    }
    pub fn validation(&mut self, revision: u64, result: Result<(), String>) {
        if revision != self.draft.revision {
            return;
        }
        self.pending = false;
        self.draft.name_valid = result.is_ok();
        self.draft.directory_valid = result.is_ok();
        self.error = result.err().unwrap_or_default();
        if self.resume.is_some() {
            self.resume_validation = (!self.error.is_empty()).then(|| self.error.clone());
            self.error.clear();
        }
        self.dirty = true;
    }
    pub fn inventory(
        &mut self,
        accounts: Vec<Account>,
        spaces: Vec<Workspace>,
        now: i64,
    ) -> Vec<Effect> {
        let empty_accounts = self.accounts.is_empty();
        let picker_cancel = matches!(self.page, Page::Accounts | Page::Spaces)
            && (self.page != Page::Accounts || !empty_accounts)
            && self
                .rows()
                .get(self.options.selected)
                .is_some_and(|r| r.label == "Cancel");
        let picker_account = (self.page == Page::Accounts)
            .then(|| {
                self.accounts
                    .get(self.options.selected)
                    .map(|a| a.id.clone())
            })
            .flatten();
        let old_account = self.draft.account.clone();
        let old_space = self.draft.space.clone();
        let picker_space = if self.page == Page::Spaces {
            self.options
                .selected
                .checked_sub(1)
                .and_then(|i| self.spaces.get(i))
                .map(|s| s.workspace_id.clone())
        } else {
            None
        };
        self.accounts = accounts;
        self.manager.inventory(self.accounts.clone());
        self.spaces = spaces;
        if let Some(id) = picker_space {
            self.options.selected = self
                .spaces
                .iter()
                .position(|s| s.workspace_id == id)
                .map(|i| i + 1)
                .unwrap_or(0);
        }
        if self.auto_space {
            let mut focused = self.spaces.iter().filter(|s| s.focused);
            self.draft.space = focused
                .next()
                .filter(|_| focused.next().is_none())
                .map(|s| Space::Existing {
                    id: s.workspace_id.clone(),
                    label: s.label.clone(),
                });
        }
        if let Some(Space::Existing { id, label }) = &mut self.draft.space {
            if let Some(s) = self.spaces.iter().find(|s| s.workspace_id == *id) {
                *label = s.label.clone();
            } else {
                self.draft.space = None;
            }
        }
        // Freshness governs choosing a default, not retaining an existing choice.
        // Only a successful authoritative inventory may invalidate its membership.
        if self
            .draft
            .account
            .as_ref()
            .is_some_and(|id| !self.accounts.iter().any(|a| a.id == *id && a.selectable))
        {
            self.draft.account = None;
        }
        if self.auto_account && self.draft.account.is_none() {
            self.draft.account = self
                .prefs
                .last_account
                .as_ref()
                .and_then(|last| {
                    self.accounts.iter().find(|a| {
                        a.id == *last
                            && a.selectable
                            && a.prefill["eligible"] == true
                            && a.prefill["valid_until"].as_i64().unwrap_or(0) > now
                    })
                })
                .map(|a| a.id.clone());
        } else if self
            .draft
            .account
            .as_ref()
            .is_some_and(|id| !self.accounts.iter().any(|a| a.id == *id && a.selectable))
        {
            self.draft.account = None;
        }
        if self.page == Page::Accounts {
            if let Some(id) = picker_account {
                self.options.selected = self
                    .accounts
                    .iter()
                    .position(|a| a.id == id)
                    .unwrap_or_else(|| self.opening_account_row());
            } else if empty_accounts && !self.accounts.is_empty() {
                self.options.selected = self.opening_account_row();
            }
        }
        if picker_cancel {
            self.options.selected = self.rows().len() - 1;
        }
        self.dirty = true;
        if old_account == self.draft.account && old_space == self.draft.space {
            vec![]
        } else {
            self.revalidate()
        }
    }
    pub fn listing(&mut self, revision: u64, result: Result<Listing, String>) {
        if revision != self.revision
            || !matches!(self.page, Page::Directory(_) | Page::NewDirectory(..))
        {
            return;
        }
        self.listing_pending = false;
        match result {
            Ok(l) => {
                self.listing = l;
                if matches!(self.page, Page::Directory(_)) && self.listing.existing {
                    let child = self
                        .pending_child
                        .take()
                        .and_then(|child| self.listing.children.iter().position(|p| *p == child))
                        .map(|i| i + self.directory_children_offset());
                    let prior = self
                        .listing
                        .path
                        .as_ref()
                        .and_then(|path| self.history.get(path))
                        .copied();
                    let rows = self.rows();
                    let remembered =
                        prior.filter(|i| rows.get(*i).is_some_and(|row| !row.separator));
                    self.options.selected = child
                        .or(remembered)
                        .unwrap_or(self.directory_children_offset());
                    self.options.scroll = 0;
                }
            }
            Err(e) => {
                self.listing = Listing::default();
                self.error = e;
            }
        }
        let rows = self.rows();
        if self.options.selected >= rows.len() {
            self.options.selected = 0;
        }
        self.dirty = true;
    }
    fn opening_account_row(&self) -> usize {
        let current = self
            .switch
            .as_ref()
            .and_then(|s| s["account_id"].as_str())
            .or(self.draft.account.as_deref());
        self.accounts
            .iter()
            .position(|a| Some(a.id.as_str()) == current)
            .or_else(|| (!self.accounts.is_empty()).then_some(0))
            .unwrap_or(0)
    }
    fn directory_action_count(&self) -> usize {
        if matches!(
            self.page,
            Page::Directory(Target::Global | Target::Space(_))
        ) {
            3
        } else {
            2
        }
    }
    fn directory_children_offset(&self) -> usize {
        self.directory_action_count()
    }
    fn directory_parent(&mut self) -> Vec<Effect> {
        if let Some(path) = self.listing.path.clone()
            && let Some(parent) = path.parent()
        {
            self.history.insert(path.clone(), self.options.selected);
            self.pending_child = Some(path.clone());
            self.input.value = parent.to_string_lossy().into_owned();
            self.input.cursor = self.input.value.len();
            return self.inspect();
        }
        vec![]
    }
    fn choose_directory(&mut self, target: Target, directory: Directory) -> Vec<Effect> {
        let Some(path) = directory.path().map(Path::to_path_buf) else {
            match target {
                Target::Draft => return vec![],
                Target::Global => {
                    self.prefs.global_root = None;
                    self.prefs.root_plans.remove("global");
                }
                Target::Space(id) => {
                    let key = Preferences::key(&self.instance, &id);
                    self.prefs.space_roots.remove(&key);
                    self.prefs.root_plans.remove(&key);
                }
            }
            self.pop();
            return self.revalidate();
        };
        match target {
            Target::Draft => self.draft.directory = directory,
            Target::Global => {
                self.prefs.global_root = Some(path);
                let plan = match &directory {
                    Directory::Create { planned, .. } => planned.clone(),
                    _ => vec![],
                };
                self.prefs.root_plans.insert("global".into(), plan);
            }
            Target::Space(id) => {
                let plan = match &directory {
                    Directory::Create { planned, .. } => planned.clone(),
                    _ => vec![],
                };
                self.prefs
                    .root_plans
                    .insert(Preferences::key(&self.instance, &id), plan);
                self.prefs
                    .space_roots
                    .insert(Preferences::key(&self.instance, &id), path);
            }
        }
        if matches!(self.page, Page::NewDirectory(..)) {
            self.pop();
        }
        self.pop();
        self.revalidate()
    }
    pub fn refresh_live(&mut self) -> Vec<Effect> {
        let intents = if self.page == Page::Summary && self.resume.is_none() {
            self.manager.read_sessions()
        } else {
            self.manager.refresh_live()
        };
        self.management(intents)
    }
    fn shared_directory(&self) -> bool {
        self.resume.is_none()
            && self.draft.directory.path().is_some_and(|path| {
                self.manager.sessions.iter().any(|s| {
                    s["directory"]
                        .as_str()
                        .is_some_and(|p| Path::new(p) == path)
                })
            })
    }
    fn start_ready(&self) -> bool {
        self.ready()
            && self.launch_supported()
            && (!self.pending || self.resume.is_some())
            && self.request.is_none()
            && (self.resume.is_some() || self.manager.sessions_known)
    }
    fn begin_start(&mut self) -> Vec<Effect> {
        if !self.start_ready() {
            return vec![];
        }
        if let Some(request) = self.creation_request() {
            self.request = Some(request.clone());
            self.launch_background = false;
            self.launch_dialog = true;
            self.manager.dialog = Some(crate::dialog::Dialog::progress("Starting session"));
            return vec![Effect::Start(request)];
        }
        vec![]
    }
    fn activate(&mut self) -> Vec<Effect> {
        if Self::is_popup(&self.page)
            && self
                .rows()
                .get(self.options.selected)
                .is_some_and(|r| r.label == "Cancel")
        {
            return self.pop();
        }
        if self.options.back {
            return self.pop();
        }
        if self.page == Page::Summary && self.options.selected == 4 {
            if self.start_ready()
                && self
                    .resume_validation
                    .as_deref()
                    .is_some_and(|e| e.starts_with("UNFINISHED_HISTORY_CONFIRMATION_REQUIRED"))
            {
                let mut dialog = crate::dialog::Dialog::new("Unfinished conversation", vec!["This conversation ended during a turn. Start from its saved history in the chosen directory? Review unfinished work before continuing; the interrupted request will not be replayed automatically.".into()], &["Start session", "Cancel"]);
                dialog.selected = 1;
                self.start_confirmation = Some((dialog, StartConfirmation::UnfinishedHistory));
                return vec![];
            }
            if self.start_ready() && self.shared_directory() {
                let mut dialog = crate::dialog::Dialog::new("Existing session in directory", vec![
                    "Another session already uses this directory. Both sessions can change the same project files.".into(),
                    self.draft.directory.path().unwrap().display().to_string(),
                    "Start another session here?".into(),
                ], &["Start session", "Cancel"]);
                dialog.selected = 1;
                self.start_confirmation = Some((
                    dialog,
                    StartConfirmation::SharedDirectory(
                        self.draft.directory.path().unwrap().to_owned(),
                    ),
                ));
                return vec![];
            }
            return self.begin_start();
        }
        let rows = self.rows();
        if !rows.get(self.options.selected).is_some_and(|r| r.enabled) {
            return vec![];
        }
        match self.page.clone() {
            Page::Management => vec![],
            Page::Home => self.push(
                if self.options.selected == 0 {
                    Page::Summary
                } else {
                    Page::Config
                },
                String::new(),
            ),
            Page::Summary => match self.options.selected {
                0 => self.push(Page::Accounts, String::new()),
                1 => self.push(Page::Spaces, String::new()),
                2 => self.push(Page::Name, self.draft.name.clone()),
                _ => self.push(
                    Page::Directory(Target::Draft),
                    self.draft
                        .directory
                        .path()
                        .or_else(|| self.prefs.root(&self.instance, self.draft.space.as_ref()))
                        .or(self.prefs.global_root.as_deref())
                        .unwrap_or(&self.initial)
                        .to_string_lossy()
                        .into_owned(),
                ),
            },
            Page::Config => match self.options.selected {
                0 => {
                    self.prefs.mode = if self.prefs.mode == Mode::Global {
                        Mode::PerSpace
                    } else {
                        Mode::Global
                    };
                    vec![]
                }
                1 => self.push(
                    Page::Directory(Target::Global),
                    self.prefs
                        .global_root
                        .as_deref()
                        .unwrap_or(&self.initial)
                        .to_string_lossy()
                        .into_owned(),
                ),
                _ => self.push(Page::Roots, String::new()),
            },
            Page::Roots => {
                let id = self.spaces[self.options.selected].workspace_id.clone();
                let p = self
                    .prefs
                    .space_roots
                    .get(&Preferences::key(&self.instance, &id))
                    .unwrap_or(&self.initial)
                    .to_string_lossy()
                    .into_owned();
                self.push(Page::Directory(Target::Space(id)), p)
            }
            Page::Accounts => {
                if let Some(session) = self.switch.take() {
                    let account = self.accounts[self.options.selected].id.clone();
                    self.pop();
                    self.manager.switch_confirmation(session, &account);
                    return vec![];
                }
                self.draft.account = Some(self.accounts[self.options.selected].id.clone());
                self.auto_account = false;
                self.pop();
                self.revalidate()
            }
            Page::Spaces => {
                if self.options.selected == 0 {
                    self.push(Page::SpaceName, String::new())
                } else {
                    self.auto_space = false;
                    let s = &self.spaces[self.options.selected - 1];
                    self.draft.space = Some(Space::Existing {
                        id: s.workspace_id.clone(),
                        label: s.label.clone(),
                    });
                    self.pop();
                    self.revalidate()
                }
            }
            Page::Name => {
                self.draft.name = self.input.value.clone();
                self.pop();
                self.revalidate()
            }
            Page::SpaceName => {
                self.auto_space = false;
                self.draft.space = Some(Space::New {
                    label: self.input.value.clone(),
                });
                self.pop();
                self.pop();
                self.revalidate()
            }
            Page::Directory(target) => {
                if self.directory_action_count() == 3 && self.options.selected == 1 {
                    return self.choose_directory(target, Directory::Unset);
                }
                if self.options.selected == self.directory_action_count() - 1 {
                    let path = self.listing.path.clone().unwrap();
                    let (parent, name) = if self.listing.existing {
                        (path, String::new())
                    } else {
                        (
                            path.parent().unwrap().into(),
                            path.file_name().unwrap().to_string_lossy().into_owned(),
                        )
                    };
                    self.push(Page::NewDirectory(target, parent), name)
                } else {
                    let path = if self.options.selected == 0 {
                        self.listing.path.clone().unwrap()
                    } else {
                        self.listing
                            .resolved
                            .get(
                                &self.listing.children
                                    [self.options.selected - self.directory_children_offset()],
                            )
                            .cloned()
                            .unwrap_or_else(|| {
                                self.listing.children
                                    [self.options.selected - self.directory_children_offset()]
                                .clone()
                            })
                    };
                    self.choose_directory(target, Directory::Existing(path))
                }
            }
            Page::NewDirectory(target, parent) => {
                let path = parent.join(&self.input.value);
                self.choose_directory(
                    target,
                    Directory::Create {
                        path: path.clone(),
                        planned: vec![path],
                    },
                )
            }
            Page::Launching => vec![],
        }
    }
    fn edit(&mut self, text: &str) -> Vec<Effect> {
        self.input.insert(text);
        self.options.back = false;
        if matches!(self.page, Page::Directory(_) | Page::NewDirectory(..)) {
            self.inspect()
        } else {
            vec![]
        }
    }
    pub fn paste(&mut self, text: &str) -> Vec<Effect> {
        if matches!(self.page, Page::Home | Page::Management) {
            self.manager.paste(text);
            self.dirty = true;
            return vec![];
        }
        if self.is_form() {
            self.dirty = true;
            self.edit(text)
        } else {
            vec![]
        }
    }
    fn is_form(&self) -> bool {
        matches!(
            self.page,
            Page::Name | Page::SpaceName | Page::Directory(_) | Page::NewDirectory(..)
        )
    }
    pub fn key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let events = self.navigation.events(key);
        events
            .into_iter()
            .flat_map(|(key, wrap)| self.key_with_wrap(key, wrap))
            .collect()
    }
    pub fn flush_navigation(&mut self) -> Vec<Effect> {
        let events = self.navigation.flush();
        events
            .into_iter()
            .flat_map(|(key, wrap)| self.key_with_wrap(key, wrap))
            .collect()
    }
    fn key_with_wrap(&mut self, key: KeyEvent, wrap: bool) -> Vec<Effect> {
        if key.kind == crossterm::event::KeyEventKind::Release {
            return vec![];
        }
        self.dirty = true;
        if let Some((dialog, confirmation)) = &mut self.start_confirmation
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            match dialog.key_with_wrap(key, wrap) {
                crate::dialog::Event::Choose(0, _) => {
                    let unchanged = match confirmation {
                        StartConfirmation::SharedDirectory(directory) => {
                            self.draft.directory.path() == Some(directory.as_path())
                        }
                        StartConfirmation::UnfinishedHistory => {
                            if let Some(session) = &mut self.resume {
                                session["confirm_unfinished"] = json!(true);
                            }
                            self.resume_validation = None;
                            true
                        }
                    };
                    self.start_confirmation = None;
                    if !unchanged {
                        self.error = "Directory changed; review Start Session again.".into();
                        return vec![];
                    }
                    return self.begin_start();
                }
                crate::dialog::Event::Dismiss | crate::dialog::Event::Choose(..) => {
                    self.start_confirmation = None
                }
                crate::dialog::Event::None => {}
            }
            return vec![];
        }
        if self.launch_dialog && !key.modifiers.contains(KeyModifiers::CONTROL) {
            if let Some(d) = &mut self.manager.dialog
                && matches!(
                    d.key_with_wrap(key, wrap),
                    crate::dialog::Event::Dismiss | crate::dialog::Event::Choose(..)
                )
            {
                self.manager.dialog = None;
                self.launch_dialog = false;
                self.launch_background = true;
                self.page = if self.resume.is_some() {
                    Page::Management
                } else {
                    Page::Home
                };
                self.manager.page = if self.resume.is_some() {
                    crate::management::Page::Sessions
                } else {
                    crate::management::Page::Home
                };
                self.manager.options = Options::default();
                if let Some(session) = &self.resume {
                    self.manager.options.selected = self
                        .manager
                        .sessions
                        .iter()
                        .position(|s| s["id"] == session["id"])
                        .map(|i| crate::session_list::session_row(&self.manager.sessions, i))
                        .unwrap_or(0);
                }
                self.stack.clear();
            }
            return vec![];
        }
        if matches!(self.page, Page::Home | Page::Management)
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            let intents = self.manager.key_with_wrap(key, wrap);
            return self.management(intents);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('d') => {
                    self.exit = true;
                    return vec![Effect::Exit];
                }
                KeyCode::Char('u')
                    if self.is_form()
                        && !matches!(self.page, Page::Directory(_) | Page::NewDirectory(..)) =>
                {
                    self.input.clear();
                    return if matches!(self.page, Page::Directory(_) | Page::NewDirectory(..)) {
                        self.inspect()
                    } else {
                        vec![]
                    };
                }
                _ => return vec![],
            }
        }
        if self.page == Page::Launching {
            if key.code == KeyCode::Esc {
                self.exit = true;
                return vec![Effect::Exit];
            }
            return vec![];
        }
        match key.code {
            KeyCode::Esc => self.pop(),
            KeyCode::Enter => self.activate(),
            KeyCode::Char(' ') if !self.is_form() => self.activate(),
            KeyCode::Char(c) if self.is_form() => self.edit(&c.to_string()),
            KeyCode::Backspace | KeyCode::Delete | KeyCode::Home | KeyCode::End
                if self.is_form()
                    && (!matches!(key.code, KeyCode::Home | KeyCode::End)
                        || !matches!(self.page, Page::Directory(_))) =>
            {
                self.input.edit(key.code);
                if matches!(self.page, Page::Directory(_) | Page::NewDirectory(..)) {
                    self.inspect()
                } else {
                    vec![]
                }
            }
            KeyCode::Home | KeyCode::End => {
                self.options.boundary(
                    &self.rows(),
                    self.page == Page::Summary,
                    key.code == KeyCode::End,
                );
                vec![]
            }
            KeyCode::Up | KeyCode::Down => {
                self.options.step_with_wrap(
                    &self.rows(),
                    self.page == Page::Summary,
                    key.code == KeyCode::Up,
                    wrap,
                );
                vec![]
            }
            KeyCode::Right if matches!(self.page, Page::Directory(_)) => {
                if self.options.back && key.code == KeyCode::Right {
                    self.options.back = false;
                    return vec![];
                }
                let Some(index) = self
                    .options
                    .selected
                    .checked_sub(self.directory_children_offset())
                else {
                    return vec![];
                };
                if let Some(child) = self.listing.children.get(index).cloned() {
                    if let Some(path) = &self.listing.path {
                        self.history.insert(path.clone(), self.options.selected);
                    }
                    self.options.selected = self
                        .history
                        .get(&child)
                        .copied()
                        .unwrap_or(self.options.selected);
                    self.input.value = child.to_string_lossy().into_owned();
                    self.input.cursor = self.input.value.len();
                    self.inspect()
                } else {
                    vec![]
                }
            }
            KeyCode::Left if matches!(self.page, Page::Directory(_)) => {
                if self.options.selected < self.directory_children_offset()
                    || self.options.selected
                        >= self.directory_children_offset() + self.listing.children.len().max(1)
                    || self.listing_pending
                    || self.listing.path.as_deref() == Some(Path::new("/"))
                {
                    return vec![];
                }
                self.directory_parent()
            }
            KeyCode::Left | KeyCode::Right if self.is_form() => {
                self.input.edit(key.code);
                vec![]
            }
            KeyCode::Left | KeyCode::Right if Self::is_popup(&self.page) => vec![],
            KeyCode::Left => {
                self.options.back = true;
                vec![]
            }
            KeyCode::Right => {
                self.options.back = false;
                vec![]
            }
            _ => vec![],
        }
    }
    fn is_popup(page: &Page) -> bool {
        matches!(
            page,
            Page::Roots
                | Page::Accounts
                | Page::Spaces
                | Page::Name
                | Page::SpaceName
                | Page::Directory(_)
                | Page::NewDirectory(..)
        )
    }
    fn popup_title(&self) -> &'static str {
        match self.page {
            Page::Roots => "Space roots",
            Page::Accounts => "Choose Account",
            Page::Spaces => "Choose Space",
            Page::Name => "Session name",
            Page::SpaceName => "New Space name",
            Page::Directory(_) => "Start directory",
            Page::NewDirectory(..) => "New directory name",
            _ => "",
        }
    }
    fn paint_saved(&mut self, saved: &Saved, canvas: &mut Canvas, width: usize, height: usize) {
        let page = std::mem::replace(&mut self.page, saved.page.clone());
        let options = std::mem::replace(&mut self.options, saved.options.clone());
        let input = std::mem::replace(&mut self.input, TextInput::new(saved.text.clone()));
        let listing = std::mem::replace(&mut self.listing, saved.listing.clone());
        let listing_pending = std::mem::replace(&mut self.listing_pending, saved.listing_pending);
        let error = std::mem::take(&mut self.error);
        let pending = std::mem::replace(&mut self.pending, false);
        let popup_size = std::mem::replace(&mut self.popup_size, saved.popup_size.clone());
        self.paint_page(canvas, width, height);
        self.page = page;
        self.options = options;
        self.input = input;
        self.listing = listing;
        self.listing_pending = listing_pending;
        self.error = error;
        self.pending = pending;
        self.popup_size = popup_size;
    }
    fn popup_description(&self) -> &'static str {
        match self.page {
            Page::Accounts => "Choose an account for this session.",
            Page::Spaces => "Choose a destination Space.",
            Page::Roots => "Choose a Space to set its default directory.",
            Page::Name => "Choose a name for this session.",
            Page::SpaceName => "Choose a name for the new Space.",
            Page::Directory(_) => {
                "Choose a directory.
New directories are created only when the session starts."
            }
            Page::NewDirectory(..) => {
                "Choose a directory name. Creation is deferred until the session starts."
            }
            _ => "",
        }
    }
    fn popup_field(&self, label: &str) -> crate::dialog::Field {
        let mut field = crate::dialog::Field::new(label, &self.input.value);
        field.input = self.input.clone();
        let valid = if matches!(self.page, Page::Directory(_)) {
            self.listing_pending || self.listing.existing || self.listing.creatable
        } else if matches!(self.page, Page::NewDirectory(..)) && self.listing_pending {
            true
        } else {
            self.rows().first().is_some_and(|r| r.enabled)
        };
        if !valid {
            field.excluded.push(self.input.value.clone());
        }
        field
    }
    fn paint_page(&mut self, canvas: &mut Canvas, width: usize, height: usize) {
        if !Self::is_popup(&self.page) {
            self.paint_screen(canvas, width, height);
            return;
        }
        let rows = self.rows();
        if matches!(
            self.page,
            Page::Name | Page::SpaceName | Page::NewDirectory(..)
        ) {
            let label = if self.page == Page::SpaceName {
                "Space name"
            } else if self.page == Page::Name {
                "Name"
            } else {
                "Directory name"
            };
            let mut dialog = crate::dialog::Dialog::new(
                self.popup_title(),
                vec![self.popup_description().into()],
                &[],
            );
            dialog.buttons = rows;
            dialog.fields.push(self.popup_field(label));
            dialog.selected = self.options.selected;
            dialog.size = self
                .popup_size
                .as_ref()
                .filter(|(page, _)| *page == self.page)
                .map(|(_, size)| *size);
            dialog.paint(canvas, &self.manager.palette);
            self.popup_size = dialog.size.map(|size| (self.page.clone(), size));
            return;
        }
        let editor = matches!(self.page, Page::Directory(_)).then(|| self.popup_field("Directory"));
        let note = if self.listing_pending && editor.is_some() {
            "Loading…".into()
        } else if !self.error.is_empty() {
            crate::messages::describe(&self.error)
        } else {
            String::new()
        };
        let description = self.popup_description();
        let mut presentation = crate::popup::Choices {
            title: self.popup_title(),
            description,
            rows: &rows,
            editor: editor.as_ref(),
            note: &note,
            note_accent: note == "Ready",
            large: editor.is_some(),
            fit_description: matches!(self.page, Page::Accounts | Page::Spaces),
            browse_hint: editor.is_some() && self.control_tokens().contains(&"◀/▶: Parent/Open"),
        };
        if self
            .popup_size
            .as_ref()
            .is_none_or(|(page, size)| *page != self.page || size.viewport != (width, height))
        {
            self.popup_size = Some((self.page.clone(), presentation.measure((width, height))));
        }
        let opened = self.popup_size.as_ref().unwrap().1;
        self.popup_size = Some((
            self.page.clone(),
            presentation.grow_for_selection(opened, self.options.selected),
        ));
        crate::popup::paint_choices(
            canvas,
            &self.manager.palette,
            &presentation,
            &mut self.options,
            self.popup_size.as_ref().unwrap().1,
        );
    }
    pub fn paint(&mut self, field: &mut Field, now: u64, width: usize, height: usize) -> Canvas {
        field.resize(width, height);
        field.options_top = 3;
        field.background = Some(self.terminal_background);
        let mut canvas = field.paint(now);
        if Self::is_popup(&self.page) {
            let first = self
                .stack
                .iter()
                .rposition(|s| !Self::is_popup(&s.page))
                .unwrap_or(0);
            let saved = self.stack[first..].to_vec();
            if saved.is_empty() {
                self.paint_saved(
                    &Saved {
                        page: Page::Home,
                        options: Options::default(),
                        text: String::new(),
                        listing: Listing::default(),
                        listing_pending: false,
                        popup_size: None,
                    },
                    &mut canvas,
                    width,
                    height,
                );
            }
            for scene in &saved {
                self.paint_saved(scene, &mut canvas, width, height);
            }
        }
        self.paint_page(&mut canvas, width, height);
        field.apply_masks(&canvas);
        canvas
    }
    fn control_tokens(&self) -> &'static [&'static str] {
        if matches!(self.page, Page::Directory(_)) {
            if self.options.back
                || self
                    .options
                    .selected
                    .checked_sub(self.directory_children_offset())
                    .and_then(|i| self.listing.children.get(i))
                    .is_none()
            {
                return &["▲/▼: Choose", "Enter: Select", "Esc: Back"];
            }
            &[
                "▲/▼: Choose",
                "◀/▶: Parent/Open",
                "Enter: Select",
                "Esc: Back",
            ]
        } else if matches!(self.page, Page::NewDirectory(..)) {
            &["Enter: Accept", "Esc: Back"]
        } else if self.is_form() {
            &["Enter: Accept", "Esc: Back", "Ctrl-U: Clear"]
        } else if self.page == Page::Home {
            &["▲/▼/◀/▶: Move", "Enter/Space: Select", "Esc: Close"]
        } else if self.page == Page::Launching {
            &["Esc: Detach"]
        } else {
            &["▲/▼/◀/▶: Move", "Enter/Space: Select", "Esc: Back"]
        }
    }
    fn paint_screen(&mut self, canvas: &mut Canvas, width: usize, height: usize) {
        let white = Cell {
            fg: [238; 3],
            ..Cell::default()
        };
        let hints = crate::widgets::hint_lines(self.control_tokens(), width);
        let footer = height.saturating_sub(hints.len());
        if width < 24 || footer < 11 {
            canvas.text(
                0,
                0,
                &format!("Resize terminal (24×{})", 11 + hints.len()),
                white.clone(),
                false,
            );
            if height > 1 {
                canvas.text(0, 1, "Esc: Back / close", white, false);
            }
            return;
        }
        let management_title = self.manager.title();
        let title = match &self.page {
            Page::Home | Page::Management => management_title.as_str(),
            Page::Summary => {
                if self.resume.is_some() {
                    "Resume Session"
                } else {
                    "New Session"
                }
            }
            Page::Config => "Settings",
            Page::Roots => "Space roots",
            Page::Accounts => "Choose Account",
            Page::Spaces => "Choose Space",
            Page::Name => "Session name",
            Page::SpaceName => "New Space name",
            Page::Directory(_) => "Start directory",
            Page::NewDirectory(..) => "New directory name",
            Page::Launching => "Starting session",
        };
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let root = self
            .prefs
            .root(&self.instance, self.draft.space.as_ref())
            .unwrap_or(Path::new(UNSET));
        let status = self.status.line(seconds as i64, root, width);
        let bar = Cell {
            bg: Some(canvas.theme.as_ref().unwrap().bar(false)),
            ..white.clone()
        };
        canvas.text(0, 0, &" ".repeat(width), bar.clone(), false);
        canvas.text(0, 0, &status, bar, false);
        if !title.is_empty() {
            let bar = Cell {
                bg: Some(canvas.theme.as_ref().unwrap().bar(true)),
                bold: true,
                ..white.clone()
            };
            canvas.text(0, 1, &" ".repeat(width), bar.clone(), false);
            canvas.text(0, 1, &format!(" {title}"), bar, false);
        }
        if matches!(self.page, Page::Home | Page::Management) {
            self.manager.paint(canvas, footer);
            self.page = if self.manager.page == crate::management::Page::Home {
                Page::Home
            } else {
                Page::Management
            };
            return;
        }

        let top = if self.is_form() { 5 } else { 3 };
        self.paint_choices(canvas, top, footer, &hints);
    }
    fn paint_choices(&mut self, canvas: &mut Canvas, top: usize, footer: usize, hints: &[String]) {
        let rows = self.rows();
        let has_note = !self.error.is_empty()
            || self.pending
            || self.page == Page::Launching
            || self.page == Page::Summary && self.resume.is_none() && !self.manager.sessions_known
            || self.page == Page::Accounts && !self.options.back;
        let menu_controls = if has_note {
            footer.saturating_sub(2)
        } else {
            footer
        };
        let bottom = menu_controls.saturating_sub(if self.page == Page::Summary { 3 } else { 1 });
        let (left, capacity) = self
            .options
            .paint_menu(canvas, &rows, top, bottom, menu_controls);
        if self.page != Page::Launching {
            self.options.paint_back(canvas, left, top, "Back");
        }
        if self.page == Page::Summary {
            let enabled = self.start_ready();
            canvas.accent_text(
                left,
                top + rows.len().min(capacity),
                "•",
                Accent {
                    brightness: 600,
                    flash: false,
                    ..Accent::default()
                },
            );
            Row::gate(
                if self.resume.is_some() {
                    "Resume Session"
                } else if self.shared_directory() {
                    "Start Session ⚠"
                } else {
                    "Start Session"
                },
                enabled,
            )
            .with_preview(if self.shared_directory() {
                "(existing session in dir)".into()
            } else {
                self.resume_validation
                    .as_deref()
                    .map(resume_explanation)
                    .unwrap_or_default()
            })
            .preview_gap(1)
            .paint(
                canvas,
                left,
                top + rows.len().min(capacity) + 1,
                self.options.selected == 4 && !self.options.back,
                self.options.back,
            );
        }

        let note = if !self.error.is_empty() {
            self.error.clone()
        } else if self.page == Page::Summary
            && self.resume.is_none()
            && !self.manager.sessions_known
        {
            "Checking session directories…".into()
        } else if self.pending {
            "Checking choices…".into()
        } else if self.page == Page::Launching {
            self.request
                .as_ref()
                .map(|r| {
                    format!(
                        "Request {} — Esc detaches",
                        r["request_id"].as_str().unwrap_or("")
                    )
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        crate::widgets::paint_status(canvas, footer, &note);
        crate::widgets::paint_hints(canvas, hints);
        if let Some((dialog, _)) = &mut self.start_confirmation {
            dialog.paint(canvas, &self.manager.palette);
        }
        if self.launch_dialog
            && let Some(d) = &mut self.manager.dialog
        {
            d.paint(canvas, &self.manager.palette);
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn account_picker_opens_on_current_or_default_and_preserves_identity() {
        for current in [None, Some("codex:b")] {
            for switching in [false, true] {
                let mut a = app();
                a.accounts = serde_json::from_value(json!([
                    {"id":"codex:a","selectable":true},
                    {"id":"codex:b","selectable":true}
                ]))
                .unwrap();
                a.page = Page::Summary;
                if switching {
                    a.switch = Some(json!({"id":"session","account_id":current}));
                } else {
                    a.draft.account = current.map(str::to_owned);
                }
                a.push(Page::Accounts, String::new());
                assert_eq!(a.rows().last().unwrap().label, "Cancel");
                assert!(a.rows()[a.accounts.len()].separator);
                assert_eq!(a.options.selected, if current.is_some() { 1 } else { 0 });
                let chosen = a.accounts[a.options.selected].id.clone();
                let mut reordered = a.accounts.clone();
                reordered.reverse();
                a.inventory(reordered, vec![], 0);
                assert_eq!(a.accounts[a.options.selected].id, chosen);
                a.options.selected = a.rows().len() - 1;
                a.activate();
                assert_eq!(a.page, Page::Summary);
            }
        }
    }
    #[test]
    fn account_choice_uses_visible_identity_and_bottom_cancel_survives_inventory_growth() {
        let mut a = app();
        a.page = Page::Summary;
        a.accounts = serde_json::from_value(json!([
            {"id":"codex:first","selectable":true},
            {"id":"codex:second","selectable":true}
        ]))
        .unwrap();
        a.push(Page::Accounts, String::new());
        a.options.selected = 1;
        a.activate();
        assert_eq!(a.draft.account.as_deref(), Some("codex:second"));
        a.push(Page::Accounts, String::new());
        a.options.boundary(&a.rows(), false, true);
        let mut refreshed = a.accounts.clone();
        refreshed
            .push(serde_json::from_value(json!({"id":"codex:third","selectable":true})).unwrap());
        a.inventory(refreshed, vec![], 0);
        assert_eq!(a.rows()[a.options.selected].label, "Cancel");
        a.activate();
        assert_eq!(a.page, Page::Summary);
        assert_eq!(a.draft.account.as_deref(), Some("codex:second"));
        let session = json!({"id":"stable","generation":7,"name":"Saved","capabilities":{"switch_account":true}});
        a.manager.sessions = vec![session.clone()];
        a.switch = Some(session);
        a.push(Page::Accounts, String::new());
        a.options.selected = 0;
        assert!(a.activate().is_empty());
        assert!(
            a.manager
                .dialog
                .as_ref()
                .unwrap()
                .body
                .join(" ")
                .contains("codex:first")
        );
    }
    #[test]
    fn all_account_statuses_stay_inline_before_and_after_polling() {
        let mut a = app();
        a.accounts = serde_json::from_value(json!([
            {"id":"codex:a","selectable":true,"usage_fresh":true,"usage":{"observed_at":1,"weekly":{"duration_minutes":10080,"used_percent":10}}},
            {"id":"codex:b","selectable":true,"usage_fresh":true,"usage":{"observed_at":1,"weekly":{"duration_minutes":10080,"used_percent":10}}}
        ])).unwrap();
        a.push(Page::Accounts, String::new());
        let mut field = Field::new(100, 30, 1);
        let c = a.paint(&mut field, 0, 100, 30);
        let opened = a.popup_size.as_ref().unwrap().1;
        let lines = |c: &Canvas| {
            c.cells
                .chunks(c.width)
                .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>())
                .collect::<Vec<_>>()
        };
        assert_eq!(lines(&c).iter().filter(|l| l.contains("Ready")).count(), 2);
        let mut polled = a.accounts.clone();
        polled[1]
            .details
            .insert("auth_required".into(), json!(true));
        polled[1].selectable = false;
        a.inventory(polled, vec![], 0);
        let c = a.paint(&mut field, 20, 100, 30);
        assert!(
            lines(&c)
                .iter()
                .any(|l| l.contains("○ b") && l.contains("Login required"))
        );
        assert_eq!(
            (
                a.popup_size.as_ref().unwrap().1.width,
                a.popup_size.as_ref().unwrap().1.height
            ),
            (opened.width, opened.height)
        );
        a.options.selected = a.rows().len() - 1;
        let c = a.paint(&mut field, 40, 100, 30);
        assert!(lines(&c).iter().any(|l| l.contains("Ready")));
        assert!(lines(&c).iter().any(|l| l.contains("Login required")));
    }
    use super::*;
    #[test]
    fn popup_geometry_is_stable_across_focus_status_and_loading_changes() {
        let mut a = app();
        a.accounts =
            vec![serde_json::from_value(json!({"id":"codex:personal","selectable":true})).unwrap()];
        a.spaces = vec![Workspace {
            workspace_id: "w1".into(),
            label: "Work".into(),
            focused: true,
        }];
        a.manager.inventory_known = true;
        for page in [
            Page::Accounts,
            Page::Spaces,
            Page::Roots,
            Page::Name,
            Page::SpaceName,
            Page::Directory(Target::Draft),
        ] {
            a.page = page;
            a.popup_size = None;
            a.pending = false;
            a.error.clear();
            a.options.back = false;
            let mut field = Field::new(100, 30, 2);
            a.paint(&mut field, 0, 100, 30);
            let initial = a.popup_size.as_ref().unwrap().1;
            for back in [true, false] {
                a.options.back = back;
                a.pending = true;
                a.error = "Synthetic status".into();
                a.paint(&mut field, 20, 100, 30);
                let size = a.popup_size.as_ref().unwrap().1;
                assert_eq!((size.width, size.height), (initial.width, initial.height));
            }
            a.paint(&mut field, 40, 40, 14);
            let resized = a.popup_size.as_ref().unwrap().1;
            assert_eq!(resized.viewport, (40, 14));
            assert!(resized.width <= 36 && resized.height <= 10);
        }
        a.page = Page::Accounts;
        a.options.back = true;
        a.pending = false;
        a.error.clear();
        a.popup_size = None;
        let c = a.paint(&mut Field::new(100, 30, 1), 0, 100, 30);
        assert!(a.popup_size.as_ref().unwrap().1.width <= 76);
        assert!(
            c.cells
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
                .contains("Cancel")
        );
    }
    #[test]
    fn unnamed_directory_picker_uses_projects_root_not_host_space_directory() {
        for per_space in [false, true] {
            let mut a = app();
            a.page = Page::Summary;
            a.initial = "/projects/SpaceName".into();
            a.prefs.global_root = Some("/projects".into());
            a.draft.space = Some(Space::Existing {
                id: "w1".into(),
                label: "SpaceName".into(),
            });
            if per_space {
                a.prefs.mode = Mode::PerSpace;
                a.prefs.space_roots.insert(
                    Preferences::key(&a.instance, "w1"),
                    "/space-projects".into(),
                );
            }
            a.options.selected = 3;
            a.activate();
            assert_eq!(
                a.input.value,
                if per_space {
                    "/space-projects"
                } else {
                    "/projects"
                }
            );
            assert_eq!(a.draft.directory, Directory::Unset);
            assert!(a.draft.name.is_empty());
        }
    }
    #[test]
    fn directory_navigation_defaults_and_loading_keep_the_frame_and_actions() {
        let mut a = app();
        a.page = Page::Directory(Target::Draft);
        a.input = TextInput::new("/projects".into());
        a.listing(
            a.revision,
            Ok(Listing {
                path: Some("/projects".into()),
                existing: true,
                children: vec!["/projects/child".into()],
                ..Listing::default()
            }),
        );
        assert_eq!(a.options.selected, a.directory_children_offset());
        let mut field = Field::new(100, 30, 7);
        a.paint(&mut field, 0, 100, 30);
        let size = a.popup_size.as_ref().unwrap().1;
        a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        let c = a.paint(&mut field, 20, 100, 30);
        let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
        assert!(
            text.contains("[use this directory]")
                && text.contains("<new>/")
                && text.contains("Loading…")
        );
        assert!(!text.contains("child/"));
        let lines: Vec<_> = c
            .cells
            .chunks(100)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>())
            .collect();
        let create = lines.iter().position(|s| s.contains("<new>/")).unwrap();
        let loading = lines.iter().position(|s| s.contains("Loading…")).unwrap();
        assert_eq!(loading, create + 2);
        let cancel = lines.iter().position(|s| s.contains("Cancel")).unwrap();
        assert!(cancel > loading);
        assert_eq!(cancel, (30 - size.height) / 2 + size.height - 3);
        assert_eq!(a.popup_size.as_ref().unwrap().1.height, size.height);
        a.listing(
            a.revision,
            Ok(Listing {
                path: Some("/projects/child".into()),
                existing: true,
                children: vec![],
                ..Listing::default()
            }),
        );
        assert!(!a.listing_pending);
        assert_eq!(a.rows()[a.options.selected].label, "(none)");
        assert!(a.activate().is_empty());
        a.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        a.listing(
            a.revision,
            Ok(Listing {
                path: Some("/projects".into()),
                existing: true,
                children: vec!["/projects/a".into(), "/projects/child".into()],
                ..Listing::default()
            }),
        );
        assert_eq!(a.rows()[a.options.selected].label, "child/");
        a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        a.history.insert("/projects/child".into(), 1);
        a.listing(
            a.revision,
            Ok(Listing {
                path: Some("/projects/child".into()),
                existing: true,
                children: vec!["/projects/child/inner".into()],
                ..Listing::default()
            }),
        );
        assert_eq!(a.options.selected, 1);
    }
    #[test]
    fn directory_actions_separate_children_and_empty_directories_have_parent_action() {
        for target in [Target::Draft, Target::Global] {
            let mut a = app();
            a.page = Page::Directory(target);
            a.input = TextInput::new("/projects/empty".into());
            a.listing = Listing {
                path: Some("/projects/empty".into()),
                existing: true,
                ..Listing::default()
            };
            let parent = a.directory_children_offset();
            assert_eq!(a.rows()[parent].label, "(none)");
            assert!(!a.rows()[parent].enabled);
            assert_eq!(a.rows()[parent - 1].label, "<new>/");
            assert_eq!(a.rows().last().unwrap().label, "Cancel");
            assert!(a.rows()[a.rows().len() - 2].separator);
            a.options.selected = parent;
            assert!(!a.control_tokens().contains(&"◀/▶: Parent/Open"));
            assert!(
                a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
                    .is_empty()
            );
            assert_eq!(a.input.value, "/projects/empty");
            assert!(a.activate().is_empty());
            assert!(
                matches!(&a.key(KeyEvent::new(KeyCode::Left,KeyModifiers::NONE))[..], [Effect::Inspect(_, p)] if p == "/projects")
            );
            assert_eq!(
                a.pending_child.as_deref(),
                Some(Path::new("/projects/empty"))
            );
            a.listing = Listing {
                path: Some("/projects".into()),
                existing: true,
                children: vec!["/projects/empty".into()],
                ..Listing::default()
            };
            let rows = a.rows();
            assert_eq!(rows[parent - 1].label, "<new>/");
            assert_eq!(rows[parent].label, "empty/");
            a.options.selected = parent - 1;
            a.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
            assert_eq!(a.options.selected, parent);
            assert!(a.control_tokens().contains(&"◀/▶: Parent/Open"));
            a.options.back = true;
            assert!(!a.control_tokens().contains(&"◀/▶: Parent/Open"));
            a.listing.path = Some("/".into());
            a.listing.children.clear();
            assert!(!a.rows().iter().any(|r| r.label == "Go to Parent"));
        }
    }
    #[test]
    fn editor_popup_keeps_summary_and_native_cursor_then_restores_selection() {
        let mut a = app();
        a.page = Page::Summary;
        a.options.selected = 2;
        let mut field = Field::new(100, 32, 2);
        let before = a.paint(&mut field, 0, 100, 32);
        a.activate();
        assert_eq!(a.page, Page::Name);
        let popup = a.paint(&mut field, 20, 100, 32);
        assert_eq!(&popup.cells[..200], &before.cells[..200]);
        assert!(popup.cursor.is_some());
        assert!(
            popup
                .cells
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
                .contains("┌Session name")
        );
        for (w, h) in [(20, 8), (8, 4), (100, 32)] {
            let frame = a.paint(&mut field, 40, w, h);
            assert_eq!(frame.cells.len(), w * h);
            assert!(frame.cursor.is_none_or(|(x, y)| x < w && y < h));
        }
        a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(a.page, Page::Summary);
        assert_eq!(a.options.selected, 2);
        assert!(a.paint(&mut field, 60, 100, 32).cursor.is_none());
    }
    #[test]
    fn back_restores_launcher_options_and_picker_scroll() {
        let mut a = app();
        a.page = Page::Summary;
        a.options.selected = 3;
        a.activate();
        assert_eq!(a.page, Page::Directory(Target::Draft));
        a.options.selected = 1;
        a.options.scroll = 7;
        a.push(
            Page::NewDirectory(Target::Draft, "/tmp".into()),
            String::new(),
        );
        a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(a.page, Page::Directory(Target::Draft));
        assert_eq!(a.options.selected, 1);
        assert_eq!(a.options.scroll, 7);
        a.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        a.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(a.page, Page::Summary);
        assert_eq!(a.options.selected, 3);
    }
    #[test]
    fn chooser_ready_uses_flat_popup_accent_and_other_statuses_stay_gray() {
        for switching in [false, true] {
            let mut a = app();
            a.page = Page::Accounts;
            a.switch = switching.then(|| json!({"id":"session"}));
            a.options.selected = 0;
            a.accounts=vec![serde_json::from_value(json!({"id":"codex:demo","selectable":true,"usage_fresh":true,"usage":{"observed_at":1,"short":{"duration_minutes":300,"used_percent":20},"weekly":{"duration_minutes":10080,"used_percent":10}}})).unwrap()];
            let mut field = Field::new(80, 24, 1);
            let c = a.paint(&mut field, 4200, 80, 24);
            let row = &c.cells[..];
            let x = row.iter().position(|c| c.glyph == "R").unwrap();
            assert!(
                row[x..x + 5]
                    .iter()
                    .all(|c| !c.bold && c.fg != Cell::default().fg)
            );
            a.accounts[0].details.insert("enabled".into(), json!(false));
            let c = a.paint(&mut field, 4300, 80, 24);
            let row = &c.cells[..];
            let x = row.iter().position(|c| c.glyph == "D").unwrap();
            assert!(
                row[x..x + 8]
                    .iter()
                    .all(|c| !c.bold && c.fg == Cell::default().fg)
            );
        }
    }
    #[test]
    fn repeated_arrows_stop_and_a_released_press_wraps() {
        use crossterm::event::KeyEventKind;
        let mut app = app();
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let mut repeat = down;
        repeat.kind = KeyEventKind::Repeat;
        let mut release = down;
        release.kind = KeyEventKind::Release;
        app.key(down);
        for _ in 0..20 {
            app.key(repeat);
        }
        assert_eq!(app.manager.options.selected, 4);
        app.key(release);
        app.key(down);
        assert_eq!(app.manager.options.selected, 0);
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let mut repeat_up = up;
        repeat_up.kind = KeyEventKind::Repeat;
        for _ in 0..20 {
            app.key(repeat_up);
        }
        assert_eq!(app.manager.options.selected, 0);
        let mut release_up = up;
        release_up.kind = KeyEventKind::Release;
        app.key(release_up);
        app.key(up);
        assert_eq!(app.manager.options.selected, 4);
    }
    #[test]
    fn home_last_account_wraps_on_fresh_down_and_hold_uses_its_origin() {
        use crossterm::event::KeyEventKind;
        let mut app = app();
        app.inventory(
            vec![
                serde_json::from_value(
                    json!({"id":"codex:work", "enabled":true, "selectable":true, "usage":{}}),
                )
                .unwrap(),
            ],
            vec![],
            100,
        );
        app.manager
            .paint(&mut crate::render::Canvas::new(100, 40), 39);
        assert!(app.manager.inline);
        let last = app.manager.rows().len() - 1;
        assert_eq!(last, 5);
        app.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        assert_eq!(app.manager.options.selected, last);
        app.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        assert_eq!(app.manager.options.selected, 0);
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let repeat = |mut key: KeyEvent| {
            key.kind = KeyEventKind::Repeat;
            key
        };
        let release = |mut key: KeyEvent| {
            key.kind = KeyEventKind::Release;
            key
        };
        app.manager.options.selected = last - 1;
        app.key(down);
        for _ in 0..20 {
            app.key(repeat(down));
        }
        assert_eq!(app.manager.options.selected, last);
        app.key(release(down));
        app.key(down);
        assert_eq!(app.manager.options.selected, 0);
        app.key(release(down));
        app.key(up);
        assert_eq!(app.manager.options.selected, last);
        app.key(repeat(up));
        assert_eq!(app.manager.options.selected, last - 1);
    }
    fn app() -> App {
        App::new(
            Preferences::default(),
            "/tmp".into(),
            "test".into(),
            json!({}),
            None,
        )
    }
    #[test]
    fn topbar_roots_and_summary_share_angle_bracket_unset_placeholders() {
        let mut a = app();
        a.prefs.global_root = Some("/global-projects".into());
        a.prefs
            .space_roots
            .insert(Preferences::key("test", "space"), "/space-projects".into());
        a.draft.space = Some(Space::Existing {
            id: "space".into(),
            label: "Space".into(),
        });
        for (mode, expected) in [
            (Mode::Global, "/global-projects"),
            (Mode::PerSpace, "/space-projects"),
        ] {
            a.prefs.mode = mode;
            let c = a.paint(&mut Field::new(120, 30, 1), 0, 120, 30);
            let line = c.cells[..120]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>();
            assert!(line.contains(expected), "{line}");
            assert!(!line.contains("/tmp"));
        }
        a.draft.space = None;
        let c = a.paint(&mut Field::new(120, 30, 1), 0, 120, 30);
        assert!(
            c.cells[..120]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
                .contains("<unset>")
        );
        a.page = Page::Summary;
        a.draft = Draft::default();
        assert!(a.rows().iter().all(|row| row.label.ends_with("<unset>")));
        a.prefs.global_root = None;
        a.page = Page::Config;
        assert_eq!(a.rows()[1].preview.as_deref(), Some("<unset>"));
        a.page = Page::Roots;
        a.spaces = vec![Workspace {
            workspace_id: "missing".into(),
            label: "No root".into(),
            focused: false,
        }];
        assert_eq!(a.rows()[0].preview.as_deref(), Some("<unset>"));
    }
    #[test]
    fn shared_directory_warns_on_hover_and_requires_confirmation_without_mutation() {
        let mut a = app();
        a.page = Page::Summary;
        a.launch_supported = true;
        a.manager.sessions_known = true;
        a.manager.sessions =
            vec![json!({"id":"old", "directory":"/shared", "availability":"saved"})];
        a.draft = Draft {
            account: Some("codex:personal".into()),
            space: Some(Space::Existing {
                id: "s".into(),
                label: "Space".into(),
            }),
            name: "Another".into(),
            directory: Directory::Existing("/shared".into()),
            name_valid: true,
            directory_valid: true,
            revision: 1,
        };
        for selected in [0, 4] {
            a.options.selected = selected;
            let c = a.paint(&mut Field::new(120, 30, 1), 0, 120, 30);
            let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
            assert!(text.contains("Start Session ⚠"));
            assert_eq!(text.contains("(existing session in dir)"), selected == 4);
        }
        assert!(a.activate().is_empty());
        assert!(a.request.is_none());
        assert!(a.start_confirmation.is_some());
        assert!(
            a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .is_empty()
        );
        assert!(a.request.is_none());
        assert_eq!(a.options.selected, 4);
        a.activate();
        a.paint(&mut Field::new(60, 22, 1), 0, 60, 22);
        a.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        assert!(matches!(
            a.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                .as_slice(),
            [Effect::Start(_)]
        ));
        assert!(a.start_confirmation.is_none());
        assert!(
            a.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                .is_empty()
        );
        a.resume = Some(json!({"id":"old"}));
        assert!(!a.shared_directory());
    }
    #[test]
    fn picker_columns_are_adjacent_and_all_statuses_stay_on_their_rows() {
        let mut a = app();
        a.page = Page::Accounts;
        a.options.selected = 0;
        let account = |name: &str, short: bool| {
            serde_json::from_value(json!({"id":format!("codex:{name}"),"selectable":true,"usage_fresh":true,"usage":{"observed_at":1,"short":if short{json!({"duration_minutes":300,"used_percent":20})}else{Value::Null},"weekly":{"duration_minutes":10080,"used_percent":10}}})).unwrap()
        };
        a.accounts = vec![account("longest", true), account("tiny", false)];
        let rows = a.rows();
        for width in [60, 120] {
            let c = a.paint(&mut Field::new(width, 24, 1), 0, width, 24);
            let line = |y: usize| {
                c.cells[y * width..(y + 1) * width]
                    .iter()
                    .map(|c| c.glyph.as_str())
                    .collect::<String>()
            };
            let ready = (0..24).find(|y| line(*y).contains("Ready")).unwrap();
            assert!(line(ready).contains("longest"));
            let inner = width.saturating_sub(4).min(76) - 4;
            let left = Options::left(inner, &rows);
            assert_eq!(rows[0].preview_position(inner, left).unwrap(), left + 10);
            assert_eq!(
                rows[0].preview_position(inner, left).unwrap()
                    + rows[0].preview.as_ref().unwrap().len(),
                rows[1].preview_position(inner, left).unwrap()
                    + rows[1].preview.as_ref().unwrap().len()
            );
        }
        a.accounts = (0..30)
            .map(|i| account(&format!("acct{i}"), true))
            .collect();
        let c = a.paint(&mut Field::new(60, 18, 1), 0, 60, 18);
        let lines: Vec<_> = c
            .cells
            .chunks(60)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>())
            .collect();
        let range = lines.iter().position(|l| l.contains(" of ")).unwrap();
        assert!(range > 0);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("acct0") && l.contains("Ready"))
        );
        let narrow = a.paint(&mut Field::new(36, 20, 1), 0, 36, 20);
        let lines: Vec<String> = narrow
            .cells
            .chunks(36)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect())
            .collect();
        let range = lines.iter().position(|l| l.contains(" of ")).unwrap();
        assert!(!lines[range + 2].contains("Ready"));
        assert!(lines.iter().any(|l| l.contains("Ready")));
        let tiny = a.paint(&mut Field::new(24, 20, 1), 0, 24, 20);
        let tiny_lines: Vec<String> = tiny
            .cells
            .chunks(24)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect())
            .collect();
        assert!(tiny_lines.iter().any(|l| l.contains("Ready")));
        assert!(
            tiny_lines
                .iter()
                .any(|l| l.contains("acct0") && l.contains("Ready"))
        );
        a.options.selected = a.rows().len() - 1;
        let c = a.paint(&mut Field::new(60, 18, 1), 0, 60, 18);
        assert!(
            c.cells
                .iter()
                .map(|c| c.glyph.as_str())
                .collect::<String>()
                .contains("Ready")
        );
    }
    #[test]
    fn background_resume_returns_to_selected_session_without_replaying() {
        let mut a = app();
        a.manager.sessions = vec![json!({"id":"one"}), json!({"id":"two"})];
        a.resume = Some(json!({"id":"two"}));
        a.launch_dialog = true;
        a.manager.dialog = Some(crate::dialog::Dialog::progress("Starting session"));
        let effects = a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(effects.is_empty());
        assert_eq!(a.page, Page::Management);
        assert_eq!(a.manager.page, crate::management::Page::Sessions);
        assert_eq!(a.manager.options.selected, 1);
        assert!(a.launch_background);
    }
    #[test]
    fn pending_receipt_freshness_uses_plain_language() {
        assert_eq!(
            reconciliation_message("SESSION_STATE_UNVERIFIED"),
            "Checking session state…"
        );
        assert_eq!(
            reconciliation_message("STALE_TARGET: changed"),
            "Checking session state…"
        );
        assert!(reconciliation_message("PERMISSION_DENIED").contains("permissions"));
    }
    #[test]
    fn creation_and_resume_account_pickers_use_standard_bullets() {
        let mut a = app();
        a.page = Page::Accounts;
        a.accounts = serde_json::from_value(json!([
            {"id":"codex:personal","selectable":true},
            {"id":"codex:work","selectable":true}
        ]))
        .unwrap();
        for resume in [None, Some(json!({"id":"saved"}))] {
            a.resume = resume;
            let rows = a.rows();
            for (index, row) in rows.iter().enumerate().filter(|(_, r)| !r.separator) {
                assert!(!row.account);
                let mut canvas = Canvas::new(80, 4);
                row.paint(&mut canvas, 0, 0, index == 0, false);
                assert_eq!(canvas.cells[0].glyph, if index == 0 { "●" } else { "○" });
                assert_eq!(
                    canvas.cells[2].fg,
                    if index == 0 { [238; 3] } else { [188; 3] }
                );
                assert!(!canvas.cells.iter().any(|c| c.glyph == "▶"));
            }
        }
    }
    #[test]
    fn new_form_and_picker_create_are_staged() {
        let mut a = app();
        a.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(a.page, Page::Summary);
        a.page = Page::NewDirectory(Target::Draft, "/tmp".into());
        a.input.value = "not-created-by-picker".into();
        a.listing.creatable = true;
        a.stack = vec![
            Saved {
                page: Page::Summary,
                options: Options {
                    selected: 3,
                    ..Default::default()
                },
                text: String::new(),
                listing: Listing::default(),
                listing_pending: false,
                popup_size: None,
            },
            Saved {
                page: Page::Directory(Target::Draft),
                options: Options {
                    selected: 1,
                    ..Default::default()
                },
                text: "/tmp".into(),
                listing: Listing::default(),
                listing_pending: false,
                popup_size: None,
            },
        ];
        let effects = a.activate();
        assert!(effects.iter().all(|e| !matches!(e, Effect::Start(_))));
        assert!(matches!(a.draft.directory, Directory::Create { .. }));
        assert_eq!(a.page, Page::Summary);
    }
    #[test]
    fn paste_is_text_not_activation() {
        let mut a = app();
        a.page = Page::Name;
        let effects = a.paste("hello\nworld\x1b");
        assert!(effects.is_empty());
        assert_eq!(a.input.value, "helloworld");
        assert_eq!(a.page, Page::Name);
    }
    #[test]
    fn stale_validation_cannot_enable_start() {
        let mut a = app();
        a.draft.revision = 2;
        a.validation(1, Ok(()));
        assert!(!a.draft.name_valid);
    }
    #[test]
    fn missing_per_space_root_does_not_use_global() {
        let mut a = app();
        a.prefs.global_root = Some("/global".into());
        a.prefs.mode = Mode::PerSpace;
        a.draft.name = "name".into();
        a.revalidate();
        assert_eq!(a.draft.directory, Directory::Unset);
    }
    #[test]
    fn new_directory_edit_waits_for_matching_read_only_validation() {
        let mut a = app();
        a.page = Page::NewDirectory(Target::Global, "/tmp".into());
        let effects = a.paste("planned-root");
        assert!(matches!(&effects[..],[Effect::Inspect(_,path)] if path=="/tmp/planned-root"));
        assert!(!a.rows()[0].enabled);
        a.listing(
            a.revision,
            Ok(Listing {
                creatable: true,
                ..Listing::default()
            }),
        );
        assert!(a.rows()[0].enabled);
        a.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(a.input.value, "planned-root");
        a.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(!a.rows()[0].enabled);
        a.listing(
            a.revision - 1,
            Ok(Listing {
                creatable: true,
                ..Listing::default()
            }),
        );
        assert!(!a.rows()[0].enabled);
    }
    #[test]
    fn refresh_retains_selected_default_until_authoritatively_invalid() {
        let mut a = app();
        a.prefs.last_account = Some("codex:work".into());
        let account = |selectable, eligible, valid_until| {
            serde_json::from_value(json!({
                "id":"codex:work", "selectable":selectable,
                "prefill":{"eligible":eligible,"valid_until":valid_until}, "usage":{}
            }))
            .unwrap()
        };
        a.inventory(vec![account(true, true, 105)], vec![], 100);
        assert_eq!(a.draft.account.as_deref(), Some("codex:work"));
        // Replacement arrives after the five-second prefill lease; stale usage
        // must not erase a choice or silently select a different account.
        a.inventory(vec![account(true, false, 105)], vec![], 107);
        assert_eq!(a.draft.account.as_deref(), Some("codex:work"));
        a.inventory(vec![account(false, false, 110)], vec![], 108);
        assert!(a.draft.account.is_none());
    }
    #[test]
    fn directory_right_and_tab_do_not_browse_from_action_rows() {
        let mut a = app();
        a.page = Page::Directory(Target::Draft);
        a.input = TextInput::new("/tmp".into());
        a.listing = Listing {
            path: Some("/tmp".into()),
            existing: true,
            children: vec!["/tmp/child".into()],
            ..Listing::default()
        };
        for row in [0, 1] {
            for code in [KeyCode::Right, KeyCode::Tab] {
                a.options.selected = row;
                assert!(a.key(KeyEvent::new(code, KeyModifiers::NONE)).is_empty());
                assert_eq!(a.input.value, "/tmp");
                assert_eq!(a.options.selected, row);
            }
        }
        a.options.selected = a.directory_children_offset();
        assert!(
            a.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
                .is_empty()
        );
        assert_eq!(a.input.value, "/tmp");
        assert!(
            matches!(&a.key(KeyEvent::new(KeyCode::Right,KeyModifiers::NONE))[..],[Effect::Inspect(_,p)] if p=="/tmp/child")
        );
    }
    #[test]
    fn default_directory_actions_clear_only_the_selected_root() {
        for target in [Target::Global, Target::Space("w1".into())] {
            let mut a = app();
            let space_key = Preferences::key(&a.instance, "w1");
            a.prefs.global_root = Some("/tmp/global".into());
            a.prefs
                .space_roots
                .insert(space_key.clone(), "/tmp/space".into());
            a.prefs
                .root_plans
                .insert("global".into(), vec!["/tmp/global".into()]);
            a.prefs
                .root_plans
                .insert(space_key.clone(), vec!["/tmp/space".into()]);
            a.push(Page::Directory(target.clone()), "/tmp".into());
            a.listing = Listing {
                path: Some("/tmp".into()),
                existing: true,
                children: vec!["/tmp/child".into()],
                ..Listing::default()
            };
            assert_eq!(a.rows()[0].label, "[use this directory]");
            assert_eq!(a.rows()[1].label, "[leave unset]");
            assert_eq!(a.rows()[2].label, "<new>/");
            for row in 0..3 {
                a.options.selected = row;
                a.options.back = false;
                assert!(
                    a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
                        .is_empty()
                );
                assert_eq!(a.input.value, "/tmp");
                a.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
                assert!(!a.options.back);
            }
            a.options.back = false;
            a.options.selected = 1;
            a.activate();
            match target {
                Target::Global => {
                    assert!(a.prefs.global_root.is_none());
                    assert!(!a.prefs.root_plans.contains_key("global"));
                    assert!(a.prefs.space_roots.contains_key(&space_key));
                }
                Target::Space(_) => {
                    assert!(!a.prefs.space_roots.contains_key(&space_key));
                    assert!(!a.prefs.root_plans.contains_key(&space_key));
                    assert!(a.prefs.global_root.is_some());
                }
                _ => unreachable!(),
            }
        }
    }
    #[test]
    fn unavailable_options_can_be_hovered_but_not_activated() {
        let mut a = app();
        a.page = Page::Config;
        a.prefs.mode = Mode::PerSpace;
        a.options.selected = 0;
        a.options.step(&a.rows(), false, false);
        assert_eq!(a.options.selected, 1);
        assert!(!a.rows()[1].enabled);
        assert!(a.activate().is_empty());
        assert_eq!(a.page, Page::Config);
        assert!(a.rows()[0].label.starts_with("Default Dir:"));
    }
    #[test]
    fn root_paths_use_bare_gray_selected_previews_with_one_space_and_global_fit() {
        let mut a = app();
        a.prefs.global_root = Some("/projects/global".into());
        a.page = Page::Config;
        let row = a.rows().remove(1);
        assert_eq!(row.preview.as_deref(), Some("/projects/global"));
        assert_eq!(row.preview_gap, 1);
        assert_eq!(row.preview_fg, crate::widgets::SECONDARY);
        assert!(row.preview_selected_only);
        a.page = Page::Roots;
        a.spaces = vec![
            Workspace {
                workspace_id: "w1".into(),
                label: "Alpha".into(),
                focused: false,
            },
            Workspace {
                workspace_id: "w2".into(),
                label: "Beta".into(),
                focused: false,
            },
        ];
        for (id, path) in [
            ("w1", "/short"),
            ("w2", "/projects/a/path/too/long/to/fit/inline"),
        ] {
            a.prefs
                .space_roots
                .insert(Preferences::key(&a.instance, id), path.into());
        }
        let rows = a.rows();
        assert!(Options::previews_detached(36, &rows));
        for width in [36, 120] {
            a.options.selected = 0;
            let mut field = Field::new(width, 24, 1);
            let c = a.paint(&mut field, 1000, width, 24);
            let start = 3 * width
                + c.cells[3 * width..20 * width]
                    .iter()
                    .position(|c| c.glyph == "/")
                    .unwrap();
            assert_eq!(
                c.cells[start..start + 6]
                    .iter()
                    .map(|c| c.glyph.as_str())
                    .collect::<String>(),
                "/short"
            );
            assert!(
                c.cells[start..start + 6]
                    .iter()
                    .all(|c| c.fg == crate::widgets::SECONDARY && !c.bold)
            );
            if Options::previews_detached(a.popup_size.as_ref().unwrap().1.width - 4, &rows) {
                let alpha = c
                    .cells
                    .chunks(width)
                    .position(|r| {
                        r.iter()
                            .map(|c| c.glyph.as_str())
                            .collect::<String>()
                            .contains("Alpha")
                    })
                    .unwrap();
                assert_eq!(start / width, alpha + rows.len() + 1);
            } else {
                let panel_width = a.popup_size.as_ref().unwrap().1.width;
                let inner = panel_width - 4;
                let left = Options::left(inner, &rows);
                assert_eq!(
                    start % width,
                    (width - panel_width) / 2 + 2 + left + 2 + "Alpha".len() + 1
                );
            }
            a.options.selected = a.rows().len() - 1;
            let c = a.paint(&mut field, 1050, width, 24);
            assert!(
                !c.cells
                    .iter()
                    .map(|c| c.glyph.as_str())
                    .collect::<String>()
                    .contains("/short")
            );
            a.options.back = false;
        }
    }
    #[test]
    fn space_ids_use_shared_selected_previews_and_global_detached_fit() {
        let mut a = app();
        a.page = Page::Spaces;
        a.spaces = vec![
            Workspace {
                workspace_id: "w1".into(),
                label: "Short".into(),
                focused: false,
            },
            Workspace {
                workspace_id: "workspace-with-a-long-id".into(),
                label: "Longer 界 Space".into(),
                focused: false,
            },
        ];
        let rows = a.rows();
        assert!(rows.iter().all(|row| row.preview_column.is_none()));
        assert_eq!(rows[0].label, "[create new space]");
        assert_eq!(rows[1].preview.as_deref(), Some("(w1)"));
        for selected in [1, 2] {
            a.options.selected = selected;
            let mut field = Field::new(100, 24, 1);
            let c = a.paint(&mut field, 1000, 100, 24);
            let index = c.cells.iter().position(|c| c.glyph == "(").unwrap();
            let size = a.popup_size.as_ref().unwrap().1;
            let inner = size.width - 4;
            let origin_x = (100 - size.width) / 2 + 2;
            let origin_y = (24 - size.height) / 2 + 1;
            let option_top = crate::dialog::wrap(a.popup_description(), inner).len() + 1;
            if Options::previews_detached(inner, &rows) {
                assert_eq!(index / 100, origin_y + option_top + 1 + rows.len());
                assert_eq!(
                    index % 100,
                    origin_x + (inner - rows[selected].preview.as_ref().unwrap().len()) / 2
                );
            } else {
                assert_eq!(index / 100, origin_y + option_top + selected);
                assert_eq!(
                    Some(index % 100),
                    rows[selected]
                        .preview_position(inner, Options::left(inner, &rows))
                        .map(|x| x + origin_x)
                );
            }
            assert_eq!(c.cells[index].fg, crate::widgets::SECONDARY);
            assert!(!c.cells[index].bold);
            assert_eq!(c.cells.iter().filter(|c| c.glyph == "(").count(), 1);
        }
        assert!(Options::previews_detached(36, &rows));
        a.options.selected = 1;
        let mut field = Field::new(36, 24, 1);
        let c = a.paint(&mut field, 1000, 36, 24);
        let index = c.cells.iter().position(|c| c.glyph == "(").unwrap();
        let first_row = c
            .cells
            .chunks(36)
            .position(|r| {
                r.iter()
                    .map(|c| c.glyph.as_str())
                    .collect::<String>()
                    .contains("[create new space]")
            })
            .unwrap();
        assert_eq!(index / 36, first_row + rows.len() + 1);
        let size = a.popup_size.as_ref().unwrap().1;
        assert_eq!(
            index % 36,
            (36 - size.width) / 2 + 2 + (size.width - 4 - "(w1)".len()) / 2
        );
        a.options.selected = a.rows().len() - 1;
        let c = a.paint(&mut field, 1050, 36, 24);
        assert!(c.cells.iter().all(|c| c.glyph != "("));
        a.options.back = false;
        a.options.selected = 0;
        let c = a.paint(&mut field, 1100, 36, 24);
        assert!(c.cells.iter().all(|c| c.glyph != "("));
    }
    #[test]
    fn space_defaults_to_exact_focused_id_and_never_to_the_first_space() {
        let mut a = app();
        let spaces = vec![
            Workspace {
                workspace_id: "w1".into(),
                label: "First".into(),
                focused: false,
            },
            Workspace {
                workspace_id: "w2".into(),
                label: "Active".into(),
                focused: true,
            },
        ];
        a.inventory(vec![], spaces.clone(), 0);
        assert!(matches!(&a.draft.space,Some(Space::Existing {id,..}) if id=="w2"));
        a.push(Page::Spaces, String::new());
        assert_eq!(a.options.selected, 2);
        let mut reordered = spaces.clone();
        reordered.reverse();
        a.inventory(vec![], reordered, 0);
        assert_eq!(a.options.selected, 1);
        let none = spaces
            .iter()
            .cloned()
            .map(|mut s| {
                s.focused = false;
                s
            })
            .collect();
        a.inventory(vec![], none, 0);
        assert_eq!(a.draft.space, None);
        a.inventory(vec![], spaces.clone(), 0);
        a.options.selected = 1;
        a.activate();
        a.inventory(vec![], spaces, 0);
        assert!(matches!(&a.draft.space,Some(Space::Existing {id,..}) if id=="w1"));
    }
    #[test]
    fn session_editor_stops_at_thirty_and_does_not_limit_other_editors() {
        let mut a = app();
        a.push(Page::Name, String::new());
        a.paste("abcdefghijklmnopqrstuvwxyz123456789");
        assert_eq!(a.input.value, "abcdefghijklmnopqrstuvwxyz1234");
        assert!(a.name_valid(&a.input.value));
        assert!(!a.name_valid("abcdefghijklmnopqrstuvwxyz12345"));
        a.push(Page::SpaceName, String::new());
        a.paste("a longer workspace name");
        assert_eq!(a.input.value, "a longer workspace name");
        a.pop();
        a.paste("ignored");
        assert_eq!(a.input.value, "abcdefghijklmnopqrstuvwxyz1234");
    }
    #[test]
    fn summary_start_is_adjacent_to_fields_and_home_control_says_close() {
        let mut a = app();
        a.page = Page::Summary;
        for (w, h) in [(80, 24), (140, 40)] {
            let mut f = Field::new(w, h, 1967);
            let c = a.paint(&mut f, 0, w, h);
            let row = |y: usize| {
                c.cells[y * w..(y + 1) * w]
                    .iter()
                    .map(|c| c.glyph.as_str())
                    .collect::<String>()
            };
            assert!(row(6).contains("Dir:"));
            let labels = ["Account:", "Space:", "Name:", "Dir:"];
            let starts = (3..7)
                .zip(labels)
                .map(|(y, label)| {
                    let text = row(y);
                    unicode_width::UnicodeWidthStr::width(&text[..text.find(label).unwrap()])
                })
                .collect::<Vec<_>>();
            assert!(starts.iter().all(|x| *x == starts[0]));
            assert!(a.rows().iter().all(|r| r.label.as_bytes()[8] == b' '));
            assert!(row(7).contains('•'));
            assert!(row(8).contains("Start Session"));
        }
        a.page = Page::Home;
        a.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        let c = a.paint(&mut Field::new(80, 24, 1967), 0, 80, 24);
        let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
        assert!(text.contains("◀— Close"));
        assert!(!text.contains("◀— Back"));
        assert!(matches!(
            &a.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))[..],
            [Effect::Exit]
        ));
    }
}

#[cfg(test)]
mod resume_tests {
    use super::*;
    use crate::management::Intent;
    #[test]
    fn resume_pending_or_stale_checks_do_not_trap_valid_choices() {
        let mut a = App::new(
            Preferences::default(),
            "/projects".into(),
            "host".into(),
            json!({}),
            None,
        );
        a.manager.capabilities = json!({"resume":{"version":1}});
        a.management(vec![Intent::Resume(json!({"id":"stable","generation":"old","name":"Saved","account_id":"codex:a","directory":"/saved"}))]);
        a.draft.space = Some(Space::New {
            label: "Chosen Space".into(),
        });
        a.revalidate();
        assert!(a.pending);
        assert!(a.ready());
        let revision = a.draft.revision;
        a.validation(revision, Err("SESSION_STATE_UNVERIFIED".into()));
        assert!(a.ready());
        assert!(a.error.is_empty());
        assert_eq!(
            resume_explanation(a.resume_validation.as_deref().unwrap()),
            "(recheck needed)"
        );
        a.validation(revision, Err("ACCOUNT_UNAVAILABLE".into()));
        assert!(!a.ready());
        a.revalidate();
        a.validation(revision, Err("ACCOUNT_UNAVAILABLE".into()));
        assert!(a.ready(), "old validation cannot invalidate edited choices");
        a.options.selected = 4;
        assert!(matches!(a.activate().as_slice(), [Effect::Start(_)]));
    }
    #[test]
    fn late_unfinished_warning_preserves_destination_and_requires_consent() {
        let mut a = App::new(
            Preferences::default(),
            "/projects".into(),
            "host".into(),
            json!({}),
            None,
        );
        a.manager.capabilities = json!({"resume":{"version":1,"directory_selection":true}});
        a.management(vec![Intent::Resume(json!({"id":"stable","generation":"g","name":"Saved","account_id":"codex:a","directory":"/missing"}))]);
        a.draft.space = Some(Space::New {
            label: "Chosen".into(),
        });
        a.draft.directory = Directory::Existing("/chosen".into());
        a.revalidate();
        a.validation(
            a.draft.revision,
            Err("UNFINISHED_HISTORY_CONFIRMATION_REQUIRED".into()),
        );
        a.options.selected = 4;
        assert!(a.activate().is_empty());
        assert!(a.start_confirmation.is_some());
        assert!(
            a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .is_empty()
        );
        assert!(a.request.is_none());
        a.activate();
        a.paint(&mut Field::new(80, 24, 1), 0, 80, 24);
        a.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        let effects = a.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let Effect::Start(request) = &effects[0] else {
            panic!()
        };
        assert_eq!(request["confirm_unfinished"], true);
        assert_eq!(request["space"]["label"], "Chosen");
        assert_eq!(request["directory"]["path"], "/chosen");
    }
    #[test]
    fn resume_preserves_draft_and_gates_directory_selection_on_backend_support() {
        let mut a = App::new(
            Preferences::default(),
            "/projects".into(),
            "host".into(),
            json!({"session":"host","socket":"/socket"}),
            None,
        );
        a.draft.name = "New draft".into();
        a.draft.revision = 40;
        a.spaces = vec![
            Workspace {
                workspace_id: "w1".into(),
                label: "Current".into(),
                focused: true,
            },
            Workspace {
                workspace_id: "w2".into(),
                label: "Original".into(),
                focused: false,
            },
        ];
        let details = json!({"id":"stable","generation":"fresh","name":"Saved/name","account_id":"codex:original","directory":"/saved/path","original_host":{"session":"host","socket":"/socket","workspace_id":"w2"}});
        let effects = a.management(vec![Intent::Resume(details.clone())]);
        assert!(effects.iter().any(|e| matches!(e, Effect::Validate(..))));
        assert!(!a.rows()[3].enabled);
        assert_eq!(a.draft.space.as_ref().unwrap().label(), "Original");
        a.draft.name = "Changed/name".into();
        a.revalidate();
        let request = a.creation_request().unwrap();
        assert!(request.get("directory").is_none());
        assert_eq!(request["session_id"], "stable");
        assert_eq!(request["expected_generation"], "fresh");
        assert!(request.get("confirm_unfinished").is_none());
        a.resume.as_mut().unwrap()["confirm_unfinished"] = json!(true);
        assert_eq!(a.creation_request().unwrap()["confirm_unfinished"], true);
        assert_eq!(a.draft.directory.path(), Some(Path::new("/saved/path")));
        a.manager.capabilities["resume"]["directory_selection"] = json!(true);
        assert!(a.rows()[3].enabled);
        assert_eq!(
            a.creation_request().unwrap()["directory"],
            json!({"path":"/saved/path","create":[],"confirmed_existing":true}),
            "the displayed directory must remain explicit even if the saved location changes without an attachment generation change"
        );
        let tmp = tempfile::tempdir().unwrap();
        let destination = tmp.path().join("chosen");
        a.draft.directory = Directory::Create {
            path: destination.clone(),
            planned: vec![destination.clone()],
        };
        let request = a.creation_request().unwrap();
        assert_eq!(request["directory"]["path"], json!(destination));
        assert_eq!(request["directory"]["create"], json!([destination]));
        assert_eq!(request["directory"]["confirmed_existing"], false);
        assert!(
            !destination.exists(),
            "planning a resume must not create its directory"
        );
        let revision = a.draft.revision;
        a.pop();
        assert_eq!(a.draft.name, "New draft");
        assert!(a.draft.revision > revision);
        assert!(a.resume.is_none());
        let mut foreign = details;
        foreign["original_host"]["socket"] = json!("/another");
        a.management(vec![Intent::Resume(foreign)]);
        assert_eq!(a.draft.space.as_ref().unwrap().label(), "Current");
    }
    #[test]
    fn background_detaches_without_another_start_or_stop() {
        let mut a = App::new(
            Preferences::default(),
            "/projects".into(),
            "host".into(),
            json!({}),
            None,
        );
        a.launch_supported = true;
        a.manager.sessions_known = true;
        a.draft = Draft {
            account: Some("codex:fixture".into()),
            space: Some(Space::Existing {
                id: "w1".into(),
                label: "Space".into(),
            }),
            name: "New".into(),
            directory: Directory::Existing("/projects".into()),
            name_valid: true,
            directory_valid: true,
            revision: 1,
        };
        a.page = Page::Summary;
        a.options.selected = 4;
        assert!(matches!(a.activate().as_slice(), [Effect::Start(_)]));
        let request = a.request.clone();
        assert!(
            a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .is_empty()
        );
        assert!(a.launch_background);
        assert_eq!(a.page, Page::Home);
        assert_eq!(a.request, request);
        assert!(a.management(vec![Intent::New]).is_empty());
        assert_eq!(a.page, Page::Home);
    }
}
