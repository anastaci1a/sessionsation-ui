use crate::{
    client::{Account, Workspace},
    directory::Listing,
    draft::{Directory, Draft, Space, valid_name},
    preferences::{
        Brightness, ClaudeSpinner, CodexSpinner, GeminiSpinner, Mode, Preferences, Starfield,
        choice_arrow,
    },
    render::{Accent, Canvas, Cell, Field},
    widgets::{NAME_MAX_CHARS, Options, Row, TextInput, UNSET},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Left-align every picker glyph of a settings page in one column: one space after
/// the longest row name, then the glyph, a space and the value. Grey settings
/// values sit three spaces after the longest name, level with the picker values.
fn align_pickers(rows: &mut [Row]) {
    use unicode_width::UnicodeWidthStr;
    let longest = rows
        .iter()
        .map(|r| UnicodeWidthStr::width(r.label.as_str()))
        .max()
        .unwrap_or(0);
    for row in rows.iter_mut() {
        if row.preview_arrows {
            row.preview_column = Some(2 + longest + 1);
        } else if row.settings_value {
            row.preview_column = Some(2 + longest + SETTINGS_VALUE_GAP);
        }
    }
}
/// Accent level of the selected row bullet.
const SELECTED_BULLET_LEVEL: i64 = 1000;
/// Spaces between the longest settings name and the grey settings values.
const SETTINGS_VALUE_GAP: usize = 3;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Draft,
    Global,
    Space(String),
}

/// Default directory modes, top (most specific) to bottom.
const MODES: [Mode; 2] = [Mode::PerSpace, Mode::Global];

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
        } else if error.starts_with("SESSION_STALE") {
            "stale"
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
/// Settings rows: label on the left, the dim selected option in one aligned column.
fn settings_rows(items: Vec<(Row, String)>) -> Vec<Row> {
    use unicode_width::UnicodeWidthStr;
    let label = items
        .iter()
        .map(|(r, _)| UnicodeWidthStr::width(r.label.as_str()))
        .max()
        .unwrap_or(0);
    items
        .into_iter()
        .map(|(mut row, v)| {
            row.preview = Some(v);
            row.preview_column = Some(2 + label + SETTINGS_VALUE_GAP);
            row.preview_gap = 1;
            row.preview_selected_only = false;
            row.preview_info = true;
            row.preview_fg = crate::widgets::MUTED;
            row.settings_value = true;
            row
        })
        .collect()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Management,
    Summary,
    Config,
    Appearance,
    Spinners,
    Directories,
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
    LaunchLogin(Vec<String>),
    /// Create confirmed settings folders in order (parents first).
    CreateDirectories(Target, PathBuf, Vec<PathBuf>),
}
enum StartConfirmation {
    SharedDirectory(PathBuf),
    UnfinishedHistory,
    CreateFolders(PathBuf),
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
    /// Preferences held when the inline value picker was entered (revert target).
    picker: Option<Box<Preferences>>,
    /// Paint time in milliseconds, for the live spinner previews.
    anim_ms: u64,
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
    provider_filter: Option<String>,
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
    /// Settings folders awaiting confirmation: (dialog, target, chosen path, folders to create).
    create_confirmation: Option<(crate::dialog::Dialog, Target, PathBuf, Vec<PathBuf>)>,
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
    /// Draft revision last started or queued; the same draft is not submitted twice.
    submitted: Option<u64>,
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
            navigation: crate::widgets::Navigation,
            manager: crate::management::Manager::with_host(host.clone()),
            page: Page::Home,
            options: Options::default(),
            input: TextInput::default(),
            listing: Listing::default(),
            listing_pending: false,
            draft,
            prefs,
            accounts: vec![],
            provider_filter: None,
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
            create_confirmation: None,
            resume: None,
            resume_validation: None,
            switch: None,
            new_draft: None,
            stack: vec![],
            history: BTreeMap::new(),
            auto_account: true,
            auto_space: true,
            dirty: true,
            picker: None,
            anim_ms: 0,
            pending_child: None,
            submitted: None,
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
                Intent::LaunchLogin(argv) => effects.push(Effect::LaunchLogin(argv)),
                Intent::New => {
                    // A launch still in progress keeps running; another Start queues
                    // behind it when its account is busy.
                    self.restore_new_draft();
                    effects.extend(self.push(Page::Summary, String::new()));
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
    /// A settings selector is grabbed: the rest of the page steps back.
    fn right_mode(&self) -> bool {
        self.picker.is_some()
            && matches!(
                self.page,
                Page::Appearance | Page::Spinners | Page::Directories
            )
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
                            .map(|id| {
                                self.accounts.iter().find(|a| a.id == id).map_or_else(
                                    || crate::usage::short_name(id),
                                    crate::usage::account_title,
                                )
                            })
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
                        if i == 3 && matches!(self.draft.directory, Directory::Create { .. }) {
                            // Same marker as the picker: folders created at launch.
                            row.preview = Some("(new)".into());
                            row.preview_info = true;
                            row.preview_fg = crate::widgets::MUTED;
                            row.preview_gap = 2;
                            row.preview_selected_only = false;
                        }
                        row.enabled = i != 3
                            || self.resume.is_none()
                            || self.manager.capabilities["resume"]["directory_selection"] == true;
                        row
                    })
                    .collect()
            }
            Page::Config => settings_rows(vec![
                (
                    Row::yes("Stars"),
                    format!(
                        "{} · {}",
                        self.prefs.starfield.label(),
                        self.prefs.brightness.label()
                    ),
                ),
                (Row::yes("Default directories"), self.mode_label().into()),
                (
                    Row::yes("Spinners"),
                    format!(
                        "{} · {} · {}",
                        self.prefs.claude_spinner.label(),
                        self.prefs.codex_spinner.label(),
                        self.prefs.gemini_spinner.label()
                    ),
                ),
            ]),
            Page::Appearance => {
                let mut rows = vec![
                    Row::yes("Starfield")
                        .with_picker(
                            self.prefs.starfield.label(),
                            choice_arrow(&Starfield::ALL, self.prefs.starfield),
                        )
                        .grabbed(self.picker.is_some() && self.options.selected == 0),
                    Row::yes("Brightness")
                        .with_picker(
                            self.prefs.brightness.label(),
                            choice_arrow(&Brightness::ALL, self.prefs.brightness),
                        )
                        .grabbed(self.picker.is_some() && self.options.selected == 1),
                ];
                align_pickers(&mut rows);
                rows
            }
            Page::Spinners => {
                let ms = self.anim_ms;
                let item =
                    |i: usize, label: &str, value: &str, arrow: &str, glyph: &'static str| {
                        let shown = if self.options.selected == i && !self.options.back {
                            format!("{value} {glyph}")
                        } else {
                            value.to_owned()
                        };
                        Row::yes(label)
                            .with_picker(shown, arrow)
                            .grabbed(self.picker.is_some() && self.options.selected == i)
                    };
                let s = self.prefs.spinners();
                let mut rows = vec![
                    item(
                        0,
                        "Claude spinner",
                        s.claude.label(),
                        choice_arrow(&ClaudeSpinner::ALL, s.claude),
                        crate::session_list::claude_frame(s.claude, ms),
                    ),
                    item(
                        1,
                        "Codex spinner",
                        s.codex.label(),
                        choice_arrow(&CodexSpinner::ALL, s.codex),
                        crate::session_list::codex_frame(s.codex, ms),
                    ),
                    item(
                        2,
                        "Gemini spinner",
                        s.gemini.label(),
                        choice_arrow(&GeminiSpinner::ALL, s.gemini),
                        crate::session_list::gemini_frame(s.gemini, ms),
                    ),
                ];
                align_pickers(&mut rows);
                rows
            }
            Page::Directories => {
                let mut rows = vec![
                    Row::yes("Default directory")
                        .with_picker(self.mode_label(), choice_arrow(&MODES, self.prefs.mode))
                        .grabbed(self.picker.is_some()),
                ];
                rows.extend(settings_rows(vec![
                    (
                        Row::gate("Global root", self.prefs.mode == Mode::Global),
                        self.prefs.global_root.as_deref().map_or_else(
                            || UNSET.into(),
                            |p| crate::widgets::ellipsize_start(&p.to_string_lossy(), 40),
                        ),
                    ),
                    (
                        Row::gate("Space roots", self.prefs.mode == Mode::PerSpace),
                        match self.prefs.space_roots.len() {
                            0 => UNSET.into(),
                            n => format!("{n} set"),
                        },
                    ),
                ]));
                align_pickers(&mut rows);
                rows
            }
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
                let visible = self.picker_accounts();
                let labels = crate::usage::picker_labels(&visible);
                let preview_width = labels
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.as_str()))
                    .max()
                    .unwrap_or(0);
                let name_width = visible
                    .iter()
                    .map(|a| UnicodeWidthStr::width(crate::usage::account_title(a).as_str()))
                    .max()
                    .unwrap_or(0);
                let tagged = self.accounts.iter().any(|a| a.provider() != "codex");
                let tag_width = if tagged { 12 } else { 0 };
                self.picker_filter_row()
                    .then(|| Row::yes(self.picker_filter_label()))
                    .into_iter()
                    .chain(visible.iter().zip(labels).map(|(a, usage)| {
                        let usage = if tagged {
                            format!(
                                "{:<11} {usage}",
                                crate::client::provider_label(a.provider())
                            )
                        } else {
                            usage
                        };
                        let column = 2 + name_width + 1 + preview_width + tag_width
                            - UnicodeWidthStr::width(usage.as_str());
                        Row::gate(crate::usage::account_title(a), a.selectable)
                            .with_symbol(crate::usage::provider_symbol(a.provider()))
                            .with_column_preview(usage, column, false)
                            .preview_gap(1)
                            .preview_color(crate::widgets::SECONDARY)
                            .with_status(
                                crate::usage::Usage::account(a).status.clone(),
                                crate::usage::Usage::account(a).status == "Ready",
                            )
                    }))
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
                let prefix = self.directory_prefix().chars().count();
                let new = |row: Row| {
                    let mut row = row;
                    row.preview = Some("(new)".into());
                    row.preview_info = true;
                    row.preview_fg = crate::widgets::MUTED;
                    row.preview_gap = 2;
                    row
                };
                let mut use_row = Row::gate("Use this folder", self.directory_base().is_some());
                if self.directory_base().is_some() && !self.listing.existing {
                    use_row = new(use_row);
                }
                let mut rows = vec![use_row, Row::separator()];
                if let Some(name) = self.directory_new_name() {
                    rows.push(new(Row::yes(format!("{name}/"))));
                } else if self.listing.children.is_empty() && !self.listing_pending {
                    rows.push(Row::placeholder("no folders"));
                }
                rows.extend(self.listing.children.iter().map(|p| {
                    let mut row = Row::yes(format!(
                        "{}/",
                        p.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    row.highlight = prefix;
                    row
                }));
                rows.push(Row::separator());
                if self.directory_action_count() == 3 {
                    rows.push(Row::yes("Leave unset"));
                }
                rows.push(Row::yes("Cancel"));
                rows
            }
            Page::Launching => vec![Row::gate("Starting session…", false)],
        };
        if self.right_mode() {
            for (i, row) in rows.iter_mut().enumerate() {
                row.mode = if i == self.options.selected {
                    crate::widgets::RowMode::Active
                } else {
                    crate::widgets::RowMode::Other
                };
            }
        }
        if Self::is_popup(&self.page) {
            if matches!(self.page, Page::Directory(_)) {
                // Its mounted sections are built above.
            } else if matches!(self.page, Page::Spaces | Page::Accounts | Page::Roots) {
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
        let save = matches!(
            self.page,
            Page::Config | Page::Roots | Page::Appearance | Page::Spinners | Page::Directories
        );
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
                self.options
                    .selected
                    .checked_sub(self.picker_base())
                    .and_then(|i| self.picker_accounts().get(i).map(|a| a.id.clone()))
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
        self.manager.space_names = self
            .spaces
            .iter()
            .map(|s| (s.workspace_id.clone(), s.label.clone()))
            .collect();
        {
            let mut focused = self.spaces.iter().filter(|s| s.focused);
            self.manager.focused_space = focused
                .next()
                .filter(|_| focused.next().is_none())
                .map(|s| s.workspace_id.clone());
        }
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
                    .picker_accounts()
                    .iter()
                    .position(|a| a.id == id)
                    .map(|i| i + self.picker_base())
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
        // Mounted sections keep focus by position from their own edge.
        let directory = matches!(self.page, Page::Directory(_));
        let old_len = self.rows().len();
        let from_end = (directory && self.options.selected + self.directory_tail() >= old_len)
            .then(|| old_len - self.options.selected);
        let in_head = directory && self.options.selected < self.directory_children_offset();
        self.listing_pending = false;
        match result {
            Ok(l) => {
                self.listing = l;
                if let Some(from_end) = from_end {
                    self.options.selected = self.rows().len().saturating_sub(from_end);
                } else if in_head {
                } else if directory && !self.listing.existing {
                    // Typing filters the list: the top match is focused.
                    self.options.selected = self.directory_children_offset();
                    self.options.scroll = 0;
                } else if matches!(self.page, Page::Directory(_)) && self.listing.existing {
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
    /// Provider an in-progress resume or account switch is bound to; the backend
    /// never converts a conversation between providers.
    fn picker_lock(&self) -> Option<String> {
        let session = self.switch.as_ref().or(self.resume.as_ref())?;
        session["provider"].as_str().map(str::to_owned).or_else(|| {
            session["account_id"]
                .as_str()
                .and_then(|id| id.split_once(':'))
                .map(|(provider, _)| provider.to_owned())
        })
    }
    fn picker_filter_row(&self) -> bool {
        self.picker_lock().is_none()
            && self
                .accounts
                .iter()
                .any(|a| a.provider() != self.accounts[0].provider())
    }
    fn picker_filter(&self) -> Option<String> {
        self.picker_lock().or_else(|| {
            self.provider_filter
                .clone()
                .filter(|_| self.picker_filter_row())
        })
    }
    /// Accounts offered by the account step, after the provider lock or filter.
    fn picker_accounts(&self) -> Vec<&Account> {
        let filter = self.picker_filter();
        self.accounts
            .iter()
            .filter(|a| filter.as_deref().is_none_or(|f| a.provider() == f))
            .collect()
    }
    fn picker_base(&self) -> usize {
        usize::from(self.picker_filter_row())
    }
    fn picker_filter_label(&self) -> String {
        format!(
            "Provider: {}",
            self.picker_filter()
                .as_deref()
                .map_or("All", |p| crate::client::provider_label(p))
        )
    }
    fn cycle_picker_filter(&mut self) {
        let mut providers: Vec<String> = vec![];
        for a in &self.accounts {
            if !providers.iter().any(|p| p == a.provider()) {
                providers.push(a.provider().to_owned());
            }
        }
        providers.sort();
        self.provider_filter = match self.picker_filter() {
            None => providers.first().cloned(),
            Some(current) => providers
                .iter()
                .position(|p| *p == current)
                .and_then(|i| providers.get(i + 1))
                .cloned(),
        };
        self.options.selected = self.opening_account_row();
        self.dirty = true;
    }
    fn opening_account_row(&self) -> usize {
        let current = self
            .switch
            .as_ref()
            .and_then(|s| s["account_id"].as_str())
            .or(self.draft.account.as_deref());
        let visible = self.picker_accounts();
        visible
            .iter()
            .position(|a| Some(a.id.as_str()) == current)
            .map(|i| i + self.picker_base())
            .unwrap_or(self.picker_base())
    }
    fn mode_label(&self) -> &'static str {
        if self.prefs.mode == Mode::Global {
            "Global"
        } else {
            "Per Space"
        }
    }
    /// Rows whose value is an interactive `‹ value ›` selector.
    fn choice_row(&self) -> bool {
        !self.options.back
            && match self.page {
                Page::Appearance => self.options.selected < 2,
                Page::Spinners => self.options.selected < 3,
                Page::Directories => self.options.selected == 0,
                _ => false,
            }
    }
    /// Grab the selected choice: Up/Down preview values until committed or cancelled.
    fn enter_picker(&mut self) {
        self.picker = Some(Box::new(self.prefs.clone()));
    }
    /// Keys while a value is grabbed. Esc reverts; Left/Enter/Space commit.
    fn picker_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Up | KeyCode::Down => self.cycle_choice(key.code == KeyCode::Up),
            KeyCode::Left | KeyCode::Enter | KeyCode::Char(' ') => self.picker = None,
            KeyCode::Esc => {
                if let Some(saved) = self.picker.take() {
                    self.prefs = *saved;
                }
            }
            _ => {}
        }
        self.dirty = true;
        vec![]
    }
    fn cycle_choice(&mut self, previous: bool) {
        use crate::preferences::step;
        let up = previous;
        match (&self.page, self.options.selected) {
            (Page::Appearance, 0) => {
                self.prefs.starfield = step(&Starfield::ALL, self.prefs.starfield, up)
            }
            (Page::Appearance, 1) => {
                self.prefs.brightness = step(&Brightness::ALL, self.prefs.brightness, up)
            }
            (Page::Spinners, 0) => {
                self.prefs.claude_spinner = step(&ClaudeSpinner::ALL, self.prefs.claude_spinner, up)
            }
            (Page::Spinners, 1) => {
                self.prefs.codex_spinner = step(&CodexSpinner::ALL, self.prefs.codex_spinner, up)
            }
            (Page::Spinners, 2) => {
                self.prefs.gemini_spinner = step(&GeminiSpinner::ALL, self.prefs.gemini_spinner, up)
            }
            (Page::Directories, 0) => self.prefs.mode = step(&MODES, self.prefs.mode, up),
            _ => {}
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
    /// Rows above the list: `Use this folder` and its bullet.
    fn directory_children_offset(&self) -> usize {
        2
    }
    /// Mounted rows below the list: a bullet, optional `Leave unset` and Cancel.
    fn directory_tail(&self) -> usize {
        if self.directory_action_count() == 3 {
            3
        } else {
            2
        }
    }
    /// The typed last component while it names no existing folder.
    fn directory_prefix(&self) -> String {
        if self.listing.existing {
            return String::new();
        }
        self.listing
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
    /// "This folder" is the typed path, existing or to be created.
    fn directory_base(&self) -> Option<PathBuf> {
        self.listing
            .path
            .clone()
            .filter(|_| self.listing.existing || self.listing.creatable)
    }
    /// A typed name matching no folder becomes a single `name/ (new)` entry.
    fn directory_new_name(&self) -> Option<String> {
        (!self.listing.existing
            && self.listing.creatable
            && self.listing.children.is_empty()
            && !self.listing_pending)
            .then(|| self.directory_prefix())
            .filter(|n| !n.is_empty())
    }
    /// Faint completion of the alphabetically first matching folder.
    fn directory_ghost(&self) -> String {
        let prefix = self.directory_prefix();
        if prefix.is_empty() || self.listing_pending {
            return String::new();
        }
        self.listing
            .children
            .first()
            .and_then(|c| c.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .and_then(|n| {
                n.strip_prefix(prefix.as_str())
                    .map(|rest| format!("{rest}/"))
            })
            .unwrap_or_default()
    }
    /// Browse into a folder: the input ends with `/` so typing filters its children.
    fn browse(&mut self, path: &Path) -> Vec<Effect> {
        let mut text = path.to_string_lossy().into_owned();
        if !text.ends_with('/') {
            text.push('/');
        }
        self.input.value = text;
        self.input.cursor = self.input.value.len();
        self.inspect()
    }
    fn directory_parent(&mut self) -> Vec<Effect> {
        let Some(path) = self.listing.path.clone() else {
            return vec![];
        };
        // A typed, unmatched name first falls back to the folder it is typed in.
        let (current, parent) = if self.listing.existing {
            (Some(path.clone()), path.parent().map(Path::to_path_buf))
        } else {
            (None, path.parent().map(Path::to_path_buf))
        };
        let Some(parent) = parent else {
            return vec![];
        };
        if let Some(current) = current {
            self.history.insert(current.clone(), self.options.selected);
            self.pending_child = Some(current);
        }
        self.browse(&parent)
    }
    /// Tab accepts the faint completion; Right enters the selected subfolder, or the
    /// completion while the top section is focused.
    fn directory_open(&mut self, tab: bool) -> Vec<Effect> {
        if self.listing_pending {
            return vec![];
        }
        let selected_child = self
            .options
            .selected
            .checked_sub(self.directory_children_offset())
            .and_then(|i| self.listing.children.get(i))
            .cloned()
            .filter(|_| self.directory_new_name().is_none());
        let completion = (!self.directory_ghost().is_empty())
            .then(|| self.listing.children.first().cloned())
            .flatten();
        let child = if tab {
            completion.or(selected_child)
        } else {
            selected_child.or(completion.filter(|_| self.options.selected == 0))
        };
        let Some(child) = child else {
            return vec![];
        };
        if let Some(path) = &self.listing.path
            && self.listing.existing
        {
            self.history.insert(path.clone(), self.options.selected);
        }
        let remembered = self.history.get(&child).copied();
        self.options.selected = remembered.unwrap_or(self.directory_children_offset());
        self.options.scroll = 0;
        self.browse(&child)
    }
    /// The shared confirmation listing exactly which folders will be created.
    pub fn folders_dialog(missing: &[PathBuf]) -> crate::dialog::Dialog {
        let count = missing.len();
        let mut body = vec![format!(
            "Create {count} folder{}?",
            if count == 1 { "" } else { "s" }
        )];
        body.extend(missing.iter().map(|p| format!("{}/", p.display())));
        let mut dialog = crate::dialog::Dialog::new(
            if count == 1 {
                "Create folder"
            } else {
                "Create folders"
            },
            body,
            &["Create", "Cancel"],
        );
        dialog.selected = 1;
        dialog
    }
    /// Before launching into folders that do not exist yet, confirm them; the
    /// launch itself creates them, so Cancel leaves nothing behind.
    fn confirm_folders_or_start(&mut self) -> Vec<Effect> {
        if let Directory::Create { path, planned } = &self.draft.directory
            && self.start_ready()
        {
            let missing = if planned.is_empty() {
                vec![path.clone()]
            } else {
                planned.clone()
            };
            self.start_confirmation = Some((
                Self::folders_dialog(&missing),
                StartConfirmation::CreateFolders(path.clone()),
            ));
            return vec![];
        }
        self.begin_start()
    }
    /// Choosing a folder that does not exist yet. New sessions stage it until the
    /// launch creates it; settings first confirm exactly which folders are created.
    fn choose_new(&mut self, target: Target) -> Vec<Effect> {
        let Some(path) = self.listing.path.clone() else {
            return vec![];
        };
        let missing = if self.listing.missing.is_empty() {
            vec![path.clone()]
        } else {
            self.listing.missing.clone()
        };
        if target == Target::Draft {
            return self.choose_directory(
                target,
                Directory::Create {
                    path,
                    planned: missing,
                },
            );
        }
        self.create_confirmation = Some((Self::folders_dialog(&missing), target, path, missing));
        vec![]
    }
    /// Result of creating confirmed settings folders.
    pub fn directories_created(
        &mut self,
        target: Target,
        path: PathBuf,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        self.dirty = true;
        match result {
            Ok(()) if matches!(self.page, Page::Directory(ref t) if *t == target) => {
                let mut effects = self.choose_directory(target, Directory::Existing(path));
                effects.push(Effect::Save);
                effects
            }
            Ok(()) => vec![],
            Err(e) => {
                self.error = format!("Folders were not created: {e}");
                vec![]
            }
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
    pub fn refresh_live(&mut self) -> Vec<Effect> {
        let intents = if self.page == Page::Summary && self.resume.is_none() {
            self.manager.poll_sessions(true)
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
            && self.submitted != Some(self.draft.revision)
            && (self.resume.is_some()
                || (self.manager.sessions_known && !self.manager.cached_snapshot))
    }
    fn begin_start(&mut self) -> Vec<Effect> {
        if !self.start_ready() {
            return vec![];
        }
        if let Some(request) = self.creation_request() {
            self.submitted = Some(self.draft.revision);
            if !self.launch_or_queue(&request) {
                return self.leave_queued();
            }
            self.request = Some(request.clone());
            self.success = false;
            self.launch_background = false;
            self.launch_dialog = true;
            self.manager.dialog = Some(crate::dialog::Dialog::progress("Starting session"));
            return vec![Effect::Start(request)];
        }
        vec![]
    }
    /// Register a direct launch with the queue, or queue it when its target or
    /// this view's launch slot is busy. Returns true when it starts now.
    fn launch_or_queue(&mut self, request: &Value) -> bool {
        use crate::op_queue::{Action, Command, Entry};
        let resume = self.resume.is_some();
        let (resource, kind) = if resume {
            (
                format!("session:{}", request["session_id"].as_str().unwrap_or("")),
                "resume-launch",
            )
        } else {
            (
                format!("account:{}", request["account_id"].as_str().unwrap_or("")),
                "launch",
            )
        };
        let provider = request["account_id"]
            .as_str()
            .and_then(|a| a.split_once(':'))
            .map_or("codex", |(p, _)| p);
        let subject = format!(
            "{} {}",
            crate::usage::provider_symbol(provider),
            request["name"].as_str().unwrap_or("")
        );
        let mut entry = Entry::new(
            resource.clone(),
            kind,
            subject,
            Action::Launch {
                kind: kind.into(),
                request: request.clone(),
            },
        );
        let slot_busy = self.request.is_some() && !self.success;
        if slot_busy || !self.manager.can_start_now(&resource) {
            self.manager.queue.push(entry.clone());
            self.manager.queue_out.push(Command::Enqueue(entry));
            return false;
        }
        entry.operation_id = request["request_id"].as_str().map(str::to_owned);
        self.manager.queue.push({
            let mut running = entry.clone();
            running.state = crate::op_queue::State::Running;
            running.started_at = Some(crate::op_queue::now());
            running
        });
        self.manager.queue_out.push(Command::Register(entry));
        true
    }
    /// A queued Start returns to the page it was opened from; the queue owns it.
    fn leave_queued(&mut self) -> Vec<Effect> {
        let mut effects = vec![];
        while !self.stack.is_empty() {
            effects.extend(self.pop());
        }
        effects
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
            return self.confirm_folders_or_start();
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
                0 => self.push(Page::Appearance, String::new()),
                1 => self.push(Page::Directories, String::new()),
                _ => self.push(Page::Spinners, String::new()),
            },
            Page::Appearance | Page::Spinners => {
                self.enter_picker();
                vec![]
            }
            Page::Directories => match self.options.selected {
                0 => {
                    self.enter_picker();
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
                if self.picker_filter_row() && self.options.selected == 0 {
                    self.cycle_picker_filter();
                    return vec![];
                }
                let Some(account) = self
                    .options
                    .selected
                    .checked_sub(self.picker_base())
                    .and_then(|i| self.picker_accounts().get(i).map(|a| a.id.clone()))
                else {
                    return vec![];
                };
                if let Some(session) = self.switch.take() {
                    self.pop();
                    self.manager.switch_confirmation(session, &account);
                    return vec![];
                }
                self.draft.account = Some(account);
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
                let label = rows
                    .get(self.options.selected)
                    .map(|r| r.label.clone())
                    .unwrap_or_default();
                if label == "Leave unset"
                    && self.options.selected + self.directory_tail() >= rows.len()
                {
                    return self.choose_directory(target, Directory::Unset);
                }
                let new_entry = self.directory_new_name().is_some()
                    && self.options.selected == self.directory_children_offset();
                if self.options.selected == 0 || new_entry {
                    if self.listing.existing {
                        let path = self.listing.path.clone().unwrap();
                        return self.choose_directory(target, Directory::Existing(path));
                    }
                    return self.choose_new(target);
                }
                let Some(child) = self
                    .options
                    .selected
                    .checked_sub(self.directory_children_offset())
                    .and_then(|i| self.listing.children.get(i))
                    .cloned()
                else {
                    return vec![];
                };
                let path = self.listing.resolved.get(&child).cloned().unwrap_or(child);
                self.choose_directory(target, Directory::Existing(path))
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
                    let folders = matches!(confirmation, StartConfirmation::CreateFolders(_));
                    let unchanged = match confirmation {
                        StartConfirmation::SharedDirectory(directory)
                        | StartConfirmation::CreateFolders(directory) => {
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
                        self.error = "Directory changed; review Start session again.".into();
                        return vec![];
                    }
                    if folders {
                        return self.begin_start();
                    }
                    return self.confirm_folders_or_start();
                }
                crate::dialog::Event::Dismiss | crate::dialog::Event::Choose(..) => {
                    self.start_confirmation = None
                }
                crate::dialog::Event::None => {}
            }
            return vec![];
        }
        if let Some((dialog, ..)) = &mut self.create_confirmation
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            match dialog.key_with_wrap(key, wrap) {
                crate::dialog::Event::Choose(0, _) => {
                    let (_, target, path, missing) = self.create_confirmation.take().unwrap();
                    return vec![Effect::CreateDirectories(target, path, missing)];
                }
                crate::dialog::Event::Dismiss | crate::dialog::Event::Choose(..) => {
                    self.create_confirmation = None
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
        if self.picker.is_some() {
            return self.picker_key(key);
        }
        match key.code {
            KeyCode::Esc => self.pop(),
            KeyCode::Enter => self.activate(),
            KeyCode::Char(' ') if !self.is_form() => self.activate(),
            KeyCode::Char(c) if self.is_form() => self.edit(&c.to_string()),
            KeyCode::Home | KeyCode::End if matches!(self.page, Page::Directory(_)) => {
                self.options
                    .boundary_sections(&self.rows(), key.code == KeyCode::End);
                vec![]
            }
            KeyCode::PageUp | KeyCode::PageDown => {
                self.options.page(&self.rows(), key.code == KeyCode::PageUp);
                vec![]
            }
            KeyCode::Up | KeyCode::Down if matches!(self.page, Page::Directory(_)) => {
                let tail = self.directory_tail();
                self.options.step_sections(
                    &self.rows(),
                    self.directory_children_offset(),
                    tail,
                    key.code == KeyCode::Up,
                    wrap,
                );
                vec![]
            }
            KeyCode::Tab if matches!(self.page, Page::Directory(_)) => self.directory_open(true),
            KeyCode::Right if matches!(self.page, Page::Directory(_)) => {
                if self.options.back {
                    self.options.back = false;
                    return vec![];
                }
                self.directory_open(false)
            }
            KeyCode::Left if matches!(self.page, Page::Directory(_)) => {
                if self.listing_pending || self.listing.path.as_deref() == Some(Path::new("/")) {
                    return vec![];
                }
                self.directory_parent()
            }
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
            KeyCode::Right if self.choice_row() => {
                self.enter_picker();
                vec![]
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
            Page::Accounts => "Choose account",
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
        if matches!(self.page, Page::Directory(_)) {
            field.input.ghost = self.directory_ghost();
        }
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
            browse_hint: editor.is_some(),
            sections: editor
                .is_some()
                .then(|| (self.directory_children_offset(), self.directory_tail())),
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
        self.anim_ms = now;
        self.manager.anim_ms = now;
        self.manager.spinners = self.prefs.spinners();
        field.background = Some(self.terminal_background);
        field.set_style(
            self.prefs.starfield.density(),
            self.prefs.brightness.scale(),
        );
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
        if let Some((dialog, ..)) = &mut self.create_confirmation {
            dialog.paint(&mut canvas, &self.manager.palette);
        }
        if Self::is_popup(&self.page) && height > 2 {
            // The footer follows the top popup or dialog, not the page beneath it.
            for cell in &mut canvas.cells[(height - 2) * width..] {
                *cell = Cell::default();
            }
            crate::widgets::paint_hints(
                &mut canvas,
                &crate::widgets::hint_lines(self.control_tokens(), width),
            );
        }
        field.apply_masks(&canvas);
        canvas
    }
    fn control_tokens(&self) -> &'static [&'static str] {
        if self.picker.is_some() {
            &["▲/▼: Choose", "Enter: Set", "Esc: Cancel"]
        } else if self.start_confirmation.is_some() || self.create_confirmation.is_some() {
            &["▲/▼: Choose", "Enter: Select", "Esc: Cancel"]
        } else if matches!(self.page, Page::Directory(_)) {
            if self.directory_ghost().is_empty() {
                return &[
                    "▲/▼: Choose",
                    "◀/▶: Parent/Open",
                    "Enter: Select",
                    "Esc: Cancel",
                ];
            }
            &[
                "▲/▼: Choose",
                "◀/▶: Parent/Open",
                "Tab: Complete",
                "Enter: Select",
                "Esc: Cancel",
            ]
        } else if matches!(self.page, Page::NewDirectory(..)) {
            &["Enter: Accept", "Esc: Cancel"]
        } else if self.is_form() {
            &["Enter: Accept", "Esc: Cancel", "Ctrl-U: Clear"]
        } else if Self::is_popup(&self.page) {
            &["▲/▼: Choose", "Enter: Select", "Esc: Cancel"]
        } else if self.launch_dialog {
            &["Esc: Close"]
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
                crate::widgets::paint_centered_hint(canvas, 0, 1, width, "Esc: Back / close");
            }
            return;
        }
        let management_title = self.manager.title();
        let title = match &self.page {
            Page::Home | Page::Management => management_title.as_str(),
            Page::Summary => {
                if self.resume.is_some() {
                    "Resume session"
                } else {
                    "New session"
                }
            }
            Page::Config => "Settings",
            Page::Appearance => "Stars",
            Page::Spinners => "Spinners",
            Page::Directories => "Default directories",
            Page::Roots => "Space roots",
            Page::Accounts => "Choose account",
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
            self.manager.band_base = self.terminal_background;
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
    /// The focused spinner row's live glyph wears the selected bullet's accent
    /// (same function and level), sampled at the glyph's own cell.
    fn paint_spinner_preview(&self, canvas: &mut Canvas, top: usize) {
        let index = self.options.selected;
        if self.options.back || index >= 3 || index < self.options.scroll {
            return;
        }
        let s = self.prefs.spinners();
        let glyph = match index {
            0 => crate::session_list::claude_frame(s.claude, self.anim_ms),
            1 => crate::session_list::codex_frame(s.codex, self.anim_ms),
            _ => crate::session_list::gemini_frame(s.gemini, self.anim_ms),
        };
        let y = top + index - self.options.scroll;
        if y >= canvas.height {
            return;
        }
        let row = &canvas.cells[y * canvas.width..(y + 1) * canvas.width];
        let Some(x) = row.iter().rposition(|c| c.glyph == glyph) else {
            return;
        };
        let bold = row[x].bold;
        canvas.accent_text(
            x,
            y,
            glyph,
            Accent {
                brightness: SELECTED_BULLET_LEVEL,
                bold,
                ..Accent::default()
            },
        );
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
        self.options.right_mode = self.right_mode();
        let (left, capacity) = self
            .options
            .paint_menu(canvas, &rows, top, bottom, menu_controls);
        if self.page == Page::Spinners {
            self.paint_spinner_preview(canvas, top);
        }
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
                    "Resume session"
                } else if self.shared_directory() {
                    "Start session ⚠"
                } else {
                    "Start session"
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
                .any(|l| l.contains("⏣ b") && l.contains("Login required"))
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
    fn rapid_legacy_presses_are_never_dropped_and_a_hold_stops_at_the_edge() {
        // Diagnosed on 8216b1a: quick re-presses at the last row were read as a
        // hold and silently ignored. Every press now moves; a rapid stream from
        // a non-edge row is one hold, so it stops at the edge instead of wrapping.
        let mut app = app();
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let rows = app.manager.rows().iter().filter(|r| !r.separator).count();
        for _ in 0..rows {
            assert!(app.key(down).is_empty());
        }
        let last = app.manager.rows().len() - 1;
        assert_eq!(app.manager.options.selected, last, "the hold stopped");
        assert!(app.flush_navigation().is_empty(), "nothing was deferred");
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        app.key(up);
        assert!(app.manager.options.selected < last, "Up moved back");
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
    pub(super) fn app() -> App {
        App::new(
            Preferences::default(),
            "/tmp".into(),
            "test".into(),
            json!({}),
            None,
        )
    }
    #[test]
    fn topbar_roots_and_summary_share_dim_not_set_placeholders() {
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
                .contains("not set")
        );
        a.page = Page::Summary;
        a.draft = Draft::default();
        // Unset fields show a dim placeholder in the value column, not in the label.
        for row in a.rows().iter().take(4) {
            assert_eq!(row.preview.as_deref(), Some("not set"));
            assert_eq!(row.preview_fg, crate::widgets::MUTED);
            assert!(row.label.ends_with(':'));
        }
        a.prefs.global_root = None;
        a.page = Page::Directories;
        assert!(
            a.rows()[1]
                .preview
                .as_deref()
                .unwrap()
                .starts_with("not set")
        );
        a.page = Page::Roots;
        a.spaces = vec![Workspace {
            workspace_id: "missing".into(),
            label: "No root".into(),
            focused: false,
        }];
        assert_eq!(a.rows()[0].preview.as_deref(), Some("not set"));
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
            assert!(text.contains("Start session ⚠"));
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
    fn manage_sessions_text_never_destroys_the_stars_beneath_it() {
        let (width, height) = (100usize, 24usize);
        let mut a = app();
        a.page = Page::Management;
        a.manager.page = crate::management::Page::Sessions;
        a.manager.sessions_known = true;
        a.manager.sessions = vec![
            json!({"id":"a","name":"alpha","provider":"claude","account_id":"claude:main","availability":"running","activity":"idle"}),
            json!({"id":"b","name":"bravo","provider":"codex","account_id":"codex:work account","availability":"stopped"}),
        ];
        let mut field = Field::new(width, height, 1967);
        let bare = Field::new(width, height, 1967).paint(0);
        let manage = a.paint(&mut field, 0, width, height);
        // Every cell the account rows paint over a star.
        let covered: Vec<usize> = (0..height)
            .filter(|&y| {
                let line: String = manage.cells[y * width..(y + 1) * width]
                    .iter()
                    .map(|c| c.glyph.as_str())
                    .collect();
                line.contains("alpha") || line.contains("bravo")
            })
            .flat_map(|y| y * width..(y + 1) * width)
            .filter(|&i| {
                matches!(bare.cells[i].glyph.as_str(), "*" | "." | "+")
                    && manage.cells[i] != bare.cells[i]
            })
            .collect();
        assert!(covered.len() > 3, "account text covers stars");
        // Another screen with nothing there: the same stars as a field that never
        // showed Manage Sessions.
        let mut b = app();
        b.page = Page::Config;
        a.page = Page::Config;
        let after = a.paint(&mut field, 0, width, height);
        let control = b.paint(&mut Field::new(width, height, 1967), 0, width, height);
        let restored: Vec<usize> = covered
            .into_iter()
            .filter(|&i| control.cells[i].glyph == bare.cells[i].glyph)
            .collect();
        assert!(!restored.is_empty());
        for i in restored {
            assert_eq!(after.cells[i], control.cells[i], "star at {i} restored");
        }
    }
    #[test]
    fn app_manage_sessions_running_account_uses_the_icon_accent_through_the_real_render_path() {
        for width in [80usize, 120, 160] {
            for (selected, back, level) in
                [(0usize, false, 1000i64), (1, false, 600), (0, true, 600)]
            {
                let mut a = app();
                a.page = Page::Management;
                a.manager.page = crate::management::Page::Sessions;
                a.manager.sessions_known = true;
                a.manager.sessions = vec![
                    json!({"id":"a","name":"alpha","provider":"claude","account_id":"claude:main","availability":"running","activity":"idle"}),
                    json!({"id":"b","name":"bravo","provider":"claude","account_id":"claude:main","availability":"running","activity":"idle"}),
                ];
                a.manager.options.selected = selected;
                a.manager.options.back = back;
                let c = a.paint(&mut Field::new(width, 24, 1), 5000, width, 24);
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
                    + rows[0].preview.as_ref().unwrap().chars().count(),
                rows[1].preview_position(inner, left).unwrap()
                    + rows[1].preview.as_ref().unwrap().chars().count()
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
                assert_eq!(
                    canvas.cells[0].glyph,
                    if row.symbol.is_some() {
                        "⏣"
                    } else if index == 0 {
                        "●"
                    } else {
                        "○"
                    }
                );
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
    fn picker(target: Target, children: usize) -> App {
        let mut a = app();
        a.page = Page::Directory(target);
        a.input = TextInput::new("/projects/".into());
        a.listing = Listing {
            path: Some("/projects".into()),
            existing: true,
            children: (0..children)
                .map(|i| format!("/projects/project{i:02}").into())
                .collect(),
            ..Listing::default()
        };
        a
    }
    fn screen(c: &crate::render::Canvas) -> Vec<String> {
        c.cells
            .chunks(c.width)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>())
            .collect()
    }
    fn key(a: &mut App, code: KeyCode) -> Vec<Effect> {
        a.key_with_wrap(KeyEvent::new(code, KeyModifiers::NONE), true)
    }
    #[test]
    fn picker_mounts_use_this_folder_above_and_cancel_below_a_scrolling_list() {
        let mut a = picker(Target::Draft, 36);
        let rows = a.rows();
        assert_eq!(rows[0].label, "Use this folder");
        assert!(rows[1].separator);
        assert_eq!(rows[2].label, "project00/");
        assert!(rows[rows.len() - 2].separator);
        assert_eq!(rows.last().unwrap().label, "Cancel");
        assert!(
            !rows
                .iter()
                .any(|r| r.label.contains("<new>") || r.label.contains('['))
        );
        let mut field = Field::new(80, 26, 1);
        let paint = |a: &mut App, field: &mut Field| screen(&a.paint(field, 0, 80, 26));
        a.options.selected = 2;
        let top = paint(&mut a, &mut field);
        let use_row = top
            .iter()
            .position(|l| l.contains("Use this folder"))
            .unwrap();
        assert!(top[use_row + 2].contains("project00/"));
        let cancel = top.iter().position(|l| l.contains("Cancel")).unwrap();
        // Up from the first list item enters the top section without scrolling.
        key(&mut a, KeyCode::End);
        assert_eq!(a.options.selected, a.rows().len() - 1);
        let after_end = paint(&mut a, &mut field);
        assert!(
            after_end[use_row + 2].contains("project00/"),
            "End keeps the list"
        );
        assert_eq!(
            after_end.iter().position(|l| l.contains("Cancel")),
            Some(cancel)
        );
        // Up from the bottom section re-enters the list at its last item.
        key(&mut a, KeyCode::Up);
        assert_eq!(a.rows()[a.options.selected].label, "project35/");
        let bottom = paint(&mut a, &mut field);
        assert!(bottom.iter().any(|l| l.contains("project35/")));
        assert!(
            bottom[use_row].contains("Use this folder"),
            "top stays mounted"
        );
        let scroll = a.options.scroll;
        key(&mut a, KeyCode::Down);
        assert_eq!(a.rows()[a.options.selected].label, "Cancel");
        assert_eq!(
            a.options.scroll, scroll,
            "entering the bottom never scrolls"
        );
        key(&mut a, KeyCode::Home);
        assert_eq!(a.options.selected, 0);
        assert_eq!(a.options.scroll, scroll, "Home keeps the list position too");
        key(&mut a, KeyCode::Down);
        assert_eq!(a.options.selected, 2, "re-enters at the list's first item");
        key(&mut a, KeyCode::PageDown);
        assert_eq!(a.options.selected, 2 + a.options.capacity - 1, "one page");
        key(&mut a, KeyCode::PageUp);
        assert!(a.options.selected <= 4);
    }
    #[test]
    fn typed_prefix_filters_highlights_and_ghosts_the_first_match() {
        let mut a = picker(Target::Draft, 0);
        a.input = TextInput::new("/projects/fo".into());
        a.listing = Listing {
            path: Some("/projects/fo".into()),
            existing: false,
            creatable: true,
            children: vec!["/projects/fob".into(), "/projects/foo".into()],
            missing: vec!["/projects/fo".into()],
            ..Listing::default()
        };
        let rows = a.rows();
        assert_eq!(rows[2].label, "fob/");
        assert_eq!(rows[2].highlight, 2);
        assert_eq!(a.directory_ghost(), "b/");
        assert!(a.control_tokens().contains(&"Tab: Complete"));
        a.options.selected = 0;
        let c = a.paint(&mut Field::new(80, 26, 1), 0, 80, 26);
        let lines = screen(&c);
        assert!(
            lines.iter().any(|l| l.contains("/projects/fob/")),
            "ghost completes: {lines:#?}"
        );
        let y = lines.iter().position(|l| l.contains("foo/")).unwrap();
        let x = lines[y].chars().position(|ch| ch == 'f').unwrap();
        let (prefix, rest) = (&c.cells[y * 80 + x], &c.cells[y * 80 + x + 2]);
        assert_ne!(prefix.fg, rest.fg, "typed prefix is brighter");
        let ghost_y = lines
            .iter()
            .position(|l| l.contains("/projects/fob/"))
            .unwrap();
        let gx = lines[ghost_y].find("fob/").unwrap() + 2;
        let gx = lines[ghost_y][..gx].chars().count();
        assert_eq!(c.cells[ghost_y * 80 + gx].fg, a.manager.palette.muted);
        assert!(
            matches!(&key(&mut a, KeyCode::Tab)[..], [Effect::Inspect(_, p)] if p == "/projects/fob/")
        );
    }
    #[test]
    fn unmatched_name_is_one_new_entry_staged_until_launch() {
        let mut a = picker(Target::Draft, 0);
        a.input = TextInput::new("/projects/newname".into());
        a.listing = Listing {
            path: Some("/projects/newname".into()),
            existing: false,
            creatable: true,
            missing: vec!["/projects/newname".into()],
            ..Listing::default()
        };
        let rows = a.rows();
        assert_eq!(rows[2].label, "newname/");
        assert_eq!(rows[2].preview.as_deref(), Some("(new)"));
        assert_eq!(rows[2].preview_fg, crate::widgets::MUTED);
        assert_eq!(rows[0].preview.as_deref(), Some("(new)"));
        a.stack.push(Saved {
            page: Page::Summary,
            options: Options::default(),
            text: String::new(),
            listing: Listing::default(),
            listing_pending: false,
            popup_size: None,
        });
        a.options.selected = 2;
        let effects = a.activate();
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::CreateDirectories(..)))
        );
        assert!(
            a.create_confirmation.is_none(),
            "new sessions stage, never create"
        );
        assert_eq!(
            a.draft.directory,
            Directory::Create {
                path: "/projects/newname".into(),
                planned: vec!["/projects/newname".into()],
            }
        );
        assert_eq!(a.page, Page::Summary);
        // The form marks the staged folder like the picker does.
        let dir = &a.rows()[3];
        assert_eq!(dir.preview.as_deref(), Some("(new)"));
        assert!(!dir.preview_selected_only);
    }
    #[test]
    fn left_is_parent_on_every_row_and_right_enters_subfolders() {
        let rows = picker(Target::Global, 3).rows().len();
        for row in [0, 2, rows - 2, rows - 1] {
            let mut a = picker(Target::Global, 3);
            a.options.selected = row;
            assert!(
                matches!(&key(&mut a, KeyCode::Left)[..], [Effect::Inspect(_, p)] if p == "/"),
                "row {row}"
            );
        }
        let mut a = picker(Target::Global, 3);
        a.options.selected = 0;
        assert!(
            key(&mut a, KeyCode::Right).is_empty(),
            "no completion to accept"
        );
        a.options.selected = 3;
        assert!(
            matches!(&key(&mut a, KeyCode::Right)[..], [Effect::Inspect(_, p)] if p == "/projects/project01/")
        );
    }
    #[test]
    fn settings_folders_are_created_only_after_a_stacked_confirmation() {
        let mut a = picker(Target::Global, 0);
        a.stack.push(Saved {
            page: Page::Directories,
            options: Options::default(),
            text: String::new(),
            listing: Listing::default(),
            listing_pending: false,
            popup_size: None,
        });
        a.input = TextInput::new("/projects/new-a/new-b".into());
        a.listing = Listing {
            path: Some("/projects/new-a/new-b".into()),
            existing: false,
            creatable: true,
            missing: vec!["/projects/new-a".into(), "/projects/new-a/new-b".into()],
            ..Listing::default()
        };
        a.options.selected = 0;
        assert!(a.activate().is_empty());
        let d = &a.create_confirmation.as_ref().unwrap().0;
        assert_eq!(
            d.body,
            [
                "Create 2 folders?",
                "/projects/new-a/",
                "/projects/new-a/new-b/"
            ]
        );
        // Stacked over the still-open picker.
        let lines = screen(&a.paint(&mut Field::new(90, 28, 1), 0, 90, 28));
        assert!(lines.iter().any(|l| l.contains("Create 2 folders?")));
        assert!(lines.iter().any(|l| l.contains("Start directory")));
        assert!(a.control_tokens().contains(&"Esc: Cancel"));
        // Cancel closes only the confirmation; the picker is untouched.
        assert!(key(&mut a, KeyCode::Esc).is_empty());
        assert!(a.create_confirmation.is_none());
        assert_eq!(a.page, Page::Directory(Target::Global));
        assert_eq!(a.input.value, "/projects/new-a/new-b");
        assert_eq!(a.options.selected, 0);
        assert!(a.prefs.global_root.is_none());
        // Create emits exactly those folders, then saves and closes the picker.
        a.activate();
        a.paint(&mut Field::new(90, 28, 1), 0, 90, 28);
        key(&mut a, KeyCode::Up);
        let effects = key(&mut a, KeyCode::Enter);
        let [Effect::CreateDirectories(Target::Global, path, missing)] = &effects[..] else {
            panic!("{effects:?}")
        };
        assert_eq!(missing.len(), 2);
        let effects = a.directories_created(Target::Global, path.clone(), Ok(()));
        assert!(effects.iter().any(|e| matches!(e, Effect::Save)));
        assert_eq!(
            a.prefs.global_root.as_deref(),
            Some(Path::new("/projects/new-a/new-b"))
        );
        assert_eq!(a.page, Page::Directories);
        // Existing folders need no confirmation.
        let mut b = picker(Target::Global, 0);
        b.options.selected = 0;
        b.activate();
        assert!(b.create_confirmation.is_none());
        assert_eq!(b.prefs.global_root.as_deref(), Some(Path::new("/projects")));
    }
    #[test]
    fn new_session_start_confirms_staged_folders_before_launching() {
        let mut a = app();
        a.page = Page::Summary;
        a.launch_supported = true;
        a.manager.sessions_known = true;
        a.draft = Draft {
            account: Some("codex:personal".into()),
            space: Some(Space::Existing {
                id: "s".into(),
                label: "Space".into(),
            }),
            name: "Another".into(),
            directory: Directory::Create {
                path: "/projects/new-a".into(),
                planned: vec!["/projects/new-a".into()],
            },
            name_valid: true,
            directory_valid: true,
            revision: 1,
        };
        a.options.selected = 4;
        assert!(
            a.activate().is_empty(),
            "nothing launches before confirming"
        );
        let (d, _) = a.start_confirmation.as_ref().unwrap();
        assert_eq!(d.body, ["Create 1 folder?", "/projects/new-a/"]);
        assert!(key(&mut a, KeyCode::Esc).is_empty());
        assert!(a.request.is_none() && a.start_confirmation.is_none());
        a.activate();
        a.paint(&mut Field::new(90, 28, 1), 0, 90, 28);
        key(&mut a, KeyCode::Up);
        assert!(
            matches!(key(&mut a, KeyCode::Enter).as_slice(), [Effect::Start(r)]
            if r["directory"]["create"] == json!(["/projects/new-a"]))
        );
    }
    #[test]
    fn picker_esc_reverts_without_popping_and_left_reaches_back_arrow() {
        use crate::preferences::Starfield;
        let mut a = app();
        a.page = Page::Appearance;
        a.options.selected = 0;
        assert_eq!(a.prefs.starfield, Starfield::Normal);
        // Normal mode: Left reaches the back arrow, Right leaves it again.
        key(&mut a, KeyCode::Left);
        assert!(a.options.back && a.picker.is_none());
        key(&mut a, KeyCode::Right);
        assert!(!a.options.back && a.picker.is_none());
        assert_eq!(a.prefs.starfield, Starfield::Normal);
        // Picker mode: grabbed rendering and picker hints.
        key(&mut a, KeyCode::Right);
        assert!(a.rows()[0].preview_grabbed && !a.rows()[1].preview_grabbed);
        assert_eq!(
            a.control_tokens(),
            ["▲/▼: Choose", "Enter: Set", "Esc: Cancel"]
        );
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Down);
        assert_eq!(a.prefs.starfield, Starfield::Off);
        key(&mut a, KeyCode::Down);
        assert_eq!(a.prefs.starfield, Starfield::Off, "no wrap at the bottom");
        assert_eq!(
            a.options.selected, 0,
            "Up/Down change the value, not the row"
        );
        key(&mut a, KeyCode::Right);
        assert!(a.picker.is_some(), "Right does nothing in picker mode");
        key(&mut a, KeyCode::Esc);
        assert!(a.picker.is_none());
        assert_eq!(a.prefs.starfield, Starfield::Normal, "Esc reverts");
        assert_eq!(
            a.page,
            Page::Appearance,
            "picker Esc does not leave the page"
        );
        assert!(!a.rows()[0].preview_grabbed);
        // The arrow shows in both modes; no cursor bullet is drawn by the value.
        key(&mut a, KeyCode::Right);
        let c = a.paint(&mut Field::new(80, 24, 1), 4000, 80, 24);
        let text: String = c.cells.iter().map(|c| c.glyph.as_str()).collect();
        assert!(text.contains("Starfield  ↕ Normal"), "{text}");
        assert!(!text.contains("● Normal"));
        key(&mut a, KeyCode::Esc);
        let c = a.paint(&mut Field::new(80, 24, 1), 4000, 80, 24);
        let text: String = c.cells.iter().map(|c| c.glyph.as_str()).collect();
        assert!(text.contains("Starfield  › Normal") && !text.contains("‹ Normal ›"));
    }
    #[test]
    fn spinner_pickers_cycle_preview_live_and_persist() {
        use crate::preferences::{ClaudeSpinner, CodexSpinner, GeminiSpinner};
        let mut a = app();
        a.page = Page::Config;
        assert_eq!(a.rows()[2].label, "Spinners");
        assert_eq!(
            a.rows()[2].preview.as_deref(),
            Some("Claude · Fill pulse · Twinkle")
        );
        a.options.selected = 2;
        a.activate();
        assert_eq!(a.page, Page::Spinners);
        let labels: Vec<_> = a.rows().iter().map(|r| r.label.clone()).collect();
        assert_eq!(
            labels,
            ["Claude spinner", "Codex spinner", "Gemini spinner"]
        );
        // Only the focused row animates; the others show just the name.
        a.options.selected = 1;
        let mut field = Field::new(80, 24, 1);
        a.paint(&mut field, 0, 80, 24);
        assert_eq!(a.rows()[1].preview.as_deref(), Some("↓ Fill pulse ⬡"));
        assert_eq!(a.rows()[0].preview.as_deref(), Some("↓ Claude"));
        a.paint(&mut field, 240, 80, 24);
        assert_eq!(a.rows()[1].preview.as_deref(), Some("↓ Fill pulse ⏣"));
        // Grab, move down to Rotate: the preview follows the grabbed option.
        key(&mut a, KeyCode::Right);
        key(&mut a, KeyCode::Down);
        assert_eq!(a.prefs.codex_spinner, CodexSpinner::Rotate);
        assert!(a.rows()[1].preview_grabbed);
        assert_eq!(a.rows()[1].preview.as_deref(), Some("↕ Rotate ⬢"));
        for _ in 0..10 {
            key(&mut a, KeyCode::Down);
        }
        assert_eq!(a.prefs.codex_spinner, CodexSpinner::Glow, "no wrap");
        assert_eq!(a.rows()[1].preview.as_deref(), Some("↑ Glow ⏣"));
        key(&mut a, KeyCode::Esc);
        assert_eq!(
            a.prefs.codex_spinner,
            CodexSpinner::FillPulse,
            "Esc reverts"
        );
        // Commit one of each.
        for (row, downs) in [(0, 2), (1, 2), (2, 1)] {
            a.options.selected = row;
            key(&mut a, KeyCode::Enter);
            for _ in 0..downs {
                key(&mut a, KeyCode::Down);
            }
            key(&mut a, KeyCode::Enter);
        }
        assert_eq!(a.prefs.claude_spinner, ClaudeSpinner::Glow);
        assert_eq!(a.prefs.codex_spinner, CodexSpinner::Benzene);
        assert_eq!(a.prefs.gemini_spinner, GeminiSpinner::SpinFill);
        assert!(matches!(
            key(&mut a, KeyCode::Esc).first(),
            Some(Effect::Save)
        ));
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("preferences.json");
        a.prefs.save(&path).unwrap();
        assert_eq!(
            Preferences::load(&path).unwrap().spinners(),
            a.prefs.spinners()
        );
        // Painting syncs the choice into the session list.
        a.paint(&mut field, 0, 80, 24);
        assert_eq!(a.manager.spinners, a.prefs.spinners());
    }
    fn row_y(c: &Canvas, label: &str) -> usize {
        (0..c.height)
            .find(|&y| {
                c.cells[y * c.width..(y + 1) * c.width]
                    .iter()
                    .map(|c| c.glyph.as_str())
                    .collect::<String>()
                    .contains(label)
            })
            .unwrap_or_else(|| panic!("{label} not painted"))
    }
    fn col_of(c: &Canvas, y: usize, text: &str) -> Option<usize> {
        let line: Vec<&str> = c.cells[y * c.width..(y + 1) * c.width]
            .iter()
            .map(|c| c.glyph.as_str())
            .collect();
        (0..line.len()).find(|&x| line[x..].concat().starts_with(text))
    }
    #[test]
    fn settings_values_and_arrows_never_count_toward_centering() {
        use crate::preferences::{ClaudeSpinner, Starfield};
        use crate::widgets::Options;
        for width in [60usize, 80, 120] {
            let label_x = |page: Page, configure: &dyn Fn(&mut App), label: &str| {
                let mut a = app();
                a.page = page;
                configure(&mut a);
                let c = a.paint(&mut Field::new(width, 24, 1), 0, width, 24);
                col_of(&c, row_y(&c, label), label).unwrap()
            };
            let names = |a: &App| {
                a.rows()
                    .into_iter()
                    .map(|mut r| {
                        r.preview = None;
                        r
                    })
                    .collect::<Vec<_>>()
            };
            for (page, label) in [
                (Page::Spinners, "Claude spinner"),
                (Page::Appearance, "Brightness"),
                (Page::Config, "Default directories"),
                (Page::Directories, "Default directory"),
            ] {
                let base = label_x(page.clone(), &|_| {}, label);
                let long = label_x(
                    page.clone(),
                    &|a| {
                        a.prefs.claude_spinner = ClaudeSpinner::Glow;
                        a.prefs.starfield = Starfield::Off;
                        a.prefs.global_root = Some("/a/very/long/global/root/path/here".into());
                    },
                    label,
                );
                let mut a = app();
                a.page = page.clone();
                let expect = Options::left(width, &names(&a)) + 2;
                assert_eq!(base, expect, "{page:?} w={width}");
                assert_eq!(long, expect, "{page:?} long values w={width}");
            }
        }
    }
    #[test]
    fn settings_previews_are_all_or_nothing_and_move_below_when_one_does_not_fit() {
        use unicode_width::UnicodeWidthStr;
        let mut a = app();
        a.page = Page::Config;
        let wide = a.paint(&mut Field::new(100, 24, 1), 0, 100, 24);
        for (label, text) in [
            ("Stars", "Normal · Normal"),
            ("Spinners", "Claude · Fill pulse · Twinkle"),
        ] {
            assert!(
                col_of(&wide, row_y(&wide, label), text).is_some(),
                "{label} inline when every preview fits"
            );
        }
        // Fit is decided with the three-space gap after the longest name.
        let widest = "Claude · Fill pulse · Twinkle";
        let longest = "Default directories".len();
        let (mut inline, mut below) = (false, false);
        for width in 50..=130 {
            let c = a.paint(&mut Field::new(width, 24, 1), 0, width, 24);
            let y = row_y(&c, "Spinners");
            let lx = col_of(&c, y, "Spinners").unwrap();
            let fits = lx + longest + 3 + UnicodeWidthStr::width(widest) < width;
            assert_eq!(col_of(&c, y, widest).is_some(), fits, "w={width}");
            inline |= fits;
            below |= !fits;
        }
        assert!(inline && below);
        // Too narrow for the widest value: none inline, the selected one below.
        for (selected, shown, hidden) in [
            (0usize, "Normal · Normal", "Claude · Fill pulse · Twinkle"),
            (2, "Claude · Fill pulse · Twinkle", "Normal · Normal"),
        ] {
            a.options.selected = selected;
            let c = a.paint(&mut Field::new(50, 24, 1), 0, 50, 24);
            for label in ["Stars", "Default directories", "Spinners"] {
                let y = row_y(&c, label);
                assert!(col_of(&c, y, "›").is_none(), "{label} stays bare");
            }
            let y = (0..24).find(|&y| col_of(&c, y, shown).is_some()).unwrap();
            assert!(y > row_y(&c, "Spinners"), "drawn below the list");
            assert!((0..24).all(|y| col_of(&c, y, hidden).is_none()));
            assert_eq!(
                c.cells[y * 50 + col_of(&c, y, shown).unwrap()].fg,
                crate::widgets::MUTED
            );
        }
    }
    #[test]
    fn settings_preview_is_always_darker_grey_three_spaces_after_the_longest_name() {
        use crate::widgets::MUTED;
        for page in [Page::Config, Page::Directories] {
            let mut a = app();
            a.page = page.clone();
            a.prefs.global_root = Some("/g".into());
            let rows = a.rows();
            let previews: Vec<usize> = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.settings_value)
                .map(|(i, _)| i)
                .collect();
            let longest = rows
                .iter()
                .map(|r| unicode_width::UnicodeWidthStr::width(r.label.as_str()))
                .max()
                .unwrap();
            for focused in 0..rows.len() {
                for back in [false, true] {
                    a.options.selected = focused;
                    a.options.back = back;
                    let c = a.paint(&mut Field::new(120, 24, 1), 0, 120, 24);
                    let mut starts = std::collections::BTreeSet::new();
                    for &i in &previews {
                        let y = row_y(&c, &rows[i].label);
                        let lx = col_of(&c, y, &rows[i].label).unwrap();
                        let line: Vec<_> = (0..120).map(|x| c.cells[y * 120 + x].clone()).collect();
                        let start = (lx + longest..120).find(|&x| line[x].glyph != " ").unwrap();
                        assert_eq!(
                            start,
                            lx + longest + 3,
                            "{page:?} three spaces after the longest name"
                        );
                        starts.insert(start - lx);
                        let preview = rows[i].preview.as_deref().unwrap();
                        for cell in &line[start..start + preview.chars().count()] {
                            assert_eq!(cell.fg, MUTED, "{page:?} row {i} focused={focused}");
                        }
                    }
                    assert_eq!(starts.len(), 1, "previews line up");
                }
            }
        }
    }
    #[test]
    fn default_directory_options_share_one_text_column() {
        // Picker, plain submenu and unset rows: names in one column, bullets in
        // theirs, and every option's text in one column after the glyph column.
        for mode in MODES {
            for width in [60usize, 80, 120] {
                let mut a = app();
                a.page = Page::Directories;
                a.prefs.mode = mode;
                a.prefs.global_root = Some("/g".into());
                let c = a.paint(&mut Field::new(width, 24, 1), 0, width, 24);
                let mut names = std::collections::BTreeSet::new();
                let mut bullets = std::collections::BTreeSet::new();
                let mut texts = std::collections::BTreeSet::new();
                for (label, text) in [
                    ("Default directory", a.mode_label()),
                    ("Global root", "/g"),
                    ("Space roots", UNSET),
                ] {
                    let y = row_y(&c, label);
                    let lx = col_of(&c, y, label).unwrap();
                    names.insert(lx);
                    bullets.insert((
                        lx - 2,
                        matches!(c.cells[y * width + lx - 2].glyph.as_str(), "●" | "○"),
                    ));
                    texts.insert(col_of(&c, y, text).unwrap());
                }
                assert_eq!(names.len(), 1, "w={width}");
                assert_eq!(bullets.len(), 1, "w={width} {bullets:?}");
                assert!(bullets.iter().all(|(_, b)| *b));
                assert_eq!(texts.len(), 1, "w={width} {texts:?}");
            }
        }
    }
    #[test]
    fn focused_spinner_preview_uses_the_selected_bullet_accent_at_its_cell() {
        use crate::render::Accent;
        let mut a = app();
        a.page = Page::Spinners;
        for selected in 0..3 {
            a.options.selected = selected;
            let c = a.paint(&mut Field::new(80, 24, 1), 5000, 80, 24);
            let s = a.prefs.spinners();
            let glyph = match selected {
                0 => crate::session_list::claude_frame(s.claude, 5000),
                1 => crate::session_list::codex_frame(s.codex, 5000),
                _ => crate::session_list::gemini_frame(s.gemini, 5000),
            };
            let label = ["Claude spinner", "Codex spinner", "Gemini spinner"][selected];
            let y = (0..24).find(|&y| col_of(&c, y, label).is_some()).unwrap();
            let row = &c.cells[y * 80..(y + 1) * 80];
            let x = row.iter().rposition(|cell| cell.glyph == glyph).unwrap();
            let want = c
                .theme
                .as_ref()
                .unwrap()
                .accent(
                    x,
                    y,
                    &Accent {
                        brightness: SELECTED_BULLET_LEVEL,
                        ..Accent::default()
                    },
                )
                .fg;
            assert_eq!(row[x].fg, want, "{label}");
        }
    }
    #[test]
    fn grabbed_selector_steps_every_other_element_back_and_restores_on_leaving() {
        use crate::render::Accent;
        use crate::widgets::{MUTED, SECONDARY};
        let mut a = app();
        a.page = Page::Spinners;
        a.options.selected = 1;
        let draw = |a: &mut App| a.paint(&mut Field::new(80, 24, 1), 2000, 80, 24);
        let bullet = |c: &Canvas, y: usize, label: &str| {
            let lx = col_of(c, y, label).unwrap();
            (lx - 2, c.cells[y * c.width + lx - 2].clone())
        };
        let dim = |c: &Canvas, x: usize, y: usize, bold: bool| {
            c.theme
                .as_ref()
                .unwrap()
                .accent(
                    x,
                    y,
                    &Accent {
                        brightness: 600,
                        bold,
                        ..Accent::default()
                    },
                )
                .fg
        };
        let glyphs = |c: &Canvas, y: usize| -> Vec<String> {
            c.cells[y * 80..(y + 1) * 80]
                .iter()
                .map(|c| c.glyph.clone())
                .collect()
        };
        let before = draw(&mut a);
        key(&mut a, KeyCode::Right);
        assert!(a.picker.is_some());
        let c = draw(&mut a);
        for (label, active) in [
            ("Claude spinner", false),
            ("Codex spinner", true),
            ("Gemini spinner", false),
        ] {
            let y = row_y(&c, label);
            let lx = col_of(&c, y, label).unwrap();
            let name = &c.cells[y * 80 + lx];
            // Names: active lighter and unbolded, others darker.
            assert_eq!(
                (name.fg, name.bold),
                (if active { SECONDARY } else { MUTED }, false),
                "{label}"
            );
            // Every bullet is the dim hue variant.
            let (bx, b) = bullet(&c, y, label);
            assert_eq!(b.fg, dim(&c, bx, y, active), "bullet {label}");
            let line = glyphs(&c, y);
            let value = lx + "Gemini spinner".len() + 1;
            let arrow = &c.cells[y * 80 + value];
            if active {
                // The active selector keeps its bold pulse arrow.
                assert!(arrow.bold && matches!(arrow.glyph.as_str(), "↕" | "↑" | "↓"));
            } else {
                // Other selectors keep their chevron in the dim hue; the value is
                // the darker grey.
                assert!(!line.concat().contains(['↕', '↑', '↓']), "{label}");
                assert_eq!(arrow.glyph, "›", "{label}");
                assert_eq!(arrow.fg, dim(&c, value, y, false), "{label} chevron");
                assert_eq!(c.cells[y * 80 + value + 2].fg, MUTED, "{label} value");
            }
        }
        // The back arrow left of the top option is the darker grey.
        let top = row_y(&c, "Claude spinner");
        let back = col_of(&c, top, "◀").unwrap();
        assert_eq!(c.cells[top * 80 + back].fg, MUTED);
        // Leaving the mode restores normal styling.
        key(&mut a, KeyCode::Esc);
        let after = draw(&mut a);
        assert!(a.picker.is_none());
        for y in 0..24 {
            for x in 0..80 {
                if before.cells[y * 80 + x].glyph == "◀" || (y, x) == (0, 0) {
                    assert_eq!(before.cells[y * 80 + x].fg, after.cells[y * 80 + x].fg);
                }
            }
        }
        let y = row_y(&after, "Claude spinner");
        assert_eq!(
            after.cells[y * 80 + col_of(&after, y, "◀").unwrap()].fg,
            SECONDARY
        );
        assert!(glyphs(&after, y).concat().contains('›'));
        let lx = col_of(&after, y, "Claude spinner").unwrap();
        assert_eq!(after.cells[y * 80 + lx].fg, SECONDARY);
        let (bx, b) = bullet(&after, y, "Claude spinner");
        assert_ne!(b.fg, dim(&after, bx, y, false));
    }
    /// Picker glyph cells of a settings page: (label x, glyph x, cell) per label.
    fn glyph_cell(c: &Canvas, label: &str) -> (usize, usize, Cell) {
        let y = row_y(c, label);
        let lx = col_of(c, y, label).unwrap();
        let gx = (lx + label.chars().count()..c.width)
            .find(|&x| {
                matches!(
                    c.cells[y * c.width + x].glyph.as_str(),
                    "›" | "↕" | "↑" | "↓"
                )
            })
            .unwrap_or_else(|| panic!("{label} has no glyph"));
        (lx, gx, c.cells[y * c.width + gx].clone())
    }
    fn hue(c: &Canvas, x: usize, y: usize, brightness: i64) -> [u8; 3] {
        c.theme
            .as_ref()
            .unwrap()
            .accent(
                x,
                y,
                &Accent {
                    brightness,
                    ..Accent::default()
                },
            )
            .fg
    }
    #[test]
    fn picker_glyph_is_a_chevron_outside_the_selector_and_directions_inside() {
        use crate::preferences::{ClaudeSpinner, CodexSpinner, GeminiSpinner};
        let mut a = app();
        a.page = Page::Spinners;
        a.prefs.claude_spinner = ClaudeSpinner::ALL[0];
        a.prefs.codex_spinner = CodexSpinner::ALL[1];
        a.prefs.gemini_spinner = *GeminiSpinner::ALL.last().unwrap();
        a.options.selected = 1;
        let labels = ["Claude spinner", "Codex spinner", "Gemini spinner"];
        let c = a.paint(&mut Field::new(80, 24, 1), 0, 80, 24);
        for label in labels {
            assert_eq!(glyph_cell(&c, label).2.glyph, "›", "{label}");
        }
        // Inside: the remaining directions on the grabbed row only.
        for (selected, want) in [(0usize, "↓"), (1, "↕"), (2, "↑")] {
            a.options.selected = selected;
            key(&mut a, KeyCode::Right);
            let c = a.paint(&mut Field::new(80, 24, 1), 0, 80, 24);
            for (i, label) in labels.iter().enumerate() {
                let want = if i == selected { want } else { "›" };
                assert_eq!(
                    glyph_cell(&c, label).2.glyph,
                    want,
                    "{label} sel={selected}"
                );
            }
            key(&mut a, KeyCode::Esc);
        }
        // Settings submenu rows whose value is only a preview carry no chevron.
        for page in [Page::Config, Page::Directories] {
            let mut a = app();
            a.page = page;
            a.prefs.global_root = Some("/g".into());
            let c = a.paint(&mut Field::new(120, 24, 1), 0, 120, 24);
            for row in a.rows().iter().filter(|r| r.settings_value) {
                let y = row_y(&c, &row.label);
                assert!(col_of(&c, y, "›").is_none(), "{}", row.label);
            }
        }
    }
    #[test]
    fn picker_chevron_colour_and_weight_follow_the_focus() {
        for page in [Page::Appearance, Page::Spinners] {
            let labels: Vec<String> = {
                let mut a = app();
                a.page = page.clone();
                a.rows().into_iter().map(|r| r.label).collect()
            };
            for now in [0u64, 2000, 4100] {
                let draw = |a: &mut App| a.paint(&mut Field::new(80, 24, 1), now, 80, 24);
                for hovered in 0..labels.len() {
                    // Default and hovered: the selected-bullet hue; bold only on hover.
                    let mut a = app();
                    a.page = page.clone();
                    a.options.selected = hovered;
                    let c = draw(&mut a);
                    for (i, label) in labels.iter().enumerate() {
                        let (_, x, cell) = glyph_cell(&c, label);
                        let y = row_y(&c, label);
                        assert_eq!(cell.glyph, "›");
                        assert_eq!(
                            (cell.fg, cell.bold),
                            (hue(&c, x, y, 1000), i == hovered),
                            "{page:?} {label} hovered={hovered} now={now}"
                        );
                    }
                    // Back arrow selected: the darker hue, never bold.
                    a.options.back = true;
                    let c = draw(&mut a);
                    for label in &labels {
                        let (_, x, cell) = glyph_cell(&c, label);
                        let y = row_y(&c, label);
                        assert_eq!((cell.fg, cell.bold), (hue(&c, x, y, 600), false), "back");
                    }
                    // Another selector grabbed: the others stay, in the darker hue.
                    a.options.back = false;
                    key(&mut a, KeyCode::Right);
                    let c = draw(&mut a);
                    for (i, label) in labels.iter().enumerate().filter(|(i, _)| *i != hovered) {
                        let (_, x, cell) = glyph_cell(&c, label);
                        let y = row_y(&c, label);
                        assert_eq!(
                            (cell.glyph.as_str(), cell.fg, cell.bold),
                            ("›", hue(&c, x, y, 600), false),
                            "{page:?} row {i} while {hovered} grabbed"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn grabbed_picker_arrow_is_bold_and_pulses_between_the_two_hue_levels() {
        use crate::render::ARROW_PULSE_MS;
        let mut a = app();
        a.page = Page::Spinners;
        a.options.selected = 1;
        key(&mut a, KeyCode::Right);
        let mut ends = (false, false);
        for now in (4000..4000 + ARROW_PULSE_MS).step_by(50) {
            let c = a.paint(&mut Field::new(80, 24, 1), now, 80, 24);
            let (_, x, cell) = glyph_cell(&c, "Codex spinner");
            let y = row_y(&c, "Codex spinner");
            let (light, dark) = (hue(&c, x, y, 1000), hue(&c, x, y, 600));
            assert!(cell.bold, "now={now}");
            for ch in 0..3 {
                let (lo, hi) = (light[ch].min(dark[ch]), light[ch].max(dark[ch]));
                assert!((lo..=hi).contains(&cell.fg[ch]), "now={now} {cell:?}");
            }
            let phase = now % ARROW_PULSE_MS;
            if phase == 0 {
                assert_eq!(cell.fg, light, "light hue at the start");
                ends.0 = true;
            }
            if phase == ARROW_PULSE_MS / 2 {
                assert_eq!(cell.fg, dark, "dim hue at mid-period");
                ends.1 = true;
            }
        }
        assert!(ends.0 && ends.1);
    }
    #[test]
    fn picker_glyphs_have_a_space_each_side_in_one_column_after_the_longest_name() {
        for (page, labels) in [
            (Page::Appearance, vec!["Starfield", "Brightness"]),
            (
                Page::Spinners,
                vec!["Claude spinner", "Codex spinner", "Gemini spinner"],
            ),
            (Page::Directories, vec!["Default directory"]),
        ] {
            for grabbed in [false, true] {
                for width in [50usize, 80, 120] {
                    let mut a = app();
                    a.page = page.clone();
                    if grabbed {
                        key(&mut a, KeyCode::Right);
                    }
                    let c = a.paint(&mut Field::new(width, 24, 1), 0, width, 24);
                    let longest = a
                        .rows()
                        .iter()
                        .map(|r| r.label.chars().count())
                        .max()
                        .unwrap();
                    let mut columns = std::collections::BTreeSet::new();
                    for label in &labels {
                        let (lx, gx, _) = glyph_cell(&c, label);
                        let y = row_y(&c, label);
                        columns.insert(gx);
                        assert_eq!(gx, lx + longest + 1, "{page:?} {label} w={width}");
                        assert_eq!(c.cells[y * width + gx - 1].glyph, " ");
                        assert_eq!(c.cells[y * width + gx + 1].glyph, " ");
                        assert_ne!(c.cells[y * width + gx + 2].glyph, " ", "value follows");
                    }
                    assert_eq!(columns.len(), 1, "{page:?}");
                }
            }
        }
    }
    #[test]
    fn settings_are_submenus_with_aligned_values_and_cycling_choices() {
        use crate::preferences::{Brightness, Starfield};
        let mut a = app();
        a.page = Page::Config;
        let rows = a.rows();
        assert_eq!(rows[0].label, "Stars");
        let v = rows[0].preview.as_deref().unwrap();
        assert_eq!(v, "Normal · Normal");
        assert_eq!(rows[1].label, "Default directories");
        assert_eq!(rows[0].preview_column, rows[1].preview_column);
        assert_eq!(rows[1].preview_fg, crate::widgets::MUTED);
        a.options.selected = 0;
        a.activate();
        assert_eq!(a.page, Page::Appearance);
        let rows = a.rows();
        assert_eq!(rows[0].preview.as_deref(), Some("↕ Normal"));
        assert!(rows[0].preview_choice && rows[1].preview_choice);
        key(&mut a, KeyCode::Right);
        assert!(a.picker.is_some(), "Right enters the picker");
        key(&mut a, KeyCode::Up);
        assert_eq!(a.prefs.starfield, Starfield::Dense);
        assert_eq!(a.rows()[0].preview.as_deref(), Some("↓ Dense"));
        key(&mut a, KeyCode::Up);
        assert_eq!(a.prefs.starfield, Starfield::Dense, "no wrap at the top");
        for _ in 0..3 {
            key(&mut a, KeyCode::Down);
        }
        assert_eq!(a.prefs.starfield, Starfield::Off);
        assert_eq!(a.rows()[0].preview.as_deref(), Some("↑ Off"));
        key(&mut a, KeyCode::Left);
        assert!(a.picker.is_none() && !a.options.back, "Left commits");
        assert_eq!(a.prefs.starfield, Starfield::Off);
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Enter);
        key(&mut a, KeyCode::Down);
        assert_eq!(a.prefs.brightness, Brightness::Dim);
        key(&mut a, KeyCode::Char(' '));
        assert!(a.picker.is_none());
        // Off paints no stars at all; leaving the page saves.
        let c = a.paint(&mut Field::new(80, 24, 1967), 4000, 80, 24);
        assert!(
            !c.cells
                .iter()
                .any(|c| matches!(c.glyph.as_str(), "*" | "+" | "."))
        );
        assert!(matches!(
            key(&mut a, KeyCode::Esc).first(),
            Some(Effect::Save)
        ));
        assert_eq!(a.page, Page::Config);
        a.options.selected = 1;
        a.activate();
        assert_eq!(a.page, Page::Directories);
        assert_eq!(a.rows()[0].preview.as_deref(), Some("↑ Global"));
        key(&mut a, KeyCode::Right);
        key(&mut a, KeyCode::Down);
        assert_eq!(a.prefs.mode, Mode::Global, "no wrap at the bottom");
        key(&mut a, KeyCode::Up);
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.prefs.mode, Mode::PerSpace);
        // Persisted beside the other preferences.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("preferences.json");
        a.prefs.save(&path).unwrap();
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.starfield, Starfield::Off);
        assert_eq!(loaded.brightness, Brightness::Dim);
        // Older files without the fields keep the defaults.
        std::fs::write(&path, r#"{"version":1,"mode":"global","global_root":null,"space_roots":{},"last_account":null}"#).unwrap();
        assert_eq!(
            Preferences::load(&path).unwrap().starfield,
            Starfield::Normal
        );
    }
    #[test]
    fn leave_unset_clears_only_the_selected_root() {
        for target in [Target::Global, Target::Space("w1".into())] {
            let mut a = app();
            let space_key = Preferences::key(&a.instance, "w1");
            a.prefs.global_root = Some("/tmp/global".into());
            a.prefs
                .space_roots
                .insert(space_key.clone(), "/tmp/space".into());
            a.push(Page::Directory(target.clone()), "/tmp".into());
            a.listing = Listing {
                path: Some("/tmp".into()),
                existing: true,
                children: vec!["/tmp/child".into()],
                ..Listing::default()
            };
            let rows = a.rows();
            assert_eq!(rows[rows.len() - 2].label, "Leave unset");
            a.options.selected = rows.len() - 2;
            a.activate();
            match target {
                Target::Global => {
                    assert!(a.prefs.global_root.is_none());
                    assert!(a.prefs.space_roots.contains_key(&space_key));
                }
                Target::Space(_) => {
                    assert!(!a.prefs.space_roots.contains_key(&space_key));
                    assert!(a.prefs.global_root.is_some());
                }
                _ => unreachable!(),
            }
        }
    }
    #[test]
    fn unavailable_options_can_be_hovered_but_not_activated() {
        let mut a = app();
        a.page = Page::Directories;
        a.prefs.mode = Mode::PerSpace;
        a.options.selected = 0;
        a.options.step(&a.rows(), false, false);
        assert_eq!(a.options.selected, 1);
        assert!(!a.rows()[1].enabled);
        assert!(a.activate().is_empty());
        assert_eq!(a.page, Page::Directories);
        assert_eq!(a.rows()[0].label, "Default directory");
    }
    #[test]
    fn root_paths_use_bare_gray_selected_previews_with_one_space_and_global_fit() {
        let mut a = app();
        a.prefs.global_root = Some("/projects/global".into());
        a.page = Page::Directories;
        let row = a.rows().remove(1);
        assert_eq!(row.preview.as_deref(), Some("/projects/global"));
        assert_eq!(row.preview_fg, crate::widgets::MUTED);
        assert!(!row.preview_selected_only);
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
            assert!(row(7).contains('•'));
            assert!(row(8).contains("Start session"));
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
        // New Session opens again while the first launch is still pending; the
        // unchanged draft cannot be submitted twice.
        a.management(vec![Intent::New]);
        assert_eq!(a.page, Page::Summary);
        assert_eq!(a.request, request);
        a.options.selected = 4;
        assert!(a.activate().is_empty());
        assert_eq!(
            a.manager.queue.len(),
            1,
            "only the first launch is registered"
        );
        // A changed draft is accepted: the launch slot is busy, so it queues
        // instead of starting, and the first request keeps being tracked.
        a.draft.name = "Second".into();
        a.draft.revision += 1;
        a.options.selected = 4;
        assert!(a.activate().iter().all(|e| !matches!(e, Effect::Start(_))));
        assert_eq!(a.request, request);
        assert_eq!(a.page, Page::Home);
        let states: Vec<_> = a.manager.queue.iter().map(|e| e.state).collect();
        use crate::op_queue::State;
        assert_eq!(states, [State::Running, State::Queued]);
        assert!(matches!(
            a.manager.queue_out.last(),
            Some(crate::op_queue::Command::Enqueue(e)) if e.kind == "launch"
        ));
    }
    fn mixed_app() -> App {
        let mut a = super::tests::app();
        a.accounts = serde_json::from_value(json!([
            {"id":"codex:a","selectable":true},
            {"id":"claude:c1","provider":"claude","selectable":true},
            {"id":"codex:b","selectable":true},
            {"id":"claude:c2","provider":"claude","selectable":true}
        ]))
        .unwrap();
        a.page = Page::Accounts;
        a.options.selected = a.opening_account_row();
        a
    }
    fn labels(a: &App) -> Vec<String> {
        a.rows()
            .into_iter()
            .filter(|r| !r.separator)
            .map(|r| r.label)
            .collect()
    }
    #[test]
    fn launcher_account_step_lists_both_providers_with_a_provider_filter() {
        let mut a = mixed_app();
        assert_eq!(
            labels(&a),
            ["Provider: All", "a", "c1", "b", "c2", "Cancel"]
        );
        assert_eq!(a.options.selected, 1, "opens on the first account");
        let tags: Vec<_> = a.rows().iter().filter_map(|r| r.preview.clone()).collect();
        assert!(tags[0].contains("Codex") && tags[1].contains("Claude Code"));
        // Filter row cycles Claude Code -> Codex -> All.
        a.options.selected = 0;
        a.activate();
        assert_eq!(labels(&a), ["Provider: Claude Code", "c1", "c2", "Cancel"]);
        a.options.selected = 0;
        a.activate();
        assert_eq!(labels(&a), ["Provider: Codex", "a", "b", "Cancel"]);
        a.options.selected = 0;
        a.activate();
        assert_eq!(a.rows()[0].label, "Provider: All");
        // Choose a Claude account through the filter: draft gets the qualified ID.
        a.options.selected = 0;
        a.activate();
        a.options.selected = 2;
        a.activate();
        assert_eq!(a.draft.account.as_deref(), Some("claude:c2"));
    }
    #[test]
    fn launcher_without_mixed_providers_has_no_filter_and_is_unchanged() {
        let mut a = super::tests::app();
        a.accounts = serde_json::from_value(json!([
            {"id":"codex:a","selectable":true},{"id":"codex:b","selectable":true}
        ]))
        .unwrap();
        a.page = Page::Accounts;
        assert_eq!(labels(&a), ["a", "b", "Cancel"]);
        let mut only_claude = super::tests::app();
        only_claude.accounts = serde_json::from_value(json!([
            {"id":"claude:c1","provider":"claude","selectable":true}
        ]))
        .unwrap();
        only_claude.page = Page::Accounts;
        only_claude.options.selected = 0;
        only_claude.activate();
        assert_eq!(only_claude.draft.account.as_deref(), Some("claude:c1"));
    }
    #[test]
    fn resume_and_switch_pickers_are_locked_to_the_session_provider() {
        let mut a = mixed_app();
        a.resume = Some(json!({"id":"s","provider":"claude","account_id":"claude:c1"}));
        assert_eq!(labels(&a), ["c1", "c2", "Cancel"]);
        a.resume = None;
        a.switch = Some(json!({"id":"s","account_id":"codex:a"}));
        assert_eq!(labels(&a), ["a", "b", "Cancel"]);
    }
}
