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
/// Settle running entries against backend evidence. `launch_phase` gives the
/// receipt phase of a launch request ID when it was read this round.
pub fn settle(
    entries: &mut Vec<Entry>,
    operations: &[Value],
    launch_phase: &dyn Fn(&str) -> Option<String>,
    now: i64,
) {
    entries.retain_mut(|e| {
        if e.state != State::Running {
            return true;
        }
        let started = e.started_at.unwrap_or(e.created_at);
        match (&e.action, &e.operation_id) {
            (Action::Launch { .. }, Some(request)) => match launch_phase(request).as_deref() {
                // The journal keeps the receipt; Operations lists it from there.
                Some("running") => false,
                Some(phase) if terminal(phase) => false,
                _ => true,
            },
            (Action::Api { .. }, Some(op)) => {
                match operations.iter().find(|o| o["id"] == op.as_str()) {
                    Some(o) => !terminal(o["phase"].as_str().unwrap_or("")),
                    // Not listed (yet): keep it for the admission grace only.
                    None => now - started < ADMISSION_GRACE_SECS,
                }
            }
            (_, None) => {
                if now - started >= ADMISSION_GRACE_SECS {
                    e.state = State::Failed;
                    e.detail = Some(
                        "Outcome unconfirmed; it was not retried. Check Operations before trying again."
                            .into(),
                    );
                }
                true
            }
        }
    });
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
            let mut phases = std::collections::BTreeMap::new();
            for e in entries.iter().filter(|e| e.state == State::Running) {
                if let (Action::Launch { kind, .. }, Some(request)) = (&e.action, &e.operation_id)
                    && let Ok(r) = journal.status(&client, kind, request).await
                {
                    phases.insert(
                        request.clone(),
                        r["phase"].as_str().unwrap_or("").to_owned(),
                    );
                }
            }
            let (claimed, _) = store.update(|entries| {
                settle(entries, &operations, &|r| phases.get(r).cloned(), now());
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
            Action::Api { args } => {
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                client
                    .api(&refs, None)
                    .await
                    .map(|v| v["id"].as_str().map(str::to_owned))
            }
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
                Err(e) => {
                    entries[i].state = State::Failed;
                    entries[i].detail = Some(failure_detail(&e.to_string()));
                }
            }
        });
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
