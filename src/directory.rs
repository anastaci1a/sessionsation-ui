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
    pub existing: bool,
    pub creatable: bool,
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
    let (path, existing) = match std::fs::canonicalize(&path) {
        Ok(p) if p.is_dir() => (p, true),
        _ => {
            let parent = path.parent().context("No parent directory")?;
            let parent = std::fs::canonicalize(parent)?;
            (
                parent.join(path.file_name().context("Invalid directory")?),
                false,
            )
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
    for entry in std::fs::read_dir(base)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(&prefix) && entry.path().is_dir() {
            children.push(entry.path());
        }
    }
    children.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    let creatable = !existing
        && std::fs::symlink_metadata(&path)
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
    Ok(Listing {
        path: Some(path),
        children,
        existing,
        creatable,
        error: None,
    })
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
}
