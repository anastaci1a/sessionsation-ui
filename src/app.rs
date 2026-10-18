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
}
#[derive(Clone)]
struct Saved {
    page: Page,
    selected: usize,
    text: String,
    listing: Listing,
}
pub struct App {
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
    pub pending: bool,
    pub request: Option<Value>,
    pub revision: u64,
    pub exit: bool,
    pub success: bool,
    pub launch_supported: bool,
    stack: Vec<Saved>,
    history: BTreeMap<PathBuf, usize>,
    auto_account: bool,
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
            pending: false,
            request: None,
            revision: 0,
            exit: false,
            success: false,
            launch_supported: false,
            stack: vec![],
            history: BTreeMap::new(),
            auto_account: true,
            dirty: true,
            pending_child: None,
            status: crate::status::Status::default(),
        }
    }
    pub fn rows(&self) -> Vec<Row> {
        let path = |p: Option<&Path>| {
            p.map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "<unset>".into())
        };
        match &self.page {
            Page::Home => vec![Row::yes("New Session"), Row::yes("Configuration")],
            Page::Summary => vec![
                Row::yes(format!(
                    "Account: {}",
                    self.draft.account.as_deref().unwrap_or("<unset>")
                )),
                Row::yes(format!(
                    "Space: {}",
                    self.draft
                        .space
                        .as_ref()
                        .map(Space::label)
                        .filter(|s| !s.is_empty())
                        .unwrap_or("<unset>")
                )),
                Row::yes(format!(
                    "Name: {}",
                    if self.draft.name.is_empty() {
                        "<unset>"
                    } else {
                        &self.draft.name
                    }
                )),
                Row::yes(format!("Directory: {}", path(self.draft.directory.path()))),
            ],
            Page::Config => vec![
                Row::yes(format!(
                    "Directory defaults: {}",
                    if self.prefs.mode == Mode::Global {
                        "Global"
                    } else {
                        "Per-Space"
                    }
                )),
                Row::gate(
                    format!(
                        "Global projects root: {}",
                        path(self.prefs.global_root.as_deref())
                    ),
                    self.prefs.mode == Mode::Global,
                ),
                Row::gate(
                    "Per-Space projects roots",
                    self.prefs.mode == Mode::PerSpace,
                ),
            ],
            Page::Roots => self
                .spaces
                .iter()
                .map(|s| {
                    Row::yes(format!(
                        "{}: {}",
                        s.label,
                        path(
                            self.prefs
                                .space_roots
                                .get(&Preferences::key(&self.instance, &s.workspace_id))
                                .map(PathBuf::as_path)
                        )
                    ))
                })
                .collect(),
            Page::Accounts => self
                .accounts
                .iter()
                .map(|a| {
                    let remaining = |k: &str| {
                        a.usage[k]["used_percent"]
                            .as_f64()
                            .map(|v| format!("{:.0}%", 100. - v))
                            .unwrap_or_else(|| "?".into())
                    };
                    let note = a.prefill["reason"]
                        .as_str()
                        .map(|s| format!(" ({})", s.to_lowercase().replace('_', " ")))
                        .unwrap_or_default();
                    Row::gate(
                        format!(
                            "{}  {} / {} remaining{}",
                            a.id,
                            remaining("short"),
                            remaining("weekly"),
                            note
                        ),
                        a.selectable,
                    )
                })
                .collect(),
            Page::Spaces => std::iter::once(Row::yes("Create new Space"))
                .chain(
                    self.spaces
                        .iter()
                        .map(|s| Row::yes(format!("{} [{}]", s.label, s.workspace_id))),
                )
                .collect(),
            Page::Name => vec![Row::gate("Continue", valid_name(&self.input.value))],
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
                std::iter::once(Row::gate("Use this directory", self.listing.existing))
                    .chain(std::iter::once(Row::gate(
                        "Add new directory",
                        self.listing.existing || self.listing.creatable,
                    )))
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
        self.options.scroll = 0;
        self.options.back = false;
        self.input.value = text;
        self.input.cursor = self.input.value.len();
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
        if let Some(old) = self.stack.pop() {
            self.page = old.page;
            self.options.selected = old.selected;
            self.input.value = old.text;
            self.input.cursor = self.input.value.len();
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
        if !valid_name(&self.draft.name) {
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
    pub fn expire_prefill(&mut self, now: i64) {
        if self.auto_account
            && self.draft.account.as_ref().is_some_and(|id| {
                !self
                    .accounts
                    .iter()
                    .any(|a| a.id == *id && a.prefill["valid_until"].as_i64().unwrap_or(0) > now)
            })
        {
            self.draft.account = None;
            self.draft.name_valid = false;
            self.dirty = true;
        }
    }
    pub fn inventory(
        &mut self,
        accounts: Vec<Account>,
        spaces: Vec<Workspace>,
        now: i64,
    ) -> Vec<Effect> {
        let old_account = self.draft.account.clone();
        let old_space = self.draft.space.clone();
        self.accounts = accounts;
        self.spaces = spaces;
        if let Some(Space::Existing { id, label }) = &mut self.draft.space {
            if let Some(s) = self.spaces.iter().find(|s| s.workspace_id == *id) {
                *label = s.label.clone();
            } else {
                self.draft.space = None;
            }
        }
        if self.auto_account {
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
                    self.options.selected = index + 2;
                }
            }
            Err(e) => {
                self.listing = Listing::default();
                self.error = e;
            }
        }
        let rows = self.rows();
        if !rows.get(self.options.selected).is_some_and(|r| r.enabled) {
            self.options.selected = rows.iter().position(|r| r.enabled).unwrap_or(0);
        }
        self.dirty = true;
    }
    fn choose_directory(&mut self, target: Target, directory: Directory) -> Vec<Effect> {
        let Some(path) = directory.path().map(Path::to_path_buf) else {
            return vec![];
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
            if self.draft.ready()
                && self.launch_supported
                && !self.pending
                && let Some(request) = self.creation_request()
            {
                self.request = Some(request.clone());
                self.page = Page::Launching;
                return vec![Effect::Start(request)];
            }
            return vec![];
        }
        let rows = self.rows();
        if !rows.get(self.options.selected).is_some_and(|r| r.enabled) {
            return vec![];
        }
        match self.page.clone() {
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
                self.draft.account = Some(self.accounts[self.options.selected].id.clone());
                self.auto_account = false;
                self.pop();
                self.revalidate()
            }
            Page::Spaces => {
                if self.options.selected == 0 {
                    self.push(Page::SpaceName, String::new())
                } else {
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
                self.draft.space = Some(Space::New {
                    label: self.input.value.clone(),
                });
                self.pop();
                self.pop();
                self.revalidate()
            }
            Page::Directory(target) => {
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
                            .get(&self.listing.children[self.options.selected - 2])
                            .cloned()
                            .unwrap_or_else(|| {
                                self.listing.children[self.options.selected - 2].clone()
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
        self.dirty = true;
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('d') => {
                    self.exit = true;
                    return vec![Effect::Exit];
                }
                KeyCode::Char('u') if self.is_form() => {
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
                if self.is_form() =>
            {
                self.input.edit(key.code);
                if matches!(self.page, Page::Directory(_) | Page::NewDirectory(..)) {
                    self.inspect()
                } else {
                    vec![]
                }
            }
            KeyCode::Up | KeyCode::Down => {
                self.options.step(
                    &self.rows(),
                    self.page == Page::Summary,
                    key.code == KeyCode::Up,
                );
                vec![]
            }
            KeyCode::Right | KeyCode::Tab if matches!(self.page, Page::Directory(_)) => {
                if self.options.back && key.code == KeyCode::Right {
                    self.options.back = false;
                    return vec![];
                }
                let index = self.options.selected.saturating_sub(2);
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
                if self.options.selected == 0
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
                self.input.edit(key.code);
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
        let mut canvas = field.paint(now);
        let white = Cell {
            fg: [238; 3],
            ..Cell::default()
        };
        let tokens: &[&str] = if matches!(self.page, Page::Directory(_)) {
            &[
                "▲/▼: Choose",
                "▶/Tab: Open",
                "◀: Parent",
                "Enter: Select",
                "Esc: Back",
                "Ctrl-U: Clear",
            ]
        } else if self.is_form() {
            &["Enter: Accept", "Esc: Back", "Ctrl-U: Clear"]
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
            Page::Home => "",
            Page::Summary => "New Session",
            Page::Config => "Configuration",
            Page::Roots => "Per-Space projects roots",
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
        let rows = self.rows();
        let top = if self.is_form() { 5 } else { 3 };
        let bottom = footer - 4 - usize::from(self.page == Page::Summary) * 2;
        let (left, capacity) = self.options.paint(&mut canvas, &rows, top, bottom);
        if self.is_form() {
            let valid = if matches!(self.page, Page::Directory(_)) {
                self.listing.existing || self.listing.creatable
            } else {
                rows.first().is_some_and(|r| r.enabled)
            };
            self.input.paint(&mut canvas, 2, 3, width - 4, valid);
        }
        if self.page == Page::Summary {
            let enabled = self.draft.ready() && self.launch_supported && !self.pending;
            canvas.accent_text(
                left,
                footer - 6,
                "•",
                Accent {
                    brightness: 600,
                    flash: false,
                    ..Accent::default()
                },
            );
            Row::gate("Start Session", enabled).paint(
                &mut canvas,
                left,
                footer - 5,
                self.options.selected == 4 && !self.options.back,
                self.options.back,
            );
        }
        if rows.len() > capacity {
            canvas.text(
                left,
                footer - 3,
                &format!(
                    "{}–{} of {}",
                    self.options.scroll + 1,
                    (self.options.scroll + capacity).min(rows.len()),
                    rows.len()
                ),
                Cell::default(),
                false,
            );
        }
        let note = if !self.error.is_empty() {
            self.error.clone()
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
        canvas.text(
            1,
            footer - 2,
            &note,
            Cell {
                fg: [215, 135, 0],
                ..Cell::default()
            },
            false,
        );
        canvas.text(
            2,
            footer - 1,
            if self.options.back {
                "◀— Back"
            } else {
                "◀—"
            },
            Cell {
                fg: if self.options.back {
                    [238; 3]
                } else {
                    [188; 3]
                },
                bold: self.options.back,
                ..Cell::default()
            },
            false,
        );
        let hint_width = hints
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.as_str()))
            .max()
            .unwrap_or(0);
        let hint_left = width.saturating_sub(hint_width) / 2;
        for (i, line) in hints.iter().enumerate() {
            canvas.accent_text(
                hint_left,
                height - hints.len() + i,
                line,
                Accent {
                    soft: true,
                    brightness: 550,
                    flash: false,
                    ..Accent::default()
                },
            );
        }
        field.apply_masks(&canvas);
        canvas
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
