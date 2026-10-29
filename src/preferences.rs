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
/// Star field amount (Appearance).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Starfield {
    Off,
    // Stored under the pre-rename names so older builds still read the file;
    // the new names are accepted too.
    #[serde(rename = "calm", alias = "sparse")]
    Sparse,
    #[default]
    Normal,
    #[serde(rename = "lively", alias = "dense")]
    Dense,
}
impl Starfield {
    /// Top (maximum) to bottom (minimum).
    pub const ALL: [Self; 4] = [Self::Dense, Self::Normal, Self::Sparse, Self::Off];
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Sparse => "Sparse",
            Self::Normal => "Normal",
            Self::Dense => "Dense",
        }
    }
    /// Star density and twinkle rate in per mille.
    pub fn density(self) -> i64 {
        match self {
            Self::Off => 0,
            Self::Sparse => 450,
            Self::Normal => 1000,
            Self::Dense => 1700,
        }
    }
}
/// Star brightness (Appearance).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Brightness {
    Dim,
    #[default]
    Normal,
    Bright,
}
impl Brightness {
    /// Top (maximum) to bottom (minimum).
    pub const ALL: [Self; 3] = [Self::Bright, Self::Normal, Self::Dim];
    pub fn label(self) -> &'static str {
        match self {
            Self::Dim => "Dim",
            Self::Normal => "Normal",
            Self::Bright => "Bright",
        }
    }
    pub fn scale(self) -> i64 {
        match self {
            Self::Dim => 550,
            Self::Normal => 1000,
            Self::Bright => 1400,
        }
    }
}
/// The value one step up (toward the top of `all`) or down; it stops at the ends.
pub fn step<T: Copy + PartialEq>(all: &[T], current: T, up: bool) -> T {
    let i = all.iter().position(|v| *v == current).unwrap_or(0);
    all[if up {
        i.saturating_sub(1)
    } else {
        (i + 1).min(all.len() - 1)
    }]
}
/// Arrow for the value in `all`: down-only at the top, up-only at the bottom.
pub fn choice_arrow<T: Copy + PartialEq>(all: &[T], current: T) -> &'static str {
    let i = all.iter().position(|v| *v == current).unwrap_or(0);
    match (i == 0, i + 1 == all.len()) {
        (true, false) => "⬇",
        (false, true) => "⬆",
        _ => "⬍",
    }
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
    #[serde(default)]
    pub starfield: Starfield,
    #[serde(default)]
    pub brightness: Brightness,
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
            starfield: Starfield::Normal,
            brightness: Brightness::Normal,
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
#[cfg(test)]
mod arrow_tests {
    use super::*;
    #[test]
    fn steps_stop_at_the_ends_and_arrows_follow_position() {
        let all = Starfield::ALL;
        assert_eq!(step(&all, Starfield::Dense, true), Starfield::Dense);
        assert_eq!(step(&all, Starfield::Off, false), Starfield::Off);
        assert_eq!(step(&all, Starfield::Normal, true), Starfield::Dense);
        assert_eq!(choice_arrow(&all, Starfield::Dense), "⬇");
        assert_eq!(choice_arrow(&all, Starfield::Normal), "⬍");
        assert_eq!(choice_arrow(&all, Starfield::Off), "⬆");
        for g in ["⬍", "⬇", "⬆"] {
            assert_eq!(unicode_width::UnicodeWidthStr::width(g), 1, "{g}");
        }
    }
    #[test]
    fn old_starfield_names_load_as_the_nearest_new_value() {
        let load =
            |name: &str| -> Starfield { serde_json::from_str(&format!("\"{name}\"")).unwrap() };
        assert_eq!(load("calm"), Starfield::Sparse);
        assert_eq!(load("lively"), Starfield::Dense);
        assert_eq!(load("sparse"), Starfield::Sparse);
        assert_eq!(load("dense"), Starfield::Dense);
        assert_eq!(load("off"), Starfield::Off);
        assert_eq!(load("normal"), Starfield::Normal);
    }
}
