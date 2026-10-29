//! Last successful session list, kept next to the preferences so Manage Sessions can
//! paint at once. It holds no credentials and is never trusted for actions: the
//! manager strips capabilities and generations from it and replaces it with fresh
//! backend state as soon as that arrives.
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path};

pub fn save(path: &Path, root: &Path, sessions: &Value) -> Result<()> {
    write(path, json!({"version":1,"root":root,"sessions":sessions}))
}
/// Last successful account list (with usage observations) for the first Home frame.
pub fn save_accounts(path: &Path, root: &Path, accounts: &Value) -> Result<()> {
    write(path, json!({"version":1,"root":root,"accounts":accounts}))
}
/// Finished operations whose badges the user dismissed.
pub fn save_dismissed(path: &Path, root: &Path, operations: &[String]) -> Result<()> {
    write(
        path,
        json!({"version":1,"root":root,"operations":operations}),
    )
}
fn write(path: &Path, document: Value) -> Result<()> {
    let parent = path.parent().context("State cache path has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".state-cache-{}", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        f.write_all(&serde_json::to_vec(&document)?)?;
        f.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    let _ = fs::remove_file(temp);
    result
}
/// The cached sessions, only when written for this backend root.
pub fn load(path: &Path, root: &Path) -> Option<Value> {
    let v: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (v["version"] == 1 && v["root"] == json!(root) && v["sessions"].is_array())
        .then(|| v["sessions"].clone())
}
/// The cached accounts, only when written for this backend root.
pub fn load_accounts(path: &Path, root: &Path) -> Option<Value> {
    let v: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (v["version"] == 1 && v["root"] == json!(root) && v["accounts"].is_array())
        .then(|| v["accounts"].clone())
}
/// The dismissed operation IDs, only when written for this backend root.
pub fn load_dismissed(path: &Path, root: &Path) -> Option<Vec<String>> {
    let v: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    if v["version"] != 1 || v["root"] != json!(root) {
        return None;
    }
    serde_json::from_value(v["operations"].clone()).ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trips_atomically_with_private_mode_and_rejects_other_roots() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("state-cache.json");
        let sessions = json!([{"id":"a","name":"A"}]);
        save(&p, Path::new("/r"), &sessions).unwrap();
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(load(&p, Path::new("/r")), Some(sessions));
        assert_eq!(load(&p, Path::new("/other")), None);
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 1);
        fs::write(&p, b"garbage").unwrap();
        assert_eq!(load(&p, Path::new("/r")), None);
        let accounts = json!([{"id":"codex:a","selectable":true}]);
        save_accounts(&p, Path::new("/r"), &accounts).unwrap();
        assert_eq!(load_accounts(&p, Path::new("/r")), Some(accounts));
        assert_eq!(load_accounts(&p, Path::new("/other")), None);
        assert_eq!(load(&p, Path::new("/r")), None);
    }
}
