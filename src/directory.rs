//! Read-only path completion; directory creation remains a backend Start effect.
use anyhow::{Context, Result};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub path: Option<PathBuf>,
    pub children: Vec<PathBuf>,
    pub resolved: BTreeMap<PathBuf, PathBuf>,
    pub existing: bool,
    pub creatable: bool,
    /// Folders a choice of `path` would create, parents first (empty when it exists).
    pub missing: Vec<PathBuf>,
    pub error: Option<String>,
}
pub fn expand(input: &str, env: &BTreeMap<String, String>) -> String {
    let input = if input == "~" || input.starts_with("~/") {
        format!(
            "{}{}",
            env.get("HOME").map(String::as_str).unwrap_or("~"),
            &input[1..]
        )
    } else {
        input.into()
    };
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && chars.get(i + 1) == Some(&'$') {
            out.push('$');
            i += 2;
            continue;
        }
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        let braces = chars.get(i) == Some(&'{');
        if braces {
            i += 1;
        }
        let begin = i;
        while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        let key: String = chars[begin..i].iter().collect();
        let valid = key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && (!braces || chars.get(i) == Some(&'}'));
        if braces && chars.get(i) == Some(&'}') {
            i += 1;
        }
        if valid && let Some(value) = env.get(&key) {
            out.push_str(value);
        } else {
            out.extend(chars[start..i].iter());
        }
    }
    out
}
pub fn inspect(input: &str, initial: &Path, env: &BTreeMap<String, String>) -> Result<Listing> {
    if input.is_empty() {
        return Ok(Listing::default());
    }
    let expanded = PathBuf::from(expand(input, env));
    let path = if expanded.is_absolute() {
        expanded
    } else {
        initial.join(expanded)
    };
    // Missing folders from the nearest existing ancestor down to the typed path.
    let mut missing = Vec::new();
    let (path, existing) = match std::fs::canonicalize(&path) {
        Ok(p) if p.is_dir() => (p, true),
        Ok(_) => {
            let parent = path.parent().context("No parent directory")?;
            let parent = std::fs::canonicalize(parent)?;
            (
                parent.join(path.file_name().context("Invalid directory")?),
                false,
            )
        }
        Err(_) => {
            let mut names = Vec::new();
            let mut cursor = path.as_path();
            let base = loop {
                names.push(cursor.file_name().context("Invalid directory")?.to_owned());
                cursor = cursor.parent().context("No parent directory")?;
                match std::fs::canonicalize(cursor) {
                    Ok(p) if p.is_dir() => break p,
                    Ok(_) => anyhow::bail!("Not a directory: {}", cursor.display()),
                    Err(_) => {}
                }
            };
            let mut full = base;
            for name in names.iter().rev() {
                full = full.join(name);
                missing.push(full.clone());
            }
            (full, false)
        }
    };
    let base = if existing {
        path.clone()
    } else {
        path.parent().unwrap().into()
    };
    let prefix = if existing {
        String::new()
    } else {
        path.file_name().unwrap().to_string_lossy().into_owned()
    };
    let mut children = Vec::new();
    let mut resolved = BTreeMap::new();
    if missing.len() <= 1 {
        for entry in std::fs::read_dir(base)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(&prefix)
                && entry.path().is_dir()
                && let Ok(real) = std::fs::canonicalize(entry.path())
            {
                resolved.insert(entry.path(), real);
                children.push(entry.path());
            }
        }
    }
    children.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    let creatable = !existing
        && std::fs::symlink_metadata(&path)
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
    Ok(Listing {
        path: Some(path),
        children,
        resolved,
        existing,
        creatable,
        missing,
        error: None,
    })
}

/// Configured roots can have been materialized by an earlier successful launch.
/// Consume only existing canonical ancestors; never adopt the session leaf.
pub fn remaining_plan(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut remaining = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        if index + 1 < paths.len() {
            match std::fs::symlink_metadata(path) {
                Ok(meta) => {
                    anyhow::ensure!(
                        meta.is_dir()
                            && !meta.file_type().is_symlink()
                            && std::fs::canonicalize(path)? == *path,
                        "Staged projects root changed identity: {}",
                        path.display()
                    );
                    continue;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        remaining.push(path.clone());
    }
    Ok(remaining)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expansion_never_evaluates_shell() {
        let e = BTreeMap::from([("HOME".into(), "/h".into()), ("ROOT".into(), "/r".into())]);
        assert_eq!(
            expand("~/$ROOT/${missing}/$(touch x)/\\$ROOT", &e),
            "/h//r/${missing}/$(touch x)/$ROOT"
        );
    }
    #[test]
    fn missing_leaf_is_preview_only() {
        let t = tempfile::tempdir().unwrap();
        let l = inspect("new", t.path(), &BTreeMap::new()).unwrap();
        assert!(l.creatable);
        assert!(!l.existing);
        assert!(!l.path.unwrap().exists());
    }
    #[test]
    fn previous_launch_root_is_reusable_but_existing_leaf_is_never_adopted() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("projects");
        let leaf = root.join("new");
        std::fs::create_dir_all(&leaf).unwrap();
        assert_eq!(remaining_plan(&[root, leaf.clone()]).unwrap(), vec![leaf]);
    }
    #[test]
    fn changed_root_symlink_is_rejected() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("link");
        std::os::unix::fs::symlink(t.path(), &root).unwrap();
        assert!(remaining_plan(&[root.clone(), root.join("new")]).is_err());
    }
}
