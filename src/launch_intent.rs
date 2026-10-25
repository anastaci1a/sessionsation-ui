//! UI submission evidence only. Backend receipts own lifecycle; reads never resend.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf};

#[derive(Clone)]
pub struct Journal {
    pub directory: PathBuf,
    pub root: PathBuf,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    root: PathBuf,
    request: Value,
    receipt: Value,
    reconciled: bool,
}
impl Journal {
    pub fn spawn_worker(
        &self,
        executable: &std::path::Path,
        client: &crate::client::Client,
        id: &str,
    ) -> Result<std::process::Child> {
        use std::os::unix::process::CommandExt;
        self.read(id)?;
        let mut command = std::process::Command::new(executable);
        command
            .args(["submit-worker", "--request-id", id])
            .arg("--journal")
            .arg(&self.directory)
            .arg("--root")
            .arg(&self.root)
            .arg("--binary")
            .arg(&client.binary)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(command.spawn()?)
    }

    fn lock(&self, id: &str) -> Result<fs::File> {
        use std::os::fd::AsRawFd;
        let f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.file(id)?.with_extension("lock"))?;
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(f)
    }

    pub async fn submit(&self, client: &crate::client::Client, id: &str) -> Result<()> {
        let entry = self.read(id)?;
        // An immutable attempt marker is claimed before the first possible API
        // call. Even a crashed worker cannot be automatically dispatched again.
        let claim = self.file(id)?.with_extension("attempt");
        let f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(claim)
            .context("Launch submission already attempted; reconcile only")?;
        f.sync_all()?;
        fs::File::open(&self.directory)?.sync_all()?;
        let result = client.start_checked(entry.request).await;
        self.finish(id, &result)
    }

    fn file(&self, id: &str) -> Result<PathBuf> {
        uuid::Uuid::parse_str(id).context("Invalid launch request ID")?;
        Ok(self.directory.join(format!("{id}.json")))
    }
    pub fn accept(&self, request: &Value) -> Result<()> {
        let id = request["request_id"]
            .as_str()
            .context("Missing launch ID")?;
        fs::create_dir_all(&self.directory)?;
        let receipt = json!({"request":request,"phase":"pending_local","detail":"Start selected; backend admission not yet confirmed. Closing this view does not cancel it.","created_at":now(),"updated_at":now(),"local_submission":true});
        let entry = Entry {
            root: self.root.clone(),
            request: request.clone(),
            receipt,
            reconciled: false,
        };
        let _lock = self.lock(id)?;
        ensure!(
            !self.file(id)?.exists(),
            "Launch ID already recorded; inspect Operations, do not resend"
        );
        self.save(id, &entry)?;
        Ok(())
    }
    fn read(&self, id: &str) -> Result<Entry> {
        let e: Entry = serde_json::from_slice(&fs::read(self.file(id)?)?)?;
        ensure!(
            e.root == self.root,
            "Launch evidence belongs to another backend root"
        );
        Ok(e)
    }
    fn save(&self, id: &str, e: &Entry) -> Result<()> {
        let target = self.file(id)?;
        let temp = target.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        f.write_all(&serde_json::to_vec(e)?)?;
        f.sync_all()?;
        fs::rename(temp, target)?;
        fs::File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
    pub fn finish(&self, id: &str, result: &Result<Value>) -> Result<()> {
        let _lock = self.lock(id)?;
        let mut e = self.read(id)?;
        let created = e.receipt["created_at"].clone();
        e.receipt = match result {
            Ok(r) => r.clone(),
            Err(err) => {
                json!({"phase":"uncertain","detail":format!("Start outcome unconfirmed: {err}. Inspect this original request; it has not been cancelled or resent.")})
            }
        };
        if e.receipt["request"].is_null() {
            e.receipt["request"] = e.request.clone();
        }
        e.receipt["local_submission"] = json!(true);
        e.receipt["updated_at"] = json!(now());
        if e.receipt["created_at"].is_null() {
            e.receipt["created_at"] = created;
        }
        self.save(id, &e)
    }
    pub async fn list(&self, client: &crate::client::Client, kind: &str) -> Result<Value> {
        let remote = client.api(&[kind, "list"], None).await;
        let mut rows = remote
            .as_ref()
            .ok()
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !self.directory.exists() {
            return remote;
        }
        for file in fs::read_dir(&self.directory)? {
            let path = file?.path();
            if path.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .context("Invalid journal entry")?;
            let mut e: Entry = serde_json::from_slice(&fs::read(&path)?)?;
            if e.root != self.root
                || (e.request["session_id"].is_string()) != (kind == "resume-launch")
            {
                continue;
            }
            if rows.iter().any(|r| r["request"]["request_id"] == id) {
                continue;
            }
            if !e.reconciled {
                // Only a read, once per durable intent. Absence is not cancellation.
                let result = client
                    .api(&[kind, "status", "--request-id", id], None)
                    .await;
                let _lock = self.lock(id)?;
                e = self.read(id)?;
                if let Ok(r) = result {
                    e.receipt = r;
                } else if e.receipt["phase"] == "pending_local" {
                    e.receipt["phase"] = json!("uncertain");
                    e.receipt["detail"] = json!(
                        "Backend admission is unconfirmed. This request was not cancelled or resent; inspect its original ID."
                    );
                }
                e.reconciled = true;
                self.save(id, &e)?;
            }
            rows.push(e.receipt);
        }
        if rows.is_empty() {
            return remote;
        }
        Ok(json!(rows))
    }
    pub async fn status(
        &self,
        client: &crate::client::Client,
        kind: &str,
        id: &str,
    ) -> Result<Value> {
        match client
            .api(&[kind, "status", "--request-id", id], None)
            .await
        {
            Ok(r) => Ok(r),
            Err(err) => match self.read(id) {
                Ok(e) => Ok(e.receipt),
                Err(_) => Err(err),
            },
        }
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
