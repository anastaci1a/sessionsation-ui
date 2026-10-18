//! Pure creation choices. Editing never creates a directory or a workspace.
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Space {
    Existing { id: String, label: String },
    New { label: String },
}
impl Space {
    pub fn label(&self) -> &str {
        match self {
            Self::Existing { label, .. } | Self::New { label } => label,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Directory {
    #[default]
    Unset,
    Derived(PathBuf),
    Existing(PathBuf),
    Create {
        path: PathBuf,
        planned: Vec<PathBuf>,
    },
}
impl Directory {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Unset => None,
            Self::Derived(p) | Self::Existing(p) | Self::Create { path: p, .. } => Some(p),
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub account: Option<String>,
    pub space: Option<Space>,
    pub name: String,
    pub directory: Directory,
    pub directory_valid: bool,
    pub name_valid: bool,
    /// Revision rejects completions belonging to older choices.
    pub revision: u64,
}
pub fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.trim() == s
        && !s.chars().any(char::is_control)
        && s != "."
        && s != ".."
        && !s.contains('/')
}
pub fn absolute_clean(p: &Path) -> bool {
    p.is_absolute()
        && !p
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
}
impl Draft {
    pub fn changed(&mut self) {
        self.revision += 1;
        self.name_valid = false;
    }
    pub fn derive(&mut self, root: Option<&Path>) {
        if matches!(
            self.directory,
            Directory::Existing(_) | Directory::Create { .. }
        ) {
            return;
        }
        self.directory = if valid_name(&self.name) {
            root.map(|r| Directory::Derived(r.join(&self.name)))
                .unwrap_or_default()
        } else {
            Directory::Unset
        };
        self.directory_valid = false;
    }
    pub fn ready(&self) -> bool {
        self.account.is_some()
            && self.space.is_some()
            && valid_name(&self.name)
            && self.name_valid
            && self.directory_valid
            && self.directory.path().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn derive_and_edit_have_no_filesystem_effects() {
        let root = tempfile::tempdir().unwrap();
        let mut d = Draft {
            name: "A new session".into(),
            ..Default::default()
        };
        d.derive(Some(root.path()));
        assert_eq!(
            d.directory.path(),
            Some(root.path().join("A new session").as_path())
        );
        assert!(!d.directory.path().unwrap().exists());
        assert!(!d.ready());
        d.name.clear();
        d.derive(Some(root.path()));
        assert_eq!(d.directory, Directory::Unset);
    }
    #[test]
    fn explicit_path_survives_name_or_root_change() {
        let mut d = Draft {
            directory: Directory::Existing("/chosen".into()),
            name: "changed".into(),
            ..Default::default()
        };
        d.derive(Some(Path::new("/different")));
        assert_eq!(d.directory.path(), Some(Path::new("/chosen")));
    }
    #[test]
    fn names_cannot_escape_the_project_root() {
        for s in ["", "..", ".", "../other", "x/y", " x", "x\n"] {
            assert!(!valid_name(s));
        }
        assert!(valid_name("Work α"));
    }
}
