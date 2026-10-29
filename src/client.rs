use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
struct ApiRejected {
    code: String,
    message: String,
}
impl std::fmt::Display for ApiRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ApiRejected {}

// Tokio's child I/O is asynchronous, but Command::spawn still waits for exec
// synchronously. A cold executable or slow filesystem must not freeze the
// single-threaded input/render runtime during each inventory poll.
async fn spawn_child(mut command: tokio::process::Command) -> Result<tokio::process::Child> {
    Ok(tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let child = command.spawn();
        crate::trace::event(
            "spawn",
            format_args!(
                "{:?} us={}",
                command.as_std().get_args().last().unwrap_or_default(),
                started.elapsed().as_micros()
            ),
        );
        child
    })
    .await??)
}
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
impl Account {
    /// Provider from the published field, else the qualified-ID prefix. IDs are
    /// always `<provider>:<name>`; an unqualified legacy ID is Codex.
    pub fn provider(&self) -> &str {
        self.details
            .get("provider")
            .and_then(Value::as_str)
            .or_else(|| self.id.split_once(':').map(|(p, _)| p))
            .unwrap_or("codex")
    }
}
/// Human label for a provider ID; unknown providers are shown verbatim.
pub fn provider_label(provider: &str) -> &str {
    match provider {
        "codex" => "Codex",
        "claude" => "Claude Code",
        "gemini" => "Gemini CLI",
        "antigravity" => "Antigravity CLI",
        other => other,
    }
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
    pub async fn start_checked(&self, request: Value) -> Result<Value> {
        if let Err(error) = self.launch("validate", request.clone()).await {
            return Ok(json!({"phase":"failed", "detail":error.to_string()}));
        }
        match self.launch("start", request.clone()).await {
            Ok(receipt) => Ok(receipt),
            Err(error) => {
                // A timeout, broken pipe, or malformed reply is not rejection.
                // Only a structured error from an exited CLI allows this check.
                if error
                    .downcast_ref::<ApiRejected>()
                    .is_some_and(|e| e.code != "REQUEST_ID_CONFLICT")
                {
                    let kind = if request["session_id"].is_string() {
                        "resume-launch"
                    } else {
                        "launch"
                    };
                    let id = request["request_id"]
                        .as_str()
                        .context("INVALID_REQUEST_ID")?;
                    match self.api(&[kind, "status", "--request-id", id], None).await {
                        Ok(receipt) => return Ok(receipt),
                        Err(status)
                            if status.downcast_ref::<ApiRejected>().is_some_and(|e| {
                                e.code
                                    == if kind == "launch" {
                                        "LAUNCH_NOT_FOUND"
                                    } else {
                                        "RESUME_NOT_FOUND"
                                    }
                            }) =>
                        {
                            // Both start APIs persist admission before creating a
                            // worker or terminal. The original CLI has exited and
                            // no receipt exists: this request did not launch.
                            return Ok(json!({"phase":"failed", "detail":error.to_string()}));
                        }
                        Err(_) => {}
                    }
                }
                Err(error)
            }
        }
    }
    async fn focus_snapshot(&self, selected: &Value, direct: bool) -> Result<Value> {
        if direct {
            return self
                .api(
                    &[
                        "attachment",
                        "--session",
                        selected["id"].as_str().context("INVALID_SESSION_ID")?,
                        "--expected-generation",
                        selected["generation"]
                            .as_str()
                            .context("INVALID_GENERATION")?,
                    ],
                    None,
                )
                .await;
        }
        let state = self.api(&["state"], None).await?;
        state["sessions"]
            .as_array()
            .and_then(|s| s.iter().find(|s| s["id"] == selected["id"]))
            .cloned()
            .context("SESSION_NOT_FOUND")
    }
    pub async fn focus_session(&self, selected: &Value, direct: bool) -> Result<()> {
        let session = self.focus_snapshot(selected, direct).await?;
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
        let fresh = self.focus_snapshot(selected, direct).await?;
        ensure!(
            fresh["id"] == selected["id"] && fresh["generation"] == selected["generation"],
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
            .kill_on_drop(!matches!(args, ["launch" | "resume-launch", "start"]));
        if matches!(args, ["launch" | "resume-launch", "start"]) {
            // A confirmed click owns this admission attempt independently of its viewer.
            // New process group protects it from terminal-group shutdown; only the
            // backend may decide lifecycle, and reads never resend this request.
            command.process_group(0);
        }
        let mut child = spawn_child(command).await?;
        if let Some(input) = input {
            let mut stdin = child.stdin.take().context("Missing backend stdin")?;
            stdin.write_all(&serde_json::to_vec(&input)?).await?;
            drop(stdin);
        }
        let out = tokio::time::timeout(Duration::from_secs(20), child.wait_with_output())
            .await
            .context("BACKEND_TIMEOUT: reconcile Start using its request ID")??;
        crate::trace::event(
            "api_done",
            format_args!("{args:?} bytes={}", out.stdout.len()),
        );
        let body: Value = serde_json::from_slice(if out.status.success() {
            &out.stdout
        } else {
            &out.stderr
        })
        .context("Invalid backend envelope")?;
        ensure!(body["api_version"] == 1, "Public API v1 required");
        if !out.status.success() || body["ok"] != true {
            let code = body["error"]["code"]
                .as_str()
                .context("Invalid backend error envelope")?;
            return Err(ApiRejected {
                code: code.into(),
                message: body["error"]["message"]
                    .as_str()
                    .unwrap_or("Request failed")
                    .into(),
            }
            .into());
        }
        Ok(body["result"].clone())
    }
    pub async fn host(&self, args: &[&str]) -> Result<Value> {
        let mut command = tokio::process::Command::new(&self.herdr);
        command
            .args(args)
            .env("HERDR_SOCKET_PATH", &self.socket)
            .env("HERDR_SESSION", &self.session)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let out = tokio::time::timeout(Duration::from_secs(10), async {
            Ok::<_, anyhow::Error>(spawn_child(command).await?.wait_with_output().await?)
        })
        .await
        .context("HERDR timed out")??;
        crate::trace::event(
            "host_done",
            format_args!("{args:?} bytes={}", out.stdout.len()),
        );
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
        fresh["id"] == request["session_id"]
            && matches!(fresh["availability"].as_str(), Some("saved" | "stopped")),
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
    async fn start_reconciles_rejection_without_replaying_or_clearing_uncertain_delivery() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let cli = tmp.path().join("backend");
        std::fs::write(&cli, r#"#!/usr/bin/env python3
import json, pathlib, sys
root=pathlib.Path(__file__).parent
mode=(root/'mode').read_text()
kind='resume-launch' if 'resume-launch' in sys.argv else 'launch'
action=sys.argv[sys.argv.index(kind)+1]
with (root/'calls').open('a') as f:f.write(action+'\n')
def ok(result): print(json.dumps({'api_version':1,'ok':True,'result':result}))
def fail(code):
 print(json.dumps({'api_version':1,'ok':False,'error':{'code':code,'message':'fixture rejection'}}),file=sys.stderr);sys.exit(1)
if action=='validate':ok({'valid':True})
elif action=='start':
 if mode=='broken':print('broken output',file=sys.stderr);sys.exit(1)
 fail('STALE_TARGET')
elif action=='status':
 if mode=='admitted':ok({'phase':'admitted'})
 elif mode=='unreadable':fail('OPERATION_FAILED')
 else:fail('RESUME_NOT_FOUND' if kind=='resume-launch' else 'LAUNCH_NOT_FOUND')
"#).unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        let client = Client {
            binary: cli,
            root: tmp.path().into(),
            herdr: "unused".into(),
            socket: "unused".into(),
            session: "fixture".into(),
        };
        for resume in [false, true] {
            for mode in ["missing", "admitted", "broken", "unreadable"] {
                std::fs::write(tmp.path().join("mode"), mode).unwrap();
                std::fs::write(tmp.path().join("calls"), "").unwrap();
                let mut request = json!({"request_id":"fixture", "directory":{"create":[]}});
                if resume {
                    request["session_id"] = json!("session");
                }
                let result = client.start_checked(request).await;
                match mode {
                    "missing" => {
                        let receipt = result.unwrap();
                        assert_eq!(receipt["phase"], "failed");
                        assert!(receipt["detail"].as_str().unwrap().contains("STALE_TARGET"));
                    }
                    "admitted" => assert_eq!(result.unwrap()["phase"], "admitted"),
                    _ => assert!(result.is_err(), "{mode}: uncertainty must stay reserved"),
                }
                let calls = std::fs::read_to_string(tmp.path().join("calls")).unwrap();
                assert_eq!(calls.lines().filter(|s| *s == "start").count(), 1);
                assert_eq!(
                    calls.lines().filter(|s| *s == "status").count(),
                    usize::from(mode != "broken")
                );
            }
        }
    }
    #[tokio::test(flavor = "current_thread")]
    async fn slow_exec_does_not_block_input_or_frame_timers() {
        fn slow_command() -> tokio::process::Command {
            let mut command = tokio::process::Command::new("/bin/true");
            command.kill_on_drop(true);
            // Isolated child only: model a slow exec handshake without depending
            // on a particular filesystem, cache, provider, or real terminal.
            unsafe {
                command.pre_exec(|| {
                    let delay = libc::timespec {
                        tv_sec: 0,
                        tv_nsec: 300_000_000,
                    };
                    libc::nanosleep(&delay, std::ptr::null_mut());
                    Ok(())
                });
            }
            command
        }
        let before = std::time::Instant::now();
        let mut baseline = slow_command().spawn().unwrap();
        let blocked = before.elapsed();
        baseline.wait().await.unwrap();
        let start = std::time::Instant::now();
        let (child, timer) = tokio::join!(spawn_child(slow_command()), async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            start.elapsed()
        });
        child.unwrap().wait().await.unwrap();
        eprintln!(
            "slow exec: old event-loop block {blocked:?}; new 20ms timer completed at {timer:?}"
        );
        assert!(blocked >= Duration::from_millis(250));
        assert!(
            timer < Duration::from_millis(200),
            "event loop stalled: {timer:?}"
        );
    }
    #[tokio::test]
    async fn focus_uses_direct_epoch_checks_and_refuses_replaced_attachments() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let selected = json!({"id":"stable","generation":"epoch","availability":"running","attachment":{"host":"herdr","target":"herdr/named/pane","pid":42}});
        std::fs::write(
            tmp.path().join("session.json"),
            serde_json::to_vec(&selected).unwrap(),
        )
        .unwrap();
        let cli = tmp.path().join("backend");
        std::fs::write(&cli, r#"#!/usr/bin/env python3
import json, pathlib, sys, time
r=pathlib.Path(__file__).parent
command='attachment' if 'attachment' in sys.argv else 'state'
with (r/'calls').open('a') as f:f.write(command+'\n')
if command=='state':time.sleep(.5)
session=json.loads((r/'session.json').read_text())
if (r/'replaced').exists():
 print(json.dumps({'api_version':1,'ok':False,'error':{'code':'STALE_TARGET','message':'Refresh the selected session'}}),file=sys.stderr);sys.exit(1)
print(json.dumps({'api_version':1,'ok':True,'result':session if command=='attachment' else {'sessions':[session]}}))
"#).unwrap();
        let host = tmp.path().join("herdr");
        std::fs::write(
            &host,
            r#"#!/usr/bin/env python3
import json,pathlib,sys
r=pathlib.Path(__file__).parent
args=sys.argv[1:]
if args[:2]==['pane','process-info']:result={'process_info':{'foreground_processes':[{'pid':42}]}}
elif args[:2]==['pane','get']:
 result={'pane':{'tab_id':'tab'}}
 if (r/'replace-on-get').exists():(r/'replaced').touch()
elif args[:2]==['tab','focus']:
 with (r/'calls').open('a') as f:f.write('focus\n')
 result={}
else:sys.exit(2)
print(json.dumps({'result':result}))
"#,
        )
        .unwrap();
        for file in [&cli, &host] {
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let client = Client {
            binary: cli,
            root: tmp.path().into(),
            herdr: host,
            socket: "/chosen.sock".into(),
            session: "named".into(),
        };
        let mut timings = vec![];
        for direct in [false, true] {
            std::fs::write(tmp.path().join("calls"), "").unwrap();
            let start = std::time::Instant::now();
            client.focus_session(&selected, direct).await.unwrap();
            timings.push(start.elapsed().as_millis());
            let calls = std::fs::read_to_string(tmp.path().join("calls")).unwrap();
            assert_eq!(
                calls,
                if direct {
                    "attachment\nattachment\nfocus\n"
                } else {
                    "state\nstate\nfocus\n"
                }
            );
        }
        println!(
            "Synthetic Focus ms: legacy={} direct={}",
            timings[0], timings[1]
        );
        std::fs::write(tmp.path().join("calls"), "").unwrap();
        std::fs::write(tmp.path().join("replace-on-get"), "").unwrap();
        assert!(
            client
                .focus_session(&selected, true)
                .await
                .unwrap_err()
                .to_string()
                .contains("STALE_TARGET")
        );
        assert!(
            !std::fs::read_to_string(tmp.path().join("calls"))
                .unwrap()
                .contains("focus")
        );
    }
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
    #[test]
    fn provider_comes_from_field_then_id_and_resume_accepts_stopped_claude() {
        let a: Account =
            serde_json::from_value(json!({"id":"claude:x","selectable":true})).unwrap();
        assert_eq!(a.provider(), "claude");
        let a: Account =
            serde_json::from_value(json!({"id":"x","selectable":true,"provider":"claude"}))
                .unwrap();
        assert_eq!(a.provider(), "claude");
        let a: Account = serde_json::from_value(json!({"id":"x","selectable":true})).unwrap();
        assert_eq!(a.provider(), "codex");
        assert_eq!(provider_label("claude"), "Claude Code");
        assert_eq!(provider_label("other"), "other");
        let snapshot =
            json!({"conversation_id":"n","account_id":"claude:x","directory":"/d","name":"N"});
        for (availability, ok) in [
            ("stopped", true),
            ("saved", true),
            ("running", false),
            ("stale", false),
        ] {
            let mut fresh = snapshot.clone();
            fresh["id"] = json!("s");
            fresh["availability"] = json!(availability);
            fresh["generation"] = json!("new");
            let mut request = json!({"session_id":"s","expected_generation":"old"});
            assert_eq!(
                refresh_resume_request(&mut request, &snapshot, &fresh).is_ok(),
                ok,
                "{availability}"
            );
        }
    }
}
