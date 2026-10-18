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
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Workspace {
    pub workspace_id: String,
    pub label: String,
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
impl Client {
    pub async fn launch(&self, operation: &str, mut request: Value) -> Result<Value> {
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
                .args(["--session", &self.session])
                .args(args)
                .env("HERDR_SOCKET_PATH", &self.socket)
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
