//! UI-only preferences; no account credentials or session lifecycle state.
use crate::draft::{Space, absolute_clean};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Global,
    PerSpace,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub version: u32,
    pub mode: Mode,
    pub global_root: Option<PathBuf>,
    pub space_roots: BTreeMap<String, PathBuf>,
    pub last_account: Option<String>,
    #[serde(default)]
    pub root_plans: BTreeMap<String, Vec<PathBuf>>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            mode: Mode::Global,
            global_root: None,
            space_roots: BTreeMap::new(),
            last_account: None,
            root_plans: BTreeMap::new(),
        }
    }
}
impl Preferences {
    pub fn key(instance: &str, workspace: &str) -> String {
        format!("{instance}\n{workspace}")
    }
    pub fn root(&self, instance: &str, space: Option<&Space>) -> Option<&Path> {
        match self.mode {
            Mode::Global => self.global_root.as_deref(),
            Mode::PerSpace => match space {
                Some(Space::Existing { id, .. }) => self
                    .space_roots
                    .get(&Self::key(instance, id))
                    .map(PathBuf::as_path),
                _ => None,
            },
        }
    }
    pub fn load(path: &Path) -> Result<Self> {
        let p: Self = match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("Invalid launcher preferences")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.into()),
        };
        p.validate()?;
        Ok(p)
    }
    fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported preferences version");
        for p in self.global_root.iter().chain(self.space_roots.values()) {
            ensure!(
                absolute_clean(p),
                "Project roots must be absolute paths without '..'"
            );
        }
        for (key, paths) in &self.root_plans {
            let root = if key == "global" {
                self.global_root.as_ref()
            } else {
                self.space_roots.get(key)
            };
            ensure!(
                paths.is_empty() || paths.last() == root,
                "Staged root plan must end at its configured root"
            );
            ensure!(
                paths.iter().all(|p| absolute_clean(p))
                    && paths
                        .windows(2)
                        .all(|p| p[1].parent() == Some(p[0].as_path())),
                "Invalid staged root plan"
            );
        }
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path.parent().context("Preferences path has no parent")?;
        fs::create_dir_all(parent)?;
        if let Ok(meta) = fs::symlink_metadata(path) {
            ensure!(
                meta.is_file() && !meta.file_type().is_symlink(),
                "Unsafe preferences file"
            );
        }
        let temp = parent.join(format!(".preferences-{}", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp)?;
            f.write_all(&serde_json::to_vec_pretty(self)?)?;
            f.sync_all()?;
            fs::rename(&temp, path)?;
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        let _ = fs::remove_file(temp);
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toggling_retains_inactive_roots_and_never_creates_them() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("not-created");
        let mut p = Preferences {
            global_root: Some(root.clone()),
            ..Default::default()
        };
        let space = Space::Existing {
            id: "w1".into(),
            label: "one".into(),
        };
        p.space_roots.insert(
            Preferences::key("server", "w1"),
            tmp.path().join("also-pending"),
        );
        p.mode = Mode::PerSpace;
        p.save(&tmp.path().join("prefs.json")).unwrap();
        let p = Preferences::load(&tmp.path().join("prefs.json")).unwrap();
        assert_eq!(p.global_root, Some(root.clone()));
        assert!(p.root("other-server", Some(&space)).is_none());
        assert!(
            p.root(
                "server",
                Some(&Space::New {
                    label: "new".into()
                })
            )
            .is_none()
        );
        assert!(!root.exists());
        assert!(!p.root("server", Some(&space)).unwrap().exists());
    }
}
