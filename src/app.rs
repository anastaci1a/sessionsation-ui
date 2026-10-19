use crate::{
    client::{Account, Workspace},
    directory::Listing,
    draft::{Directory, Draft, Space, valid_name},
    preferences::{Mode, Preferences},
    render::{Accent, Canvas, Cell, Field},
    widgets::{Options, Row, TextInput},
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
    FocusSession(Value),
}
#[derive(Clone)]
struct Saved {
    page: Page,
    selected: usize,
    text: String,
    listing: Listing,
}
pub struct App {
    navigation: crate::widgets::Navigation,
    pub manager: crate::management::Manager,
    pub page: Page,
    pub options: Options,
    pub input: TextInput,
    pub listing: Listing,
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
    pub resume: Option<Value>,
    switch: Option<Value>,
    new_draft: Option<(Draft, bool, bool)>,
    stack: Vec<Saved>,
    history: BTreeMap<PathBuf, usize>,
    auto_account: bool,
    auto_space: bool,
    pub dirty: bool,
    pending_child: Option<PathBuf>,
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
            resume: None,
            switch: None,
            new_draft: None,
            stack: vec![],
            history: BTreeMap::new(),
            auto_account: true,
            auto_space: true,
            dirty: true,
            pending_child: None,
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
                Intent::Focus(session) => effects.push(Effect::FocusSession(session)),
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
                && name.chars().count() <= 24
                && name.trim() == name
                && !name.chars().any(char::is_control)
        } else {
            valid_name(name) && name.chars().count() <= 24
        }
    }
    fn ready(&self) -> bool {
        self.draft.account.is_some()
            && self.draft.space.is_some()
            && self.name_valid(&self.draft.name)
            && self.draft.name_valid
            && self.draft.directory_valid
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
                .unwrap_or_else(|| "[unset]".into())
        };
        match &self.page {
            Page::Home | Page::Management => self.manager.rows(),
            Page::Summary => {
                let fields = [
                    (
                        "Account",
                        self.draft
                            .account
                            .as_deref()
                            .map(crate::usage::short_name)
                            .unwrap_or_else(|| "[unset]".into()),
                    ),
                    (
                        "Space",
                        self.draft
                            .space
                            .as_ref()
                            .map(Space::label)
                            .filter(|s| !s.is_empty())
                            .unwrap_or("[unset]")
                            .to_owned(),
                    ),
                    (
                        "Name",
                        if self.draft.name.is_empty() {
                            "[unset]".into()
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
                        row.enabled = i != 3 || self.resume.is_none();
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
                    .with_preview(path(self.prefs.global_root.as_deref())),
                Row::gate("Space roots", self.prefs.mode == Mode::PerSpace),
            ],
            Page::Roots => self
                .spaces
                .iter()
                .map(|s| {
                    Row::yes(&s.label).with_preview(path(
                        self.prefs
                            .space_roots
                            .get(&Preferences::key(&self.instance, &s.workspace_id))
                            .map(PathBuf::as_path),
                    ))
                })
                .collect(),
            Page::Accounts => self
                .accounts
                .iter()
                .zip(crate::usage::picker_labels(&self.accounts))
                .map(|(a, usage)| {
                    Row::gate(crate::usage::short_name(&a.id), a.selectable)
                        .with_right_preview(usage)
                        .account()
                })
                .collect(),
            Page::Spaces => std::iter::once(Row::yes("Create new Space"))
                .chain(
                    self.spaces
                        .iter()
                        .map(|s| Row::yes(format!("{} [{}]", s.label, s.workspace_id))),
                )
                .collect(),
            Page::Name => vec![Row::gate("Continue", self.name_valid(&self.input.value))],
            Page::SpaceName => vec![Row::gate(
                "Use new Space",
                valid_name(&self.input.value)
                    && !self.spaces.iter().any(|s| s.label == self.input.value),
            )],
            Page::NewDirectory(_, _) => vec![Row::gate(
                "Create directory",
                valid_name(&self.input.value) && self.listing.creatable,
            )],
            Page::Directory(_) => {
                std::iter::once(Row::gate("[use this dir]", self.listing.existing))
                    .chain(std::iter::once(Row::gate(
                        "[create dir]",
                        self.listing.existing || self.listing.creatable,
                    )))
                    .chain((self.directory_action_count() == 3).then(|| Row::yes("[leave unset]")))
                    .chain(self.listing.children.iter().map(|p| {
                        Row::yes(format!(
                            "{}/",
                            p.file_name().unwrap_or_default().to_string_lossy()
                        ))
                    }))
                    .collect()
            }
            Page::Launching => vec![Row::gate("Starting session…", false)],
        }
    }
    fn push(&mut self, page: Page, text: String) -> Vec<Effect> {
        self.stack.push(Saved {
            page: self.page.clone(),
            selected: self.options.selected,
            text: self.input.value.clone(),
            listing: self.listing.clone(),
        });
        self.page = page;
        self.options.selected = 0;
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
        self.input.max_chars = (self.page == Page::Name).then_some(24);
        self.listing = Listing::default();
        self.error.clear();
        self.dirty = true;
        if matches!(self.page, Page::Directory(_) | Page::NewDirectory(..)) {
            self.inspect()
        } else {
            vec![]
        }
    }
    fn inspect(&mut self) -> Vec<Effect> {
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
        let save = matches!(self.page, Page::Config | Page::Roots);
        if self.page == Page::Accounts {
            self.switch = None;
        }
        if self.page == Page::Summary && self.resume.is_some() {
            self.restore_new_draft();
        }
        if let Some(old) = self.stack.pop() {
            self.page = old.page;
            self.options.selected = old.selected;
            self.input.value = old.text;
            self.input.cursor = self.input.value.len();
            self.input.max_chars = (self.page == Page::Name).then_some(24);
            self.listing = old.listing;
            self.options.back = false;
            self.options.scroll = 0;
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
        if let Some(session) = &self.resume {
            let mut request = json!({"version":1,"request_id":uuid::Uuid::new_v4().to_string(),"session_id":session["id"],"expected_generation":session["generation"],"account_id":self.draft.account.as_ref()?,"name":self.draft.name,"host":self.host,"space":space});
            if session["confirm_unfinished"] == true {
                request["confirm_unfinished"] = json!(true);
            }
            return Some(request);
        }
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
        self.dirty = true;
    }
    pub fn inventory(
        &mut self,
        accounts: Vec<Account>,
        spaces: Vec<Workspace>,
        now: i64,
    ) -> Vec<Effect> {
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
        match result {
            Ok(l) => {
                self.listing = l;
                if let Some(child) = self.pending_child.take()
                    && let Some(index) = self.listing.children.iter().position(|p| *p == child)
                {
                    self.options.selected = index + self.directory_action_count();
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
    fn activate(&mut self) -> Vec<Effect> {
        if self.options.back {
            return self.pop();
        }
        if self.page == Page::Summary && self.options.selected == 4 {
            if self.ready()
                && self.launch_supported()
                && !self.pending
                && self.request.is_none()
                && let Some(request) = self.creation_request()
            {
                self.request = Some(request.clone());
                self.launch_background = false;
                self.launch_dialog = true;
                self.manager.dialog = Some(crate::dialog::Dialog::progress("Starting session"));
                return vec![Effect::Start(request)];
            }
            return vec![];
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
                if self.directory_action_count() == 3 && self.options.selected == 2 {
                    return self.choose_directory(target, Directory::Unset);
                }
                if self.options.selected == 1 {
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
                                    [self.options.selected - self.directory_action_count()],
                            )
                            .cloned()
                            .unwrap_or_else(|| {
                                self.listing.children
                                    [self.options.selected - self.directory_action_count()]
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
        let wrap = self.navigation.key(key);
        if key.kind == crossterm::event::KeyEventKind::Release {
            return vec![];
        }
        self.dirty = true;
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
                self.page = Page::Home;
                self.manager.page = crate::management::Page::Home;
                self.manager.options = Options::default();
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
                    .checked_sub(self.directory_action_count())
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
                if self.options.selected < self.directory_action_count()
                    || self.listing.path.as_deref() == Some(Path::new("/"))
                {
                    self.options.back = true;
                    return vec![];
                }
                if let Some(path) = &self.listing.path {
                    self.history.insert(path.clone(), self.options.selected);
                    if let Some(parent) = path.parent() {
                        self.pending_child = Some(path.clone());
                        self.input.value = parent.to_string_lossy().into_owned();
                        self.input.cursor = self.input.value.len();
                        return self.inspect();
                    }
                }
                vec![]
            }
            KeyCode::Left | KeyCode::Right if self.is_form() => {
                if self.options.back && key.code == KeyCode::Right {
                    self.options.back = false;
                } else if key.code == KeyCode::Left && self.input.cursor == 0 {
                    self.options.back = true;
                } else {
                    self.input.edit(key.code);
                }
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
            _ => vec![],
        }
    }
    pub fn paint(&mut self, field: &mut Field, now: u64, width: usize, height: usize) -> Canvas {
        field.resize(width, height);
        field.options_top = if self.is_form() { 5 } else { 3 };
        field.background = Some(self.terminal_background);
        let mut canvas = field.paint(now);
        let white = Cell {
            fg: [238; 3],
            ..Cell::default()
        };
        let tokens: &[&str] = if matches!(self.page, Page::Directory(_)) {
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
        };
        let hints = crate::widgets::hint_lines(tokens, width);
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
            return canvas;
        }
        let title = match &self.page {
            Page::Home | Page::Management => self.manager.title(),
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
        let status = self.status.line(seconds as i64, &self.initial, width);
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
            self.manager.paint(&mut canvas, footer);
            field.apply_masks(&canvas);
            self.page = if self.manager.page == crate::management::Page::Home {
                Page::Home
            } else {
                Page::Management
            };
            return canvas;
        }
        let rows = self.rows();
        let top = if self.is_form() { 5 } else { 3 };
        let has_note = !self.error.is_empty()
            || self.pending
            || self.page == Page::Launching
            || self.page == Page::Accounts && !self.options.back;
        let menu_controls = if has_note {
            footer.saturating_sub(2)
        } else {
            footer
        };
        let bottom = menu_controls.saturating_sub(if self.page == Page::Summary { 3 } else { 1 });
        let (left, capacity) =
            self.options
                .paint_menu(&mut canvas, &rows, top, bottom, menu_controls);
        if self.page != Page::Launching {
            self.options.paint_back(
                &mut canvas,
                left,
                top,
                if self.page == Page::Home {
                    "Close"
                } else {
                    "Back"
                },
            );
        }
        if self.is_form() {
            let valid = if matches!(self.page, Page::Directory(_)) {
                self.listing.existing || self.listing.creatable
            } else {
                rows.first().is_some_and(|r| r.enabled)
            };
            self.input.paint(&mut canvas, 2, 3, width - 4, valid);
        }
        if self.page == Page::Summary {
            let enabled =
                self.ready() && self.launch_supported() && !self.pending && self.request.is_none();
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
                } else {
                    "Start Session"
                },
                enabled,
            )
            .paint(
                &mut canvas,
                left,
                top + rows.len().min(capacity) + 1,
                self.options.selected == 4 && !self.options.back,
                self.options.back,
            );
        }

        let note = if !self.error.is_empty() {
            self.error.clone()
        } else if self.page == Page::Accounts {
            self.accounts
                .get(self.options.selected)
                .filter(|_| !self.options.back)
                .map(|a| {
                    format!(
                        "{} — {}",
                        crate::usage::short_name(&a.id),
                        crate::usage::Usage::account(a).status
                    )
                })
                .unwrap_or_default()
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
        crate::widgets::paint_status(&mut canvas, footer, &note);
        crate::widgets::paint_hints(&mut canvas, &hints);
        if self.launch_dialog
            && let Some(d) = &mut self.manager.dialog
        {
            d.paint(&mut canvas, &self.manager.palette);
        }
        field.apply_masks(&canvas);
        canvas
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
                selected: 3,
                text: String::new(),
                listing: Listing::default(),
            },
            Saved {
                page: Page::Directory(Target::Draft),
                selected: 1,
                text: "/tmp".into(),
                listing: Listing::default(),
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
        a.options.selected = 2;
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
            assert_eq!(a.rows()[2].label, "[leave unset]");
            for row in 0..3 {
                a.options.selected = row;
                a.options.back = false;
                assert!(
                    a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
                        .is_empty()
                );
                assert_eq!(a.input.value, "/tmp");
                a.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
                assert!(a.options.back);
            }
            a.options.back = false;
            a.options.selected = 2;
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
    fn session_editor_stops_at_twenty_four_and_does_not_limit_other_editors() {
        let mut a = app();
        a.push(Page::Name, String::new());
        a.paste("abcdefghijklmnopqrstuvwxyz");
        assert_eq!(a.input.value, "abcdefghijklmnopqrstuvwx");
        assert!(a.name_valid(&a.input.value));
        assert!(!a.name_valid("abcdefghijklmnopqrstuvwxy"));
        a.push(Page::SpaceName, String::new());
        a.paste("a longer workspace name");
        assert_eq!(a.input.value, "a longer workspace name");
        a.pop();
        a.paste("ignored");
        assert_eq!(a.input.value, "abcdefghijklmnopqrstuvwx");
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
    fn resume_uses_verified_space_readonly_directory_and_preserves_new_draft() {
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
