//! Operations queue popup, live status summary and queue-aware dispatch.
//!
//! The queue itself lives in `crate::op_queue` (persisted beside the preferences
//! and dispatched by a detached worker); the manager keeps a snapshot, sends edits
//! through `queue_out`, and derives every view from backend operations plus that
//! snapshot.
use super::*;
use crate::op_queue::{self, Action, Command, Entry, State};

/// Finished operations listed under Recent.
pub const RECENT_LIMIT: usize = 5;
/// Recent keeps finished operations created within this many seconds.
const RECENT_WINDOW_SECS: i64 = 86_400;
/// Longest failure reason shown on a Recent row.
const RECENT_REASON_WIDTH: usize = 56;

/// One row of the Operations queue popup.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum QueueSlot {
    Heading(String),
    Running(String),
    Queued(String),
    Recent(String),
    Spacer,
    All,
    Close,
}
impl QueueSlot {
    pub(super) fn key(&self) -> String {
        match self {
            Self::Heading(h) => format!("heading:{h}"),
            Self::Running(k) | Self::Queued(k) | Self::Recent(k) => k.clone(),
            Self::Spacer => "spacer".into(),
            Self::All => "all".into(),
            Self::Close => "close".into(),
        }
    }
}
/// A displayed operation: `op:<backend id>` or `q:<queue entry id>`.
#[derive(Clone, Debug)]
pub(super) struct Item {
    pub key: String,
    pub label: String,
    pub subject: String,
    pub started: Option<i64>,
    pub status: String,
    pub failed: bool,
}
/// Live elapsed counter in the session rows' shared `duration` format (`m:ss`).
pub(super) fn elapsed(now: i64, start: Option<i64>) -> String {
    start
        .map(|t| if t > 100_000_000_000 { t / 1000 } else { t })
        .map_or_else(String::new, |t| crate::session_list::duration(now - t))
}
/// Short noun for badges: `move`, `restart`, `switch`…
pub(super) fn kind_noun(kind: &str) -> &'static str {
    match kind {
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
        "launch" => "start",
        "account_delete" => "account delete",
        _ => "operation",
    }
}
impl Manager {
    /// Replace the queue snapshot (read back from `operation-queue.json`).
    pub fn set_queue(&mut self, entries: Vec<Entry>) {
        self.queue = entries;
    }
    /// Provider symbol and name for a queue resource.
    pub(super) fn resource_subject(&self, resource: &str) -> String {
        if let Some(session) = resource.strip_prefix("session:") {
            return self.operation_subject("operation", &json!({"source_session_id": session}));
        }
        if let Some(account) = resource.strip_prefix("account:") {
            return self.operation_subject("operation", &json!({"account_id": account}));
        }
        resource.into()
    }
    /// Whether `resource` is free for a direct start (else the request queues).
    pub fn can_start_now(&self, resource: &str) -> bool {
        let reserved = resource
            .strip_prefix("session:")
            .and_then(|s| self.session(s))
            .is_some_and(|s| s["pending_operation_id"].is_string());
        !reserved && op_queue::can_start(&self.queue, &self.operations, resource)
    }
    /// Route a backend-operation mutation through the queue. Returns true when
    /// it was queued; false means it starts directly (and is registered).
    pub(super) fn queue_or_register(&mut self, args: &[String]) -> bool {
        let Some((resource, kind)) = op_queue::classify(args) else {
            return false;
        };
        let subject = self.resource_subject(&resource);
        let mut entry = Entry::new(
            resource.clone(),
            kind,
            subject.clone(),
            Action::Api {
                args: args.to_vec(),
            },
        );
        entry.conversation = resource
            .strip_prefix("session:")
            .and_then(|s| self.session(s))
            .and_then(|s| s["conversation_id"].as_str())
            .map(str::to_owned);
        if self.can_start_now(&resource) {
            self.direct.insert(self.sequence + 1, entry.id.clone());
            let mut running = entry.clone();
            running.state = State::Running;
            running.started_at = Some(op_queue::now());
            self.queue.push(running);
            self.queue_out.push(Command::Register(entry));
            return false;
        }
        self.queue.push(entry.clone());
        self.queue_out.push(Command::Enqueue(entry));
        self.dialog = None;
        self.form = Form::None;
        self.foreground = None;
        self.details_open = false;
        true
    }
    /// A direct mutation answered: keep watching its operation, or forget it.
    pub(super) fn direct_answered(&mut self, token: u64, result: &Result<Value, String>) {
        let Some(entry) = self.direct.remove(&token) else {
            return;
        };
        match result {
            Ok(v) if v["phase"].is_string() && v["id"].is_string() => {
                let op = id(v).to_owned();
                if let Some(e) = self.queue.iter_mut().find(|e| e.id == entry) {
                    e.operation_id = Some(op.clone());
                }
                self.queue_out.push(Command::Admitted(entry, op));
            }
            _ => {
                self.queue.retain(|e| e.id != entry);
                self.queue_out.push(Command::Remove(entry));
            }
        }
    }
    /// Work in progress, newest first: backend operations, then queue entries the
    /// backend does not list yet.
    pub(super) fn running_items(&self) -> Vec<Item> {
        let mut items: Vec<Item> = self
            .operations
            .iter()
            .filter(|o| !terminal(o["phase"].as_str().unwrap_or("")))
            .map(|o| Item {
                key: format!("op:{}", id(o)),
                label: kind_label(o["kind"].as_str().unwrap_or("operation")),
                subject: self.operation_subject("operation", o),
                started: o["created_at"].as_i64(),
                status: operation_row_status("operation", o),
                failed: false,
            })
            .collect();
        for e in self.queue.iter().filter(|e| e.state == State::Running) {
            if e.operation_id
                .as_ref()
                .is_some_and(|op| self.operations.iter().any(|o| id(o) == op))
            {
                continue;
            }
            items.push(Item {
                key: format!("q:{}", e.id),
                label: kind_label(&e.kind),
                subject: e.subject.clone(),
                started: e.started_at.or(Some(e.created_at)),
                status: "starting".into(),
                failed: false,
            });
        }
        items.sort_by_key(|i| std::cmp::Reverse(i.started));
        items
    }
    pub(super) fn queued_items(&self) -> Vec<Item> {
        self.queue
            .iter()
            .filter(|e| e.state == State::Queued)
            .map(|e| Item {
                key: format!("q:{}", e.id),
                label: kind_label(&e.kind),
                subject: e.subject.clone(),
                started: Some(e.created_at),
                status: "queued".into(),
                failed: false,
            })
            .collect()
    }
    pub(super) fn recent_items(&self) -> Vec<Item> {
        let now = crate::usage::clock();
        let mut items: Vec<Item> = self
            .queue
            .iter()
            .filter(|e| e.state == State::Failed)
            .map(|e| Item {
                key: format!("q:{}", e.id),
                label: kind_label(&e.kind),
                subject: e.subject.clone(),
                started: e.started_at.or(Some(e.created_at)),
                status: match &e.detail {
                    Some(d) => format!("failed · {}", d.trim_end_matches('.')),
                    None => "failed".into(),
                },
                failed: true,
            })
            .collect();
        items.extend(
            self.operations
                .iter()
                .filter(|o| terminal(o["phase"].as_str().unwrap_or("")))
                .filter(|o| !self.dismissed.contains(id(o)))
                .filter(|o| {
                    o["created_at"]
                        .as_i64()
                        .is_none_or(|t| now - t < RECENT_WINDOW_SECS)
                })
                .map(|o| Item {
                    key: format!("op:{}", id(o)),
                    label: kind_label(o["kind"].as_str().unwrap_or("operation")),
                    subject: self.operation_subject("operation", o),
                    started: o["created_at"].as_i64(),
                    status: operation_row_status("operation", o),
                    failed: o["phase"] == "failed",
                }),
        );
        items.sort_by_key(|i| std::cmp::Reverse(i.started));
        items.truncate(RECENT_LIMIT);
        items
    }
    /// Counts for the bottom status lane: `2 running · 1 queued`, omitting a zero
    /// part; `None` while nothing is running or queued. Names stay in the popup.
    pub fn operation_summary(&self) -> Option<String> {
        let running = self.running_items().len();
        let queued = self.queued_items().len();
        let parts: Vec<String> = [(running, "running"), (queued, "queued")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, word)| format!("{n} {word}"))
            .collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
    /// The spinner and counts when they fit `width` whole; otherwise nothing.
    pub fn fitted_summary(&self, width: usize) -> Option<String> {
        use unicode_width::UnicodeWidthStr;
        let text = format!(
            "{} {}",
            crate::session_list::operation_spinner(self.anim_ms),
            self.operation_summary()?
        );
        (UnicodeWidthStr::width(text.as_str()) + 2 <= width).then_some(text)
    }
    pub(super) fn paint_summary(&self, canvas: &mut Canvas, controls_start: usize, text: &str) {
        use unicode_width::UnicodeWidthStr;
        let width = UnicodeWidthStr::width(text);
        let x = canvas.width.saturating_sub(width) / 2;
        let y = controls_start.saturating_sub(2);
        let (icon, rest) = text.split_once(' ').unwrap_or(("", text));
        canvas.accent_text(
            x,
            y,
            icon,
            crate::render::Accent {
                brightness: 1000,
                bold: false,
                flash: false,
                ..crate::render::Accent::default()
            },
        );
        canvas.widget_text(
            x + UnicodeWidthStr::width(icon) + 1,
            y,
            rest,
            crate::render::Cell {
                fg: crate::widgets::SECONDARY,
                ..crate::render::Cell::default()
            },
            false,
        );
    }
    /// Popup layout: Running, Queued and Recent sections, then All operations and Close.
    pub(super) fn queue_slots(&self) -> Vec<QueueSlot> {
        let mut slots = vec![];
        let running = self.running_items();
        let queued = self.queued_items();
        let recent = self.recent_items();
        if !running.is_empty() {
            slots.push(QueueSlot::Heading("Running".into()));
            slots.extend(running.into_iter().map(|i| QueueSlot::Running(i.key)));
        }
        if !queued.is_empty() {
            slots.push(QueueSlot::Heading("Queued".into()));
            slots.extend(queued.into_iter().map(|i| QueueSlot::Queued(i.key)));
        }
        if !recent.is_empty() {
            slots.push(QueueSlot::Heading("Recent".into()));
            slots.extend(recent.into_iter().map(|i| QueueSlot::Recent(i.key)));
        }
        if slots.is_empty() {
            slots.push(QueueSlot::Heading("Nothing running or queued".into()));
        }
        slots.extend([QueueSlot::Spacer, QueueSlot::All, QueueSlot::Close]);
        slots
    }
    pub(super) fn queue_rows(&self) -> Vec<Row> {
        use unicode_width::UnicodeWidthStr;
        let now = crate::usage::clock();
        let items: Vec<(String, Item)> = self
            .running_items()
            .into_iter()
            .map(|i| ("run", i))
            .chain(self.queued_items().into_iter().map(|i| ("queue", i)))
            .chain(self.recent_items().into_iter().map(|i| ("recent", i)))
            .map(|(section, i)| (format!("{section}:{}", i.key), i))
            .collect();
        let find = |section: &str, key: &str| {
            items
                .iter()
                .find(|(k, _)| k == &format!("{section}:{key}"))
                .map(|(_, i)| i)
        };
        let label_w = items
            .iter()
            .map(|(_, i)| UnicodeWidthStr::width(i.label.as_str()))
            .max()
            .unwrap_or(0);
        let subject_w = items
            .iter()
            .map(|(_, i)| {
                UnicodeWidthStr::width(crate::widgets::ellipsize(&i.subject, 24).as_str())
            })
            .max()
            .unwrap_or(0);
        let pad = |text: &str, w: usize| {
            format!(
                "{text}{}",
                " ".repeat(w.saturating_sub(UnicodeWidthStr::width(text)))
            )
        };
        let spinner = crate::session_list::operation_spinner(self.anim_ms);
        let row = |icon: &str, item: &Item, status: String| {
            let label = format!(
                "{icon} {}  {}",
                pad(&item.label, label_w),
                pad(&crate::widgets::ellipsize(&item.subject, 24), subject_w)
            );
            let mut row = Row::yes(label).with_reason(status);
            row.shorten(
                UnicodeWidthStr::width(icon) + 1 + label_w + 2,
                UnicodeWidthStr::width(crate::widgets::ellipsize(&item.subject, 24).as_str()),
                &item.subject,
            );
            row.preview = row.preview.map(|p| p.trim_start_matches("· ").to_owned());
            row.preview_column = Some(2 + 2 + label_w + 2 + subject_w + 2);
            row
        };
        let mut queued_position = 0;
        self.queue_slots()
            .into_iter()
            .map(|slot| match &slot {
                QueueSlot::Heading(h) => Row::heading(h.clone()),
                QueueSlot::Running(k) => match find("run", k) {
                    Some(i) => row(spinner, i, elapsed(now, i.started)),
                    None => Row::yes("Unknown operation"),
                },
                QueueSlot::Queued(k) => match find("queue", k) {
                    Some(i) => {
                        queued_position += 1;
                        row(
                            crate::session_list::QUEUED_ICON,
                            i,
                            format!("queued #{queued_position}"),
                        )
                    }
                    None => Row::yes("Unknown operation"),
                },
                QueueSlot::Recent(k) => match find("recent", k) {
                    Some(i) if i.failed => {
                        // A failure's reason is the point of the row: the popup
                        // widens for it (shortened to a readable length).
                        let mut r = row(
                            "✗",
                            i,
                            crate::widgets::ellipsize(&i.status, RECENT_REASON_WIDTH),
                        );
                        if UnicodeWidthStr::width(i.status.as_str()) > RECENT_REASON_WIDTH {
                            r.preview_full = Some(i.status.clone());
                        }
                        r.preview_info = false;
                        r
                    }
                    Some(i) => row("✓", i, i.status.clone()),
                    None => Row::yes("Unknown operation"),
                },
                QueueSlot::Spacer => Row::separator(),
                QueueSlot::All => Row::yes("All operations"),
                QueueSlot::Close => Row::yes("Close"),
            })
            .collect()
    }
    /// Keep the selected item through live updates; fall back to the nearest row.
    pub(super) fn retain_queue_focus(&mut self) {
        let slots = self.queue_slots();
        if let Some(key) = &self.queue_focus
            && let Some(i) = slots.iter().position(|s| &s.key() == key)
        {
            self.options.selected = i;
            return;
        }
        let rows = self.queue_rows();
        let mut i = self.options.selected.min(rows.len().saturating_sub(1));
        if rows.get(i).is_some_and(|r| r.separator) {
            i = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| !r.separator)
                .min_by_key(|(index, _)| index.abs_diff(i))
                .map(|(index, _)| index)
                .unwrap_or(0);
        }
        self.options.selected = i;
        self.queue_focus = slots.get(i).map(QueueSlot::key);
    }
    pub(super) fn note_queue_focus(&mut self) {
        if self.page == Page::Queue {
            self.queue_focus = self
                .queue_slots()
                .get(self.options.selected)
                .map(QueueSlot::key);
        }
    }
    /// Open the queue popup over the current page.
    pub fn open_queue(&mut self) -> Vec<Intent> {
        if self.page == Page::Queue {
            return vec![];
        }
        self.enter(Page::Queue);
        self.queue_focus = None;
        self.retain_queue_focus();
        self.refresh()
    }
    pub(super) fn activate_queue(&mut self) -> Vec<Intent> {
        let Some(slot) = self.queue_slots().get(self.options.selected).cloned() else {
            return vec![];
        };
        match slot {
            QueueSlot::Close => self.back(),
            QueueSlot::All => {
                self.enter(Page::Operations);
                self.refresh()
            }
            QueueSlot::Running(k) | QueueSlot::Recent(k) if k.starts_with("op:") => {
                self.open_operation(k.trim_start_matches("op:").to_owned())
            }
            QueueSlot::Running(k) | QueueSlot::Queued(k) | QueueSlot::Recent(k) => {
                self.open_queue_entry(k.trim_start_matches("q:"));
                vec![]
            }
            QueueSlot::Heading(_) | QueueSlot::Spacer => vec![],
        }
    }
    /// A queue entry's dialog: queued ones offer Cancel, failed ones Dismiss.
    pub(super) fn open_queue_entry(&mut self, key: &str) {
        let Some(e) = self.queue.iter().find(|e| e.id == key).cloned() else {
            return;
        };
        let title = kind_label(&e.kind);
        match e.state {
            State::Queued => {
                let ahead = self
                    .queue
                    .iter()
                    .take_while(|o| o.id != e.id)
                    .filter(|o| o.resource == e.resource && o.state != State::Failed)
                    .count();
                let body = vec![
                    format!("{} is waiting to start.", e.subject),
                    if ahead == 0 {
                        "It starts when a slot is free.".into()
                    } else {
                        format!("It starts after {ahead} earlier operation(s) on this target.")
                    },
                    "Cancel removes it before it starts.".into(),
                ];
                self.show(
                    &title,
                    body,
                    &["Cancel", "Close"],
                    Form::Queued(e.id.clone()),
                );
                if let Some(d) = &mut self.dialog {
                    d.selected = 1;
                }
            }
            State::Running => {
                self.show(
                    &title,
                    vec![
                        format!("Starting {}…", e.subject),
                        "The backend has not confirmed it yet. It is never resent automatically."
                            .into(),
                    ],
                    &["Close"],
                    Form::None,
                );
            }
            State::Failed => {
                let mut body = vec![format!("{} did not complete.", e.subject)];
                if let Some(d) = &e.detail {
                    body.push(d.clone());
                }
                self.show(
                    &title,
                    body,
                    &["Dismiss", "Close"],
                    Form::QueuedFailed(e.id.clone()),
                );
                if let Some(d) = &mut self.dialog {
                    d.selected = 1;
                }
            }
        }
        self.details_open = true;
    }
    /// Cancel a queued entry before it starts; started ones are left alone.
    pub(super) fn cancel_queued(&mut self, key: &str) {
        if !self
            .queue
            .iter()
            .any(|e| e.id == key && e.state == State::Queued)
        {
            return;
        }
        self.queue.retain(|o| o.id != key);
        self.queue_out.push(Command::Cancel(key.to_owned()));
    }
    pub(super) fn dismiss_queued(&mut self, key: &str) {
        self.queue.retain(|o| o.id != key);
        self.queue_out.push(Command::Remove(key.to_owned()));
    }
    /// The session's most recent finished outcome, from backend operations and
    /// failed queue entries, as a badge only when it is a failure inside the
    /// recency window: a newer success (or a dismissed failure) hides older ones.
    pub(super) fn outcome_badge(
        &self,
        session: &str,
        s: &Value,
        now: i64,
    ) -> Option<crate::session_list::Badge> {
        let secs = |t: i64| if t > 100_000_000_000 { t / 1000 } else { t };
        let backend = self
            .operations
            .iter()
            .filter(|o| {
                (o["source_session_id"] == session
                    || o["session_id"] == session
                    || s["pending_operation_id"].as_str() == Some(id(o)))
                    && terminal(o["phase"].as_str().unwrap_or(""))
            })
            .map(|o| {
                let at = o["updated_at"].as_i64().or(o["created_at"].as_i64());
                let failed = o["phase"] == "failed" && !self.dismissed.contains(id(o));
                let badge = failed.then(|| crate::session_list::Badge {
                    kind: "operation".into(),
                    id: id(o).to_owned(),
                    text: failed_text(o["kind"].as_str().unwrap_or("operation")),
                    active: false,
                    queued: false,
                });
                (at.map(secs), badge)
            });
        let resource = format!("session:{session}");
        let local = self
            .queue
            .iter()
            .filter(|e| e.resource == resource && e.state == State::Failed)
            .map(|e| {
                let at = e.finished_at.or(e.started_at).unwrap_or(e.created_at);
                let badge = crate::session_list::Badge {
                    kind: "queue".into(),
                    id: e.id.clone(),
                    text: format!("{} failed", kind_noun(&e.kind)),
                    active: false,
                    queued: false,
                };
                (Some(secs(at)), Some(badge))
            });
        let (at, badge) = backend.chain(local).max_by_key(|(at, _)| *at)?;
        // Undated outcomes cannot be placed in the window; show them only alone.
        badge.filter(|_| at.is_none_or(|t| now - t < RECENT_WINDOW_SECS))
    }
    /// Queue state for a session row badge: starting or queued.
    pub(super) fn queue_badge(&self, session: &str) -> Option<crate::session_list::Badge> {
        let resource = format!("session:{session}");
        let mine: Vec<&Entry> = self
            .queue
            .iter()
            .filter(|e| e.resource == resource)
            .collect();
        let badge =
            |e: &Entry, text: String, active: bool, queued: bool| crate::session_list::Badge {
                kind: "queue".into(),
                id: e.id.clone(),
                text,
                active,
                queued,
            };
        if let Some(e) = mine.iter().find(|e| {
            e.state == State::Running
                && !e
                    .operation_id
                    .as_ref()
                    .is_some_and(|op| self.operations.iter().any(|o| id(o) == op))
        }) {
            return Some(badge(e, "starting".into(), true, false));
        }
        if let Some(e) = mine.iter().find(|e| e.state == State::Queued) {
            return Some(badge(
                e,
                format!("{} queued", kind_noun(&e.kind)),
                false,
                true,
            ));
        }
        None
    }
}
impl Manager {
    /// The Operations popup over the page it was opened from.
    pub(super) fn paint_queue(&mut self, canvas: &mut Canvas, footer: usize, now: i64) {
        self.retain_queue_focus();
        if let Some(beneath) = self.history.pop() {
            let options = std::mem::replace(&mut self.options, beneath.options.clone());
            let dialog = self.dialog.take();
            let note = std::mem::take(&mut self.note);
            let popup_size = self.popup_size.take();
            self.page = beneath.page.clone();
            self.paint_at(canvas, footer, now);
            self.page = Page::Queue;
            self.options = options;
            self.dialog = dialog;
            self.note = note;
            self.popup_size = popup_size;
            self.history.push(beneath);
        }
        let measured = self.queue_rows();
        let mut rows = measured.clone();
        let palette = self.palette.clone();
        let description = format!(
            "Up to {} operations run at once; one at a time per session or account.",
            op_queue::MAX_CONCURRENT
        );
        let mut view = crate::popup::Choices {
            title: "Operations",
            description: &description,
            rows: &measured,
            editor: None,
            note: &self.note,
            note_accent: false,
            large: false,
            fit_description: false,
            hints: &["▲/▼: Choose", "Enter: Select", "Esc: Close"],
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
        // Failure reasons shorten to the room the popup has instead of vanishing.
        let inner = self.popup_size.unwrap().width.saturating_sub(4);
        let left = crate::widgets::Options::left(inner, &rows);
        for row in rows.iter_mut().filter(|r| !r.preview_info) {
            if let (Some(column), Some(_)) = (row.preview_column, &row.preview) {
                let room = inner.saturating_sub(left + column + 1);
                row.shorten_preview(room, crate::widgets::ellipsize);
            }
        }
        let view = crate::popup::Choices {
            rows: &rows,
            ..view
        };
        crate::popup::paint_choices(
            canvas,
            &palette,
            &view,
            &mut self.options,
            self.popup_size.unwrap(),
        );
        // The popup and its dialogs draw their own controls; the main footer
        // shows none of them, nor those of the page beneath.
        let start = canvas.height.saturating_sub(2);
        for cell in &mut canvas.cells[start * canvas.width..] {
            *cell = crate::render::Cell::default();
        }
        if let Some(d) = &mut self.dialog {
            d.paint(canvas, &self.palette);
        }
    }
}
impl Manager {
    /// A launch receipt reached a settled phase: stop counting it as running.
    pub fn launch_settled(&mut self, request: &str) {
        let settled: Vec<String> = self
            .queue
            .iter()
            .filter(|e| {
                e.state == State::Running
                    && matches!(e.action, Action::Launch { .. })
                    && e.operation_id.as_deref() == Some(request)
            })
            .map(|e| e.id.clone())
            .collect();
        for key in settled {
            self.queue.retain(|e| e.id != key);
            self.queue_out.push(Command::Remove(key));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    fn key(m: &mut Manager, code: KeyCode) -> Vec<Intent> {
        m.key(KeyEvent::new(code, KeyModifiers::NONE))
    }
    fn queued(session: &str, kind: &str) -> Entry {
        Entry::new(
            format!("session:{session}"),
            kind,
            format!("⏣ {session}"),
            Action::Api {
                args: vec![kind.into(), "--session".into(), session.into()],
            },
        )
    }
    fn running_op(id: &str, session: &str) -> Value {
        json!({"id":id,"kind":"move_space","phase":"dispatching","source_session_id":session,"created_at":90})
    }
    fn manager() -> Manager {
        Manager {
            sessions_known: true,
            sessions: vec![
                json!({"id":"a","name":"alpha","account_id":"codex:x","availability":"running","activity":"idle","generation":"g"}),
                json!({"id":"b","name":"beta","account_id":"codex:x","availability":"running","activity":"idle","generation":"g"}),
            ],
            ..Manager::default()
        }
    }
    #[test]
    fn status_summary_counts_running_and_queued_without_names() {
        let mut m = manager();
        assert_eq!(m.operation_summary(), None, "hidden when idle");
        assert_eq!(m.fitted_summary(80), None);
        m.operations = vec![running_op("o1", "a")];
        assert_eq!(m.operation_summary().as_deref(), Some("1 running"));
        m.anim_ms = 0;
        assert_eq!(m.fitted_summary(80).as_deref(), Some("◜ 1 running"));
        m.operations.push(running_op("o2", "b"));
        m.queue.push(queued("a", "restart"));
        assert_eq!(
            m.operation_summary().as_deref(),
            Some("2 running · 1 queued")
        );
        m.anim_ms = crate::session_list::OPERATION_SPINNER_FRAME_MS;
        assert_eq!(
            m.fitted_summary(80).as_deref(),
            Some("◝ 2 running · 1 queued")
        );
        // All or nothing: too narrow shows nothing rather than a clipped line.
        assert_eq!(m.fitted_summary(12), None);
        m.operations.clear();
        assert_eq!(m.operation_summary().as_deref(), Some("1 queued"));
    }
    #[test]
    fn summary_replaces_the_status_lane_only_without_a_notice() {
        let mut m = manager();
        m.page = Page::Sessions;
        m.operations = vec![running_op("o1", "a")];
        m.queue.push(queued("b", "restart"));
        let text = |m: &mut Manager| {
            let mut c = crate::render::Field::new(80, 20, 1967).paint(0);
            m.paint_at(&mut c, 19, 100);
            c.cells
                .chunks(80)
                .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>())
                .collect::<Vec<_>>()
        };
        let lines = text(&mut m);
        assert!(lines.iter().any(|l| l.contains("◜ 1 running · 1 queued")));
        assert!(lines.iter().any(|l| l.contains("o: Operations")));
        m.note = "Session changed".into();
        let lines = text(&mut m);
        assert!(!lines.iter().any(|l| l.contains("running")));
    }
    #[test]
    fn busy_session_mutation_queues_and_free_one_starts_directly() {
        let mut m = manager();
        m.operations = vec![running_op("o1", "a")];
        let args = |s: &str| {
            vec![
                "restart".to_owned(),
                "--session".into(),
                s.into(),
                "--expected-generation".into(),
                "g".into(),
            ]
        };
        assert!(
            m.mutate(args("a")).is_empty(),
            "busy target: queued, nothing sent"
        );
        assert_eq!(m.queue[0].state, State::Queued);
        assert!(matches!(m.queue_out[0], Command::Enqueue(_)));
        let sent = m.mutate(args("b"));
        let Some(Intent::Api(token, _)) = sent.first() else {
            panic!("free target starts directly")
        };
        assert_eq!(m.queue[1].state, State::Running);
        m.response(
            *token,
            Ok(json!({"id":"o2","kind":"restart","phase":"queued"})),
        );
        assert_eq!(m.queue[1].operation_id.as_deref(), Some("o2"));
        // The session row shows a distinct queued badge.
        let badges = m.badges();
        assert!(badges["a"].active, "running backend work wins on its row");
        m.operations.clear();
        let badge = &m.badges()["a"];
        assert!(badge.queued && badge.text == "restart queued");
        assert_eq!(badge.icon(0), crate::session_list::QUEUED_ICON);
    }
    #[test]
    fn queue_popup_sections_navigation_cancel_and_back() {
        let mut m = manager();
        m.page = Page::Sessions;
        m.operations = vec![
            running_op("o1", "a"),
            json!({"id":"o9","kind":"restart","phase":"complete","source_session_id":"b","created_at":crate::usage::clock()}),
        ];
        let first = queued("a", "restart");
        let second = queued("a", "stop");
        m.queue = vec![first.clone(), second.clone()];
        // Opened from the existing Operations entry, at the first item.
        m.options.selected = m.rows().len() - 1;
        m.activate();
        assert_eq!(m.page, Page::Queue);
        let labels: Vec<String> = m.rows().iter().map(|r| r.label.clone()).collect();
        let at = |l: &str| labels.iter().position(|x| x.contains(l)).unwrap();
        assert!(at("Running") < at("Move to Space"));
        assert!(at("Queued") < at("Restart") && at("Restart") < at("End session"));
        assert!(at("Recent") < at("✓"));
        assert!(labels.ends_with(&["".into(), "All operations".into(), "Close".into()]));
        assert_eq!(m.options.selected, 1, "the cursor starts on the first item");
        assert!(m.rows()[1].label.starts_with('◜'));
        // Down skips headings; the selection follows its item through updates.
        key(&mut m, KeyCode::Down);
        assert_eq!(m.options.selected, at("Restart"));
        m.queue.retain(|e| e.id != first.id); // the first queued one started elsewhere
        m.queue.insert(0, {
            let mut e = first.clone();
            e.state = State::Running;
            e
        });
        m.retain_queue_focus();
        assert!(m.rows()[m.options.selected].label.contains("Restart"));
        // Select the second queued item: Cancel and Close.
        m.options.selected = m
            .rows()
            .iter()
            .position(|r| r.label.contains("End session"))
            .unwrap();
        m.note_queue_focus();
        key(&mut m, KeyCode::Enter);
        let d = m.dialog.as_ref().unwrap();
        let buttons: Vec<_> = d.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(buttons, ["Cancel", "Close"]);
        assert_eq!(d.selected, 1, "Close is the safe default");
        key(&mut m, KeyCode::Up);
        key(&mut m, KeyCode::Enter);
        assert!(m.dialog.is_none());
        assert!(!m.queue.iter().any(|e| e.id == second.id));
        assert_eq!(
            m.queue_out.last(),
            Some(&Command::Cancel(second.id.clone()))
        );
        assert_eq!(m.page, Page::Queue);
        // Back returns to the opener at its remembered row.
        key(&mut m, KeyCode::Left);
        assert_eq!(m.page, Page::Sessions);
        assert_eq!(m.options.selected, m.rows().len() - 1);
        // `o` opens it from the list too, and Esc closes it.
        key(&mut m, KeyCode::Char('o'));
        assert_eq!(m.page, Page::Queue);
        key(&mut m, KeyCode::Esc);
        assert_eq!(m.page, Page::Sessions);
    }
    #[test]
    fn queue_popup_paints_over_its_opener_with_its_own_footer() {
        let mut m = manager();
        m.page = Page::Sessions;
        m.operations = vec![running_op("o1", "a")];
        m.queue = vec![queued("a", "restart")];
        m.open_queue();
        let mut c = crate::render::Field::new(90, 24, 1967).paint(0);
        m.paint_at(&mut c, 23, 100);
        let lines: Vec<String> = c
            .cells
            .chunks(90)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>())
            .collect();
        let screen = lines.join("\n");
        assert!(screen.contains("Operations"));
        assert!(screen.contains("Running") && screen.contains("Queued"));
        assert!(screen.contains("queued #1"));
        // The popup's controls sit inside its own frame; the main footer has none.
        assert!(!lines[22].contains("Esc:") && !lines[23].contains("Esc:"));
        let hint = lines.iter().position(|l| l.contains("Esc: Close")).unwrap();
        assert!(lines[hint + 1].contains('└'), "{screen}");
    }
    #[test]
    fn row_badge_shows_only_the_most_recent_outcome_when_it_is_a_recent_failure() {
        let now = crate::usage::clock();
        let mut m = manager();
        let failed_at = |at: i64| {
            let mut e = queued("a", "restart");
            e.state = State::Failed;
            e.finished_at = Some(at);
            e
        };
        let op = |id: &str, phase: &str, at: i64| json!({"id":id,"kind":"move_space","phase":phase,"source_session_id":"a","created_at":at});
        // A queue failure older than a later success is not shown.
        m.queue = vec![failed_at(now - 100)];
        m.operations = vec![op("ok", "complete", now - 10)];
        assert!(!m.badges().contains_key("a"), "newer success hides it");
        // A failure newer than the latest success is.
        m.queue = vec![failed_at(now - 5)];
        let badge = &m.badges()["a"];
        assert_eq!(
            (badge.kind.as_str(), badge.text.as_str()),
            ("queue", "restart failed")
        );
        // Old failures age out of the window.
        m.operations.clear();
        m.queue = vec![failed_at(now - RECENT_WINDOW_SECS - 1)];
        assert!(!m.badges().contains_key("a"));
        // The newest of a backend and a queue failure wins.
        m.queue = vec![failed_at(now - 50)];
        m.operations = vec![op("bad", "failed", now - 20)];
        assert_eq!(m.badges()["a"].id, "bad");
        m.queue = vec![failed_at(now - 1)];
        assert_eq!(m.badges()["a"].kind, "queue");
        // Running backend work and queued work still come first.
        m.operations.push(op("live", "dispatching", now));
        assert!(m.badges()["a"].active);
        m.operations.pop();
        m.queue.push(queued("a", "stop"));
        assert!(m.badges()["a"].queued);
        // A dismissed failure is still the latest outcome: older ones stay hidden.
        m.queue = vec![failed_at(now - 100)];
        m.operations = vec![op("bad", "failed", now - 20)];
        m.dismissed.insert("bad".into());
        assert!(!m.badges().contains_key("a"));
    }
    #[test]
    fn failed_recent_rows_show_their_reason() {
        let mut m = manager();
        m.page = Page::Sessions;
        let mut e = queued("a", "restart");
        e.state = State::Failed;
        e.detail = Some(op_queue::UNCERTAIN_DETAIL.into());
        m.queue = vec![e];
        m.operations = vec![
            json!({"id":"o9","kind":"restart","phase":"failed","detail":"The terminal could not be reached.","source_session_id":"b","created_at":crate::usage::clock()}),
        ];
        m.open_queue();
        let mut c = crate::render::Field::new(120, 24, 1967).paint(0);
        m.paint_at(&mut c, 23, 100);
        let screen: String = c
            .cells
            .chunks(120)
            .map(|r| r.iter().map(|c| c.glyph.as_str()).collect::<String>() + "\n")
            .collect();
        assert!(screen.contains("Outcome uncertain"), "{screen}");
        assert!(screen.contains("terminal could not"), "{screen}");
    }
    #[test]
    fn elapsed_uses_the_shared_live_counter_format() {
        assert_eq!(elapsed(100, Some(88)), "0:12");
        assert_eq!(elapsed(400, Some(100)), "5:00");
        assert_eq!(elapsed(7_400, Some(100)), "2:01:40");
        assert_eq!(elapsed(100, None), "");
    }
}
