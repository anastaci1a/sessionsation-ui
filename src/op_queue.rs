//! UI operation queue. The backend has no queue: it runs operations on different
//! conversations concurrently and refuses a second one on a reserved conversation
//! (`SESSION_OPERATION_IN_PROGRESS`). This queue holds what the user asked for
//! while its resource (session or account) was busy or the concurrency cap was
//! reached. It lives beside the preferences (`operation-queue.json`), scoped to
//! the backend root, and a detached one-shot worker dispatches it, so closing the
//! popup neither drops nor runs anything twice.
//!
//! Safety rules: an entry is marked running and saved before its single dispatch
//! attempt, so a crash can never replay it. Uncertain outcomes become failed
//! entries; nothing is retried automatically. Running work is never cancelled.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

/// Operations on different resources that may run at the same time.
pub const MAX_CONCURRENT: usize = 3;
/// A running entry that never learned its backend ID settles as unconfirmed.
pub const ADMISSION_GRACE_SECS: i64 = 60;
/// Worker polling interval.
pub const WORKER_POLL_MS: u64 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Queued,
    Running,
    Failed,
}
/// What one entry runs: a public API mutation or a journaled launch request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Action {
    Api { args: Vec<String> },
    Launch { kind: String, request: Value },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    /// `session:<id>` or `account:<id>`; one runs at a time per resource.
    pub resource: String,
    /// Backend kind code (`move_space`, `restart`, `launch`, …).
    pub kind: String,
    /// Display subject: provider symbol and session or account name.
    pub subject: String,
    pub action: Action,
    pub state: State,
    pub created_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    /// Backend operation ID, or the launch request ID, once known.
    #[serde(default)]
    pub operation_id: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    /// When the entry failed; badges show only recent outcomes.
    #[serde(default)]
    pub finished_at: Option<i64>,
    /// Conversation of the target session when queued. Dispatch takes the
    /// session's current generation only while it is the same conversation lineage.
    #[serde(default)]
    pub conversation: Option<String>,
}
impl Entry {
    pub fn new(resource: String, kind: &str, subject: String, action: Action) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            resource,
            kind: kind.into(),
            subject,
            action,
            state: State::Queued,
            created_at: now(),
            started_at: None,
            operation_id: None,
            detail: None,
            finished_at: None,
            conversation: None,
        }
    }
}
/// Resource and backend kind of a mutation that runs as a backend operation.
/// Instant edits (rename, limits, settings, responses) are not queued.
pub fn classify(args: &[String]) -> Option<(String, &'static str)> {
    let arg = |i: usize| args.get(i).map(String::as_str);
    if arg(0) == Some("account")
        && arg(1) == Some("delete")
        && args.iter().any(|a| a == "--confirm")
    {
        return Some((format!("account:{}", arg(2)?), "account_delete"));
    }
    let kind = match arg(0)? {
        "switch" => "switch",
        "move-space" => "move_space",
        "relocate" => "relocate",
        "restart" => "restart",
        "stop" => "stop",
        "archive" => "archive_session",
        "delete-session" => "delete_session",
        "restore" => "restore_session",
        _ => return None,
    };
    let session = args.windows(2).find(|w| w[0] == "--session")?[1].clone();
    Some((format!("session:{session}"), kind))
}
/// Resources reserved by backend operations that have not reached a terminal phase.
pub fn backend_busy(operations: &[Value]) -> BTreeSet<String> {
    operations
        .iter()
        .filter(|o| !terminal(o["phase"].as_str().unwrap_or("")))
        .flat_map(|o| {
            let session = o["source_session_id"]
                .as_str()
                .or_else(|| o["session_id"].as_str())
                .map(|s| format!("session:{s}"));
            let account = (o["kind"] == "account_delete")
                .then(|| o["account_id"].as_str().map(|a| format!("account:{a}")))
                .flatten();
            session.into_iter().chain(account)
        })
        .collect()
}
/// Backend operation IDs that are still in progress.
pub fn backend_running(operations: &[Value]) -> BTreeSet<String> {
    operations
        .iter()
        .filter(|o| !terminal(o["phase"].as_str().unwrap_or("")))
        .filter_map(|o| o["id"].as_str().map(str::to_owned))
        .collect()
}
pub fn terminal(phase: &str) -> bool {
    matches!(
        phase,
        "complete" | "completed" | "cancelled" | "failed" | "recovered"
    )
}
/// Running count: queue entries plus backend operations the queue does not track.
pub fn running_count(entries: &[Entry], operations: &[Value]) -> usize {
    let tracked: BTreeSet<&str> = entries
        .iter()
        .filter(|e| e.state == State::Running)
        .filter_map(|e| e.operation_id.as_deref())
        .collect();
    entries.iter().filter(|e| e.state == State::Running).count()
        + backend_running(operations)
            .iter()
            .filter(|id| !tracked.contains(id.as_str()))
            .count()
}
/// Whether a new request for `resource` may start now instead of queueing.
pub fn can_start(entries: &[Entry], operations: &[Value], resource: &str) -> bool {
    !entries
        .iter()
        .any(|e| e.resource == resource && e.state != State::Failed)
        && !backend_busy(operations).contains(resource)
        && running_count(entries, operations) < MAX_CONCURRENT
}
/// Indices of queued entries to dispatch now: FIFO per resource, oldest first,
/// never more than the cap in total, never two on one resource.
pub fn ready(entries: &[Entry], operations: &[Value]) -> Vec<usize> {
    let mut busy = backend_busy(operations);
    busy.extend(
        entries
            .iter()
            .filter(|e| e.state == State::Running)
            .map(|e| e.resource.clone()),
    );
    let mut running = running_count(entries, operations);
    let mut out = vec![];
    for (i, e) in entries.iter().enumerate() {
        if e.state != State::Queued {
            continue;
        }
        // An earlier queued entry claims its resource, even when it cannot start.
        if !busy.insert(e.resource.clone()) {
            continue;
        }
        if running >= MAX_CONCURRENT {
            continue;
        }
        running += 1;
        out.push(i);
    }
    out
}
/// The durable queue file and its short read-modify-write lock.
#[derive(Clone, Debug)]
pub struct Store {
    pub path: PathBuf,
    pub root: PathBuf,
}
impl Store {
    pub fn beside(preferences: &Path, root: &Path) -> Self {
        Self {
            path: preferences.with_file_name("operation-queue.json"),
            root: root.into(),
        }
    }
    fn lock_path(&self) -> PathBuf {
        self.path.with_extension("lock")
    }
    pub fn worker_lock_path(&self) -> PathBuf {
        self.path.with_extension("worker.lock")
    }
    /// True while a worker holds this queue (its lock cannot be taken).
    pub fn worker_running(&self) -> bool {
        use std::os::fd::AsRawFd;
        let Ok(lock) = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.worker_lock_path())
        else {
            return false;
        };
        // SAFETY: flock on a valid, owned descriptor; released when it closes.
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) != 0 }
    }
    /// Entries written for this backend root; a missing or foreign file is empty.
    pub fn load(&self) -> Vec<Entry> {
        let Ok(bytes) = fs::read(&self.path) else {
            return vec![];
        };
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
            return vec![];
        };
        if v["version"] != 1 || v["root"] != json!(self.root) {
            return vec![];
        }
        serde_json::from_value(v["entries"].clone()).unwrap_or_default()
    }
    /// Apply `change` under the exclusive lock and save atomically.
    pub fn update<R>(&self, change: impl FnOnce(&mut Vec<Entry>) -> R) -> Result<(R, Vec<Entry>)> {
        use std::os::fd::AsRawFd;
        let parent = self.path.parent().context("Queue path has no parent")?;
        fs::create_dir_all(parent)?;
        let lock = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.lock_path())?;
        // SAFETY: flock on a valid, owned descriptor.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut entries = self.load();
        let before = entries.clone();
        let result = change(&mut entries);
        if entries != before {
            self.save(parent, &entries)?;
        }
        Ok((result, entries))
    }
    fn save(&self, parent: &Path, entries: &[Entry]) -> Result<()> {
        let temp = parent.join(format!(".operation-queue-{}", uuid::Uuid::new_v4()));
        let document = json!({"version":1,"root":self.root,"entries":entries});
        let result = (|| -> Result<()> {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp)?;
            f.write_all(&serde_json::to_vec(&document)?)?;
            f.sync_all()?;
            fs::rename(&temp, &self.path)?;
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        let _ = fs::remove_file(&temp);
        result
    }
}
/// Popup-side queue edits, applied off the input thread.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Add a queued entry (dispatched by the worker).
    Enqueue(Entry),
    /// Record work the popup started itself, so the cap and FIFO see it.
    Register(Entry),
    /// The backend admitted a registered entry under this ID.
    Admitted(String, String),
    /// Forget an entry: its direct dispatch was answered or the user dismissed it.
    Remove(String),
    /// Cancel a queued entry; running entries are left alone.
    Cancel(String),
}
/// Apply one command. Returns false when a Cancel found nothing still queued.
pub fn apply(entries: &mut Vec<Entry>, command: Command) -> bool {
    match command {
        Command::Enqueue(e) => {
            entries.push(e);
            true
        }
        Command::Register(mut e) => {
            e.state = State::Running;
            e.started_at = Some(now());
            entries.push(e);
            true
        }
        Command::Admitted(id, operation) => {
            if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
                e.operation_id = Some(operation);
            }
            true
        }
        Command::Remove(id) => {
            entries.retain(|e| e.id != id || e.state == State::Queued);
            true
        }
        Command::Cancel(id) => {
            let before = entries.len();
            entries.retain(|e| e.id != id || e.state != State::Queued);
            entries.len() != before
        }
    }
}
/// Launch receipt phases that are still making progress.
const LAUNCH_IN_PROGRESS: [&str; 13] = [
    "admitted",
    "queued",
    "preparing",
    "creating_terminal",
    "dispatching",
    "starting_provider",
    "attached",
    "transferring",
    "checking_history",
    "verifying_history",
    "compressing_history",
    "moving_history",
    "waiting_for_idle",
];
/// An uncertain launch is shown as failed and never replayed.
pub const UNCERTAIN_DETAIL: &str =
    "Outcome uncertain — not retried. Check Operations before trying again.";
const UNKNOWN_DETAIL: &str = "Outcome unknown — not retried. Check Operations before trying again.";
fn fail(e: &mut Entry, detail: String, now: i64) {
    e.state = State::Failed;
    e.finished_at = Some(now);
    e.detail = Some(detail);
}
/// Settle running entries against backend evidence. `launch_receipt` gives the
/// receipt of a launch request ID when it was read this round. A running or
/// finished launch is dropped (the journal keeps its receipt for Operations); an
/// uncertain, failed or unknown one becomes a failed entry, so the work queued
/// behind it proceeds and nothing is replayed.
pub fn settle(
    entries: &mut Vec<Entry>,
    operations: &[Value],
    launch_receipt: &dyn Fn(&str) -> Option<Value>,
    now: i64,
) {
    entries.retain_mut(|e| {
        if e.state != State::Running {
            return true;
        }
        let waited = now - e.started_at.unwrap_or(e.created_at) >= ADMISSION_GRACE_SECS;
        match (&e.action, &e.operation_id) {
            (Action::Launch { .. }, Some(request)) => {
                let receipt = launch_receipt(request).unwrap_or(Value::Null);
                let phase = receipt["phase"].as_str().unwrap_or("");
                match phase {
                    "running" | "complete" | "completed" | "recovered" => false,
                    "failed" | "cancelled" => {
                        let detail = receipt["detail"]
                            .as_str()
                            .filter(|d| !d.trim().is_empty())
                            .map_or_else(
                                || crate::messages::phase(phase),
                                crate::messages::describe,
                            );
                        fail(e, detail, now);
                        true
                    }
                    "uncertain" => {
                        fail(e, UNCERTAIN_DETAIL.into(), now);
                        true
                    }
                    p if LAUNCH_IN_PROGRESS.contains(&p) => true,
                    // Local admission, an unreadable receipt or an unrecognised
                    // phase: allow the admission grace, then stop holding the queue.
                    _ if !waited => true,
                    "pending_local" => {
                        fail(e, UNCERTAIN_DETAIL.into(), now);
                        true
                    }
                    _ => {
                        fail(e, UNKNOWN_DETAIL.into(), now);
                        true
                    }
                }
            }
            (Action::Api { .. }, Some(op)) => {
                match operations.iter().find(|o| o["id"] == op.as_str()) {
                    Some(o) => !terminal(o["phase"].as_str().unwrap_or("")),
                    // Not listed (yet): keep it for the admission grace only.
                    None => !waited,
                }
            }
            (_, None) => {
                if waited {
                    fail(
                        e,
                        "Outcome unconfirmed; it was not retried. Check Operations before trying again."
                            .into(),
                        now,
                    );
                }
                true
            }
        }
    });
}
/// Point a queued session mutation at the session's current generation.
///
/// Restart, move and switch change a session's generation, so an action queued
/// behind them would always be stale. The target is re-resolved by its stable
/// identity: the same session ID, or the entry that now carries the queued
/// conversation's lineage (`previous_conversation_ids` or a merge). When the
/// session's conversation is no longer that lineage, or it is gone, this fails.
pub fn rebase(
    args: &[String],
    conversation: Option<&str>,
    sessions: &[Value],
) -> Result<Vec<String>, String> {
    const GONE: &str = "The session is no longer available; choose the action again.";
    let position = |flag: &str| args.iter().position(|a| a == flag);
    let (Some(at_session), Some(at_generation)) =
        (position("--session"), position("--expected-generation"))
    else {
        return Ok(args.to_vec());
    };
    let session = args.get(at_session + 1).ok_or(GONE)?;
    let lineage = |s: &Value| {
        let Some(c) = conversation else {
            return true;
        };
        s["conversation_id"] == c
            || s["previous_conversation_ids"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|i| i == c))
    };
    let current = match sessions.iter().find(|s| s["id"] == session.as_str()) {
        Some(s) if lineage(s) => s,
        Some(_) => {
            return Err(
                "The session now holds a different conversation; choose the action again.".into(),
            );
        }
        // A merged entry continues under the survivor that records its lineage.
        None => match conversation {
            Some(_) => {
                let mut survivors = sessions.iter().filter(|s| lineage(s));
                match (survivors.next(), survivors.next()) {
                    (Some(s), None) => s,
                    _ => return Err(GONE.into()),
                }
            }
            None => return Err(GONE.into()),
        },
    };
    let (Some(id), Some(generation)) = (current["id"].as_str(), current["generation"].as_str())
    else {
        return Err(GONE.into());
    };
    let mut out = args.to_vec();
    out[at_session + 1] = id.into();
    if let Some(g) = out.get_mut(at_generation + 1) {
        *g = generation.into();
    }
    Ok(out)
}
/// Plain-language failure for a queued dispatch.
pub fn failure_detail(error: &str) -> String {
    if error.starts_with("STALE")
        || error.starts_with("SESSION_STALE")
        || error.contains("GENERATION")
    {
        "The session changed while this waited; choose the action again.".into()
    } else {
        crate::messages::describe(error)
    }
}
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Detached dispatcher. One per queue (non-blocking lock); it exits when nothing
/// is queued or running. Each entry is attempted at most once.
pub mod worker {
    use super::*;
    pub async fn run(
        store: Store,
        client: crate::client::Client,
        journal: crate::launch_intent::Journal,
    ) -> Result<()> {
        use std::os::fd::AsRawFd;
        let lock = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(store.worker_lock_path())?;
        // SAFETY: flock on a valid, owned descriptor.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Ok(()); // another worker owns this queue
        }
        let mut failures = 0u32;
        loop {
            let entries = store.load();
            if !entries.iter().any(|e| e.state != State::Failed) {
                return Ok(());
            }
            let operations = match client.api(&["operations"], None).await {
                Ok(v) => {
                    failures = 0;
                    v.as_array().cloned().unwrap_or_default()
                }
                Err(_) => {
                    failures += 1;
                    if failures > 600 {
                        return Ok(()); // the next popup start resumes dispatching
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(WORKER_POLL_MS)).await;
                    continue;
                }
            };
            let mut receipts = std::collections::BTreeMap::new();
            for e in entries.iter().filter(|e| e.state == State::Running) {
                if let (Action::Launch { kind, .. }, Some(request)) = (&e.action, &e.operation_id)
                    && let Ok(r) = journal.status(&client, kind, request).await
                {
                    receipts.insert(request.clone(), r);
                }
            }
            let (claimed, _) = store.update(|entries| {
                settle(entries, &operations, &|r| receipts.get(r).cloned(), now());
                let picked = ready(entries, &operations);
                for &i in &picked {
                    entries[i].state = State::Running;
                    entries[i].started_at = Some(now());
                }
                picked
                    .into_iter()
                    .map(|i| entries[i].clone())
                    .collect::<Vec<_>>()
            })?;
            for entry in claimed {
                dispatch(&store, &client, &journal, entry).await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(WORKER_POLL_MS)).await;
        }
    }
    async fn dispatch(
        store: &Store,
        client: &crate::client::Client,
        journal: &crate::launch_intent::Journal,
        entry: Entry,
    ) {
        let result: Result<Option<String>> = match &entry.action {
            Action::Api { args } => match current_args(client, args, &entry).await {
                Ok(args) => {
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    client
                        .api(&refs, None)
                        .await
                        .map(|v| v["id"].as_str().map(str::to_owned))
                }
                Err(detail) => Err(anyhow::anyhow!(detail)),
            },
            Action::Launch { request, .. } => {
                let id = request["request_id"].as_str().unwrap_or("").to_owned();
                match journal.accept(request) {
                    Ok(()) => journal.submit(client, &id).await.map(|_| Some(id)),
                    Err(e) => Err(e),
                }
            }
        };
        let _ = store.update(|entries| {
            let Some(i) = entries.iter().position(|e| e.id == entry.id) else {
                return;
            };
            match result {
                Ok(Some(op)) => entries[i].operation_id = Some(op),
                // Acknowledged without an operation: nothing left to watch.
                Ok(None) => {
                    entries.remove(i);
                }
                Err(e) => fail(&mut entries[i], failure_detail(&e.to_string()), now()),
            }
        });
    }
    /// The entry's arguments with the session's current generation (see `rebase`).
    async fn current_args(
        client: &crate::client::Client,
        args: &[String],
        entry: &Entry,
    ) -> Result<Vec<String>, String> {
        if !args.iter().any(|a| a == "--expected-generation") {
            return Ok(args.to_vec());
        }
        let state = client
            .api(&["state", "--archive-filter", "all"], None)
            .await
            .map_err(|_| {
                "The session could not be checked before starting; choose the action again."
                    .to_owned()
            })?;
        let sessions = state["sessions"].as_array().cloned().unwrap_or_default();
        rebase(args, entry.conversation.as_deref(), &sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(resource: &str, state: State) -> Entry {
        let mut e = Entry::new(
            resource.into(),
            "restart",
            "⏣ s".into(),
            Action::Api {
                args: vec!["restart".into()],
            },
        );
        e.state = state;
        e
    }
    #[test]
    fn concurrency_cap_limits_dispatch_across_resources() {
        let entries: Vec<_> = (0..5)
            .map(|i| entry(&format!("session:{i}"), State::Queued))
            .collect();
        assert_eq!(ready(&entries, &[]), vec![0, 1, 2]);
        assert_eq!(MAX_CONCURRENT, 3);
        // A backend operation the queue did not start counts toward the cap.
        let ops = [json!({"id":"x","phase":"dispatching","source_session_id":"other"})];
        assert_eq!(ready(&entries, &ops), vec![0, 1]);
        let mut running = entries.clone();
        running[0].state = State::Running;
        assert_eq!(ready(&running, &ops), vec![1]);
    }
    #[test]
    fn same_resource_runs_fifo_one_at_a_time() {
        let mut entries = vec![
            entry("session:a", State::Queued),
            entry("session:a", State::Queued),
            entry("session:b", State::Queued),
        ];
        assert_eq!(ready(&entries, &[]), vec![0, 2]);
        entries[0].state = State::Running;
        assert_eq!(ready(&entries, &[]), vec![2], "second waits for the first");
        entries.remove(0);
        assert_eq!(ready(&entries, &[]), vec![0, 1]);
        // A backend reservation on the resource holds the queue too.
        let ops = [json!({"id":"o","phase":"preparing","source_session_id":"a"})];
        assert_eq!(ready(&entries, &ops), vec![1]);
        assert!(!can_start(&entries, &ops, "session:a"));
        assert!(
            !can_start(&entries, &[], "session:b"),
            "queued entries keep FIFO"
        );
        assert!(can_start(&entries, &[], "session:c"));
    }
    #[test]
    fn cancelling_only_removes_entries_that_have_not_started() {
        let mut entries = vec![
            entry("session:a", State::Running),
            entry("session:a", State::Queued),
        ];
        let (running, queued) = (entries[0].id.clone(), entries[1].id.clone());
        assert!(!apply(&mut entries, Command::Cancel(running)));
        assert_eq!(entries.len(), 2, "running work is never cancelled");
        assert!(apply(&mut entries, Command::Cancel(queued.clone())));
        assert_eq!(entries.len(), 1);
        assert!(!apply(&mut entries, Command::Cancel(queued)));
    }
    #[test]
    fn settling_never_retries_and_marks_unconfirmed_admission_failed() {
        let mut admitted = entry("session:a", State::Running);
        admitted.operation_id = Some("op".into());
        admitted.started_at = Some(100);
        let mut lost = entry("session:b", State::Running);
        lost.started_at = Some(100);
        let mut entries = vec![admitted, lost];
        let ops = [json!({"id":"op","phase":"dispatching"})];
        settle(&mut entries, &ops, &|_| None, 120);
        assert_eq!(entries.len(), 2);
        settle(&mut entries, &ops, &|_| None, 100 + ADMISSION_GRACE_SECS);
        assert_eq!(entries[1].state, State::Failed);
        assert!(
            ready(&entries, &ops).is_empty(),
            "failed entries are not dispatched"
        );
        let done = [json!({"id":"op","phase":"complete"})];
        settle(&mut entries, &done, &|_| None, 200);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].state, State::Failed);
    }
    fn launch(state: State, started: i64) -> Entry {
        let mut e = Entry::new(
            "account:codex:x".into(),
            "launch",
            "⏣ new".into(),
            Action::Launch {
                kind: "launch".into(),
                request: json!({"request_id":"r"}),
            },
        );
        e.state = state;
        e.started_at = Some(started);
        e.operation_id = Some(uuid::Uuid::new_v4().to_string());
        e
    }
    #[test]
    fn uncertain_failed_and_unknown_launches_settle_failed_and_release_the_queue() {
        let receipt = |phase: &str, detail: &str| json!({"phase":phase,"detail":detail});
        for (phase, detail, after, expect) in [
            (
                "uncertain",
                "Start outcome unconfirmed: broken pipe",
                1,
                Some(UNCERTAIN_DETAIL),
            ),
            ("failed", "ACCOUNT_UNAVAILABLE", 1, None),
            ("cancelled", "", 1, None),
            (
                "pending_local",
                "",
                ADMISSION_GRACE_SECS,
                Some(UNCERTAIN_DETAIL),
            ),
            ("", "", ADMISSION_GRACE_SECS, Some(UNKNOWN_DETAIL)),
            (
                "mystery_phase",
                "",
                ADMISSION_GRACE_SECS,
                Some(UNKNOWN_DETAIL),
            ),
        ] {
            let mut entries = vec![launch(State::Running, 100), {
                let mut q = launch(State::Queued, 100);
                q.operation_id = None;
                q
            }];
            assert!(ready(&entries, &[]).is_empty(), "{phase}: held behind");
            let r = receipt(phase, detail);
            let read = |_: &str| (!phase.is_empty()).then(|| r.clone());
            settle(&mut entries, &[], &read, 100 + after);
            assert_eq!(entries[0].state, State::Failed, "{phase}");
            assert_eq!(entries[0].finished_at, Some(100 + after));
            let shown = entries[0].detail.clone().unwrap();
            match expect {
                Some(text) => assert_eq!(shown, text, "{phase}"),
                None => assert!(
                    !shown.is_empty() && !shown.contains('_'),
                    "{phase}: {shown}"
                ),
            }
            // Never replayed, and what waited behind it proceeds.
            assert_eq!(ready(&entries, &[]), vec![1], "{phase}");
            // With nothing left but failures, the worker's loop has nothing to do.
            entries.remove(1);
            assert!(!entries.iter().any(|e| e.state != State::Failed));
        }
        // Uncertain is shown as such, never as a plain failure.
        assert!(UNCERTAIN_DETAIL.contains("uncertain") && UNCERTAIN_DETAIL.contains("not retried"));
    }
    #[test]
    fn progressing_and_finished_launches_are_not_failed() {
        for phase in ["admitted", "dispatching", "starting_provider", "attached"] {
            let mut entries = vec![launch(State::Running, 100)];
            let r = json!({"phase":phase});
            settle(
                &mut entries,
                &[],
                &|_| Some(r.clone()),
                100 + 10 * ADMISSION_GRACE_SECS,
            );
            assert_eq!(entries[0].state, State::Running, "{phase} keeps running");
        }
        // Local admission inside the grace keeps waiting for the backend.
        let mut entries = vec![launch(State::Running, 100)];
        let local = json!({"phase":"pending_local"});
        settle(&mut entries, &[], &|_| Some(local.clone()), 110);
        assert_eq!(entries[0].state, State::Running);
        for phase in ["running", "complete", "recovered"] {
            let mut entries = vec![launch(State::Running, 100)];
            let r = json!({"phase":phase});
            settle(&mut entries, &[], &|_| Some(r.clone()), 101);
            assert!(entries.is_empty(), "{phase}: the journal keeps the receipt");
        }
    }
    #[test]
    fn queued_session_actions_take_the_current_generation_of_the_same_lineage() {
        let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        let args = s(&["stop", "--session", "a", "--expected-generation", "g1"]);
        // Restarted while queued: same session and conversation, new generation.
        let sessions = [
            json!({"id":"a","generation":"g2","conversation_id":"c1"}),
            json!({"id":"b","generation":"x","conversation_id":"c9"}),
        ];
        assert_eq!(
            rebase(&args, Some("c1"), &sessions).unwrap(),
            s(&["stop", "--session", "a", "--expected-generation", "g2"])
        );
        // Switched with a continued conversation: the lineage still holds.
        let continued = [
            json!({"id":"a","generation":"g3","conversation_id":"c2","previous_conversation_ids":["c1"]}),
        ];
        assert_eq!(rebase(&args, Some("c1"), &continued).unwrap()[4], "g3");
        // Merged into a survivor that records the lineage: follow the survivor.
        let merged = [
            json!({"id":"z","generation":"g4","conversation_id":"c5","previous_conversation_ids":["c1"]}),
        ];
        assert_eq!(
            rebase(&args, Some("c1"), &merged).unwrap(),
            s(&["stop", "--session", "z", "--expected-generation", "g4"])
        );
        // A different conversation under the same entry fails honestly.
        let other = [json!({"id":"a","generation":"g5","conversation_id":"c7"})];
        assert!(
            rebase(&args, Some("c1"), &other)
                .unwrap_err()
                .contains("different conversation")
        );
        // Gone, or two candidates claiming the lineage: never guess.
        assert!(rebase(&args, Some("c1"), &[]).is_err());
        let ambiguous = [
            json!({"id":"y","generation":"1","previous_conversation_ids":["c1"]}),
            json!({"id":"z","generation":"2","previous_conversation_ids":["c1"]}),
        ];
        assert!(rebase(&args, Some("c1"), &ambiguous).is_err());
        // An entry queued before conversations were recorded: same ID only.
        assert_eq!(rebase(&args, None, &sessions).unwrap()[4], "g2");
        assert!(rebase(&args, None, &merged).is_err());
        // Arguments without a generation pass through unchanged.
        let plain = s(&["account", "delete", "codex:x", "--confirm", "codex:x"]);
        assert_eq!(rebase(&plain, None, &[]).unwrap(), plain);
    }
    #[test]
    fn entries_written_by_older_builds_still_load() {
        let old = json!({"id":"e","resource":"session:a","kind":"restart","subject":"⏣ a",
            "action":{"type":"api","args":["restart"]},"state":"failed","created_at":1});
        let e: Entry = serde_json::from_value(old).unwrap();
        assert_eq!((e.finished_at, e.conversation), (None, None));
    }
    #[test]
    fn classifies_backend_operations_and_skips_instant_edits() {
        let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert_eq!(
            classify(&s(&["move-space", "--session", "a", "--space", "w"])),
            Some(("session:a".into(), "move_space"))
        );
        assert_eq!(
            classify(&s(&[
                "account",
                "delete",
                "codex:x",
                "--confirm",
                "codex:x"
            ])),
            Some(("account:codex:x".into(), "account_delete"))
        );
        assert_eq!(classify(&s(&["rename", "--session", "a"])), None);
        assert_eq!(classify(&s(&["account", "limits", "codex:x"])), None);
    }
    #[test]
    fn queue_persists_across_a_reload_and_is_root_scoped() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let store = Store::beside(&t.path().join("preferences.toml"), Path::new("/r"));
        let e = entry("session:a", State::Queued);
        store
            .update(|entries| apply(entries, Command::Enqueue(e.clone())))
            .unwrap();
        // A new popup (or the worker) reads the same queue back.
        let reloaded = Store::beside(&t.path().join("preferences.toml"), Path::new("/r"));
        assert_eq!(reloaded.load(), vec![e]);
        assert_eq!(
            fs::metadata(&store.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let other = Store::beside(&t.path().join("preferences.toml"), Path::new("/other"));
        assert!(other.load().is_empty());
        // The worker lock is exclusive: held by one, seen as running by others.
        assert!(!store.worker_running());
        let held = fs::File::create(store.worker_lock_path()).unwrap();
        use std::os::fd::AsRawFd;
        assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);
        assert!(store.worker_running());
        drop(held);
        assert!(!store.worker_running());
    }
}
