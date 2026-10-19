use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::AsyncWriteExt;
#[derive(Clone, Debug)]
pub struct Client {
    pub binary: PathBuf,
    pub root: PathBuf,
    pub herdr: PathBuf,
    pub socket: PathBuf,
    pub session: String,
}
#[derive(Deserialize)]
struct Config {
    binary: PathBuf,
    #[serde(rename = "dataDir")]
    data_dir: PathBuf,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Account {
    pub id: String,
    pub selectable: bool,
    #[serde(default)]
    pub prefill: Value,
    #[serde(default)]
    pub usage: Value,
    #[serde(flatten)]
    pub details: std::collections::BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Workspace {
    pub workspace_id: String,
    pub label: String,
    #[serde(default)]
    pub focused: bool,
}
/// Popup panes have no pane ID; the host supplies the originating tiled context.
pub fn origin(context: Option<&str>) -> Result<(Option<String>, Option<PathBuf>)> {
    let Some(raw) = context else {
        return Ok((None, None));
    };
    let value: Value = serde_json::from_str(raw).context("Invalid HERDR plugin context")?;
    let space = value["workspace_id"].as_str().map(str::to_owned);
    let cwd = value["focused_pane_cwd"]
        .as_str()
        .or_else(|| value["workspace_cwd"].as_str())
        .map(PathBuf::from);
    ensure!(
        cwd.as_ref().is_none_or(|p| p.is_absolute()),
        "Invalid originating directory"
    );
    Ok((space, cwd))
}
pub fn config_path() -> PathBuf {
    std::env::var_os("SESSIONSATION_UI_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if Path::new("/mnt/user/appdata").is_dir() {
                "/mnt/user/appdata/sessionsation-ui/config.json".into()
            } else {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                    .join(".config/sessionsation-ui/config.json")
            }
        })
}
/// Resolve a deployed config pointer before choosing its writable sidecar.
pub fn preferences_path(config: &Path) -> PathBuf {
    std::fs::canonicalize(config)
        .unwrap_or_else(|_| config.into())
        .with_file_name("preferences.json")
}
impl Client {
    pub async fn focus_session(&self, selected: &Value) -> Result<()> {
        let state = self.api(&["state"], None).await?;
        let session = state["sessions"]
            .as_array()
            .and_then(|s| s.iter().find(|s| s["id"] == selected["id"]))
            .context("SESSION_NOT_FOUND")?;
        ensure!(
            session["generation"] == selected["generation"],
            "STALE_TARGET"
        );
        ensure!(
            session["availability"] == "running" && session["attachment"]["host"] == "herdr",
            "SESSION_NOT_IN_HERDR"
        );
        let target = session["attachment"]["target"]
            .as_str()
            .context("MISSING_HOST_TARGET")?;
        let prefix = format!("herdr/{}/", self.session);
        let pane = target
            .strip_prefix(&prefix)
            .context("Session belongs to another HERDR instance")?;
        let process = self.host(&["pane", "process-info", "--pane", pane]).await?;
        ensure!(
            process["process_info"]["foreground_processes"]
                .as_array()
                .is_some_and(|ps| ps.iter().any(|p| p["pid"] == session["attachment"]["pid"])),
            "HERDR_ATTACHMENT_MISMATCH"
        );
        let info = self.host(&["pane", "get", pane]).await?;
        let info = info.get("pane").unwrap_or(&info);
        let tab = info["tab_id"]
            .as_str()
            .context("HERDR returned no tab for the selected pane")?;
        // Recheck after host discovery so a replaced catalog attachment cannot be
        // deliberately followed using its old identity.
        let fresh = self.api(&["state"], None).await?;
        ensure!(
            fresh["sessions"].as_array().is_some_and(|ss| ss
                .iter()
                .any(|s| s["id"] == selected["id"] && s["generation"] == selected["generation"])),
            "STALE_TARGET"
        );
        self.host(&["tab", "focus", tab]).await?;
        Ok(())
    }
    pub async fn launch(&self, operation: &str, mut request: Value) -> Result<Value> {
        if request["session_id"].is_string() {
            let snapshot = request.as_object_mut().unwrap().remove("source_snapshot");
            if let Some(snapshot) = snapshot {
                let state = self.api(&["state"], None).await?;
                let source = state["sessions"]
                    .as_array()
                    .and_then(|ss| ss.iter().find(|s| s["id"] == request["session_id"]))
                    .context("SESSION_NOT_FOUND")?;
                if source["generation"] != request["expected_generation"] {
                    let fresh = self
                        .api(
                            &[
                                "session",
                                "--session",
                                source["id"].as_str().context("INVALID_SESSION_ID")?,
                                "--expected-generation",
                                source["generation"]
                                    .as_str()
                                    .context("INVALID_GENERATION")?,
                            ],
                            None,
                        )
                        .await?;
                    refresh_resume_request(&mut request, &snapshot, &fresh)?;
                }
            }
            return self.api(&["resume-launch", operation], Some(request)).await;
        }
        let paths: Vec<PathBuf> = serde_json::from_value(request["directory"]["create"].clone())?;
        let remaining =
            tokio::task::spawn_blocking(move || crate::directory::remaining_plan(&paths)).await??;
        request["directory"]["create"] = serde_json::to_value(remaining)?;
        self.api(&["launch", operation], Some(request)).await
    }
    pub fn load() -> Result<Self> {
        let stored = std::fs::read(config_path())
            .ok()
            .map(|b| serde_json::from_slice::<Config>(&b))
            .transpose()?;
        let binary = std::env::var_os("SESSIONSATION_BIN")
            .map(PathBuf::from)
            .or_else(|| stored.as_ref().map(|c| c.binary.clone()))
            .context("Set SESSIONSATION_BIN")?;
        let root = std::env::var_os("SESSIONSATION_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| stored.as_ref().map(|c| c.data_dir.clone()))
            .context("Set SESSIONSATION_DATA_DIR")?;
        ensure!(
            binary.is_absolute() && root.is_absolute(),
            "Backend binary and root must be absolute"
        );
        let socket = PathBuf::from(
            std::env::var_os("HERDR_SOCKET_PATH").context("Open this launcher from HERDR")?,
        );
        let session = std::env::var("SESSIONSATION_HERDR_SESSION")
            .or_else(|_| std::env::var("HERDR_SESSION"))
            .or_else(|_| {
                let base = std::env::var_os("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
                    });
                if std::fs::canonicalize(&socket)
                    .ok()
                    .zip(std::fs::canonicalize(base.join("herdr/herdr.sock")).ok())
                    .is_some_and(|(a, b)| a == b)
                {
                    Ok("default".into())
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            })
            .context("HERDR instance could not be identified")?;
        let herdr = std::env::var_os("HERDR_BIN_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| "herdr".into());
        Ok(Self {
            binary,
            root,
            herdr,
            socket,
            session,
        })
    }
    pub async fn api(&self, args: &[&str], input: Option<Value>) -> Result<Value> {
        let mut command = tokio::process::Command::new(&self.binary);
        command
            .arg("--data-dir")
            .arg(&self.root)
            .args(["api", "--api-version", "1"])
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        if let Some(input) = input {
            let mut stdin = child.stdin.take().context("Missing backend stdin")?;
            stdin.write_all(&serde_json::to_vec(&input)?).await?;
            drop(stdin);
        }
        let out = tokio::time::timeout(Duration::from_secs(20), child.wait_with_output())
            .await
            .context("BACKEND_TIMEOUT: reconcile Start using its request ID")??;
        let body: Value = serde_json::from_slice(if out.status.success() {
            &out.stdout
        } else {
            &out.stderr
        })
        .context("Invalid backend envelope")?;
        ensure!(body["api_version"] == 1, "Public API v1 required");
        ensure!(
            out.status.success() && body["ok"] == true,
            "{}: {}",
            body["error"]["code"].as_str().unwrap_or("BACKEND_ERROR"),
            body["error"]["message"]
                .as_str()
                .unwrap_or("Request failed")
        );
        Ok(body["result"].clone())
    }
    pub async fn host(&self, args: &[&str]) -> Result<Value> {
        let out = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::process::Command::new(&self.herdr)
                .args(args)
                .env("HERDR_SOCKET_PATH", &self.socket)
                .env("HERDR_SESSION", &self.session)
                .stdin(Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .context("HERDR timed out")??;
        ensure!(
            out.status.success(),
            "HERDR: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let body: Value = serde_json::from_slice(&out.stdout)?;
        ensure!(body.get("error").is_none(), "HERDR rejected request");
        Ok(body.get("result").unwrap_or(&body).clone())
    }
    pub fn instance(&self) -> String {
        std::fs::canonicalize(&self.socket)
            .unwrap_or(self.socket.clone())
            .to_string_lossy()
            .into_owned()
    }
    pub fn host_value(&self) -> Value {
        json!({"session":self.session,"socket":self.socket})
    }
}

/// Refresh only the same inactive source. Destination choices are never overwritten,
/// and a different account/history/location requires a new explicit selection.
fn refresh_resume_request(request: &mut Value, snapshot: &Value, fresh: &Value) -> Result<()> {
    ensure!(
        fresh["id"] == request["session_id"] && fresh["availability"] == "saved",
        "SESSION_ALREADY_RUNNING_OR_UNAVAILABLE"
    );
    for key in ["conversation_id", "account_id", "directory", "name"] {
        ensure!(
            snapshot[key].is_string() && snapshot[key] == fresh[key],
            "SESSION_SOURCE_CHANGED: select the session again"
        );
    }
    request["expected_generation"] = fresh["generation"].clone();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn explicit_host_socket_is_not_overridden_by_a_session_selector() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let herdr = tmp.path().join("herdr");
        std::fs::write(&herdr,"#!/bin/sh\n[ \"$1\" = workspace ] || exit 1\n[ \"$HERDR_SOCKET_PATH\" = /chosen.sock ] || exit 2\n[ \"$HERDR_SESSION\" = named ] || exit 3\nprintf '%s\\n' '{\"result\":{\"selected\":true}}'\n").unwrap();
        std::fs::set_permissions(&herdr, std::fs::Permissions::from_mode(0o755)).unwrap();
        let client = Client {
            binary: "unused".into(),
            root: tmp.path().into(),
            herdr,
            socket: "/chosen.sock".into(),
            session: "named".into(),
        };
        assert_eq!(
            client.host(&["workspace", "list"]).await.unwrap()["selected"],
            true
        );
    }
    #[test]
    fn resume_refresh_preserves_destination_and_rejects_changed_source() {
        let snapshot = json!({"id":"stable","conversation_id":"thread","account_id":"codex:original","directory":"/saved","name":"Original"});
        let fresh = json!({"id":"stable","conversation_id":"thread","account_id":"codex:original","directory":"/saved","name":"Original","availability":"saved","generation":"new"});
        let mut request = json!({"session_id":"stable","expected_generation":"old","account_id":"codex:target","name":"Chosen","space":{"kind":"new","label":"Other"}});
        refresh_resume_request(&mut request, &snapshot, &fresh).unwrap();
        assert_eq!(request["expected_generation"], "new");
        assert_eq!(request["space"]["label"], "Other");
        assert_eq!(request["account_id"], "codex:target");
        assert_eq!(request["name"], "Chosen");
        for key in [
            "id",
            "conversation_id",
            "account_id",
            "directory",
            "availability",
            "name",
        ] {
            let mut changed = fresh.clone();
            changed[key] = json!("changed");
            assert!(
                refresh_resume_request(&mut request, &snapshot, &changed).is_err(),
                "{key}"
            );
        }
    }
    #[test]
    fn deployment_pointer_keeps_preferences_beside_real_config() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let runtime = root.path().join("runtime");
        std::fs::create_dir(&data).unwrap();
        std::fs::create_dir(&runtime).unwrap();
        let config = data.join("popup.json");
        std::fs::write(&config, "{}").unwrap();
        let link = runtime.join("config.json");
        std::os::unix::fs::symlink(&config, &link).unwrap();
        let preferences = preferences_path(&link);
        assert_eq!(preferences, data.join("preferences.json"));
        crate::preferences::Preferences::default()
            .save(&preferences)
            .unwrap();
        assert!(!runtime.join("preferences.json").exists());
    }
}
