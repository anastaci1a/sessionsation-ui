//! Same-name detection in a target Space and the "typable terminal" check.
//!
//! HERDR names tabs (`label`) and, optionally, panes (`label`). A new or
//! resumed session about to take a name already shown by a tab or pane of its
//! target Space is flagged; when the clash is one terminal that is verifiably at
//! its shell prompt, the session may be run in that terminal instead.
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tab {
    pub id: String,
    pub space: String,
    pub label: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pane {
    pub id: String,
    pub tab: String,
    pub space: String,
    pub label: Option<String>,
    pub agent_status: String,
}
/// One consistent snapshot of the host's tabs and panes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub tabs: Vec<Tab>,
    pub panes: Vec<Pane>,
}
/// The single terminal a same-name clash points at, verified at its shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Existing {
    pub pane: String,
    pub tab: String,
}

/// Names clash when equal after trimming, ignoring letter case.
pub fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    !a.is_empty() && a.to_lowercase() == b.to_lowercase()
}

impl Layout {
    /// Parse `herdr tab list` and `herdr pane list` results.
    pub fn parse(tabs: &Value, panes: &Value) -> Self {
        let text = |v: &Value, k: &str| v[k].as_str().map(str::to_owned);
        Self {
            tabs: tabs["tabs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| {
                    Some(Tab {
                        id: text(t, "tab_id")?,
                        space: text(t, "workspace_id")?,
                        label: text(t, "label").unwrap_or_default(),
                    })
                })
                .collect(),
            panes: panes["panes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| {
                    Some(Pane {
                        id: text(p, "pane_id")?,
                        tab: text(p, "tab_id")?,
                        space: text(p, "workspace_id")?,
                        label: text(p, "label").filter(|l| !l.trim().is_empty()),
                        agent_status: text(p, "agent_status").unwrap_or_default(),
                    })
                })
                .collect(),
        }
    }
    /// Whether any tab or pane of `space` is already named `name`.
    pub fn in_use(&self, space: &str, name: &str) -> bool {
        self.tabs
            .iter()
            .any(|t| t.space == space && same(&t.label, name))
            || self
                .panes
                .iter()
                .any(|p| p.space == space && p.label.as_deref().is_some_and(|l| same(l, name)))
    }
    /// The terminals a clash points at. A same-named tab with one pane is that
    /// pane; a same-named tab with several panes counts only panes that are
    /// themselves named; a named pane is itself. Deduplicated, in listing order.
    pub fn candidates(&self, space: &str, name: &str) -> Vec<&Pane> {
        let mut out: Vec<&Pane> = vec![];
        for pane in self.panes.iter().filter(|p| p.space == space) {
            let named = pane.label.as_deref().is_some_and(|l| same(l, name));
            let in_named_tab = self
                .tabs
                .iter()
                .any(|t| t.id == pane.tab && same(&t.label, name))
                && self.panes.iter().filter(|q| q.tab == pane.tab).count() == 1;
            if (named || in_named_tab) && !out.iter().any(|q| q.id == pane.id) {
                out.push(pane);
            }
        }
        out
    }
}

/// A pane is typable only when HERDR reports no agent on it and its sole
/// foreground process is the pane's own shell (the process group leader is the
/// shell PID HERDR itself spawned), so nothing else can be reading the keys.
pub fn typable(agent_status: &str, process_info: &Value) -> bool {
    let info = &process_info["process_info"];
    let (Some(shell), Some(group)) = (
        info["shell_pid"].as_u64(),
        info["foreground_process_group_id"].as_u64(),
    ) else {
        return false;
    };
    agent_status == "unknown"
        && group == shell
        && info["foreground_processes"]
            .as_array()
            .is_some_and(|p| p.len() == 1 && p[0]["pid"].as_u64() == Some(shell))
}

/// Contract version from which inline `resume` accepts `--confirm-unfinished`.
const INLINE_CONFIRM_FROM: (u64, u64) = (1, 31);

/// Whether the connected backend can run this request inline in a terminal.
pub fn inline_supported(capabilities: &Value, request: &Value) -> bool {
    if request["confirm_unfinished"] != true {
        return true;
    }
    let version = capabilities["contract_version"].as_str().and_then(|v| {
        let mut parts = v.split('.').map(|p| p.parse::<u64>().ok());
        Some((parts.next()??, parts.next()??))
    });
    version.is_some_and(|v| v >= INLINE_CONFIRM_FROM)
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The shell line that runs a validated launch request in an existing terminal:
/// the backend's own inline paths (`resume SESSION --expected-generation G`,
/// `run --account A`), preceded by entering the chosen directory.
pub fn inline_command(
    binary: &std::path::Path,
    data_dir: &std::path::Path,
    request: &Value,
) -> Option<String> {
    let mut line = String::new();
    let resume = request["session_id"].as_str();
    if let Some(path) = request["directory"]["path"].as_str() {
        if request["directory"]["create"]
            .as_array()
            .is_some_and(|c| !c.is_empty())
        {
            line += &format!("mkdir -p -- {} && ", quote(path));
        }
        line += &format!("cd -- {} && ", quote(path));
    }
    line += &format!(
        "{} --data-dir {}",
        quote(binary.to_str()?),
        quote(data_dir.to_str()?)
    );
    if let Some(session) = resume {
        line += &format!(
            " resume {} --expected-generation {}",
            quote(session),
            quote(request["expected_generation"].as_str()?)
        );
        if request["confirm_unfinished"] == true {
            line += " --confirm-unfinished";
        }
    } else {
        line += &format!(" run --account {}", quote(request["account_id"].as_str()?));
    }
    if let Some(p) = request["permissions"].as_str()
        && matches!(p, "yolo" | "default")
    {
        line += &format!(" --permissions {p}");
    }
    Some(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn layout() -> Layout {
        Layout::parse(
            &json!({"tabs":[
                {"tab_id":"w1:t1","workspace_id":"w1","label":"1"},
                {"tab_id":"w1:t2","workspace_id":"w1","label":"Work"},
                {"tab_id":"w1:t3","workspace_id":"w1","label":"Duo"},
                {"tab_id":"w2:t1","workspace_id":"w2","label":"Other"}]}),
            &json!({"panes":[
                {"pane_id":"w1:p1","tab_id":"w1:t1","workspace_id":"w1","agent_status":"unknown","label":"named"},
                {"pane_id":"w1:p2","tab_id":"w1:t2","workspace_id":"w1","agent_status":"unknown"},
                {"pane_id":"w1:p3","tab_id":"w1:t3","workspace_id":"w1","agent_status":"unknown"},
                {"pane_id":"w1:p4","tab_id":"w1:t3","workspace_id":"w1","agent_status":"unknown"},
                {"pane_id":"w2:p1","tab_id":"w2:t1","workspace_id":"w2","agent_status":"unknown"}]}),
        )
    }
    #[test]
    fn names_match_trimmed_and_case_insensitively_within_the_space_only() {
        let l = layout();
        assert!(l.in_use("w1", "work"));
        assert!(l.in_use("w1", "  WORK "));
        assert!(l.in_use("w1", "Named"), "a pane label counts");
        assert!(!l.in_use("w1", "Other"), "another Space does not");
        assert!(l.in_use("w2", "other"));
        assert!(!l.in_use("w1", "Wor"), "whole name, never a prefix");
        assert!(!l.in_use("w1", ""), "an empty name is never in use");
        assert!(!l.in_use("missing", "Work"));
    }
    #[test]
    fn candidates_follow_single_pane_tabs_and_named_panes_only() {
        let l = layout();
        let ids = |n: &str| {
            l.candidates("w1", n)
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("work"), ["w1:p2"]);
        assert_eq!(ids("named"), ["w1:p1"]);
        assert!(
            l.in_use("w1", "Duo") && ids("duo").is_empty(),
            "a multi-pane tab is flagged but names no single terminal"
        );
    }
    #[test]
    fn inline_commands_use_the_backends_own_launch_paths() {
        let bin = std::path::Path::new("/opt/b in/sessionsation");
        let root = std::path::Path::new("/data");
        let new = json!({"account_id":"codex:me","directory":{"path":"/p/it's","create":["/p/it's"]},"permissions":"default"});
        assert_eq!(
            inline_command(bin, root, &new).unwrap(),
            "mkdir -p -- '/p/it'\\''s' && cd -- '/p/it'\\''s' && '/opt/b in/sessionsation' --data-dir '/data' run --account 'codex:me' --permissions default"
        );
        let resume = json!({"session_id":"s1","expected_generation":"g9","confirm_unfinished":true,"directory":{"path":"/p","create":[]},"permissions":"yolo"});
        assert_eq!(
            inline_command(bin, root, &resume).unwrap(),
            "cd -- '/p' && '/opt/b in/sessionsation' --data-dir '/data' resume 's1' --expected-generation 'g9' --confirm-unfinished --permissions yolo"
        );
        assert!(inline_command(bin, root, &json!({"session_id":"s1"})).is_none());
        assert!(inline_supported(
            &json!({"contract_version":"1.31.0"}),
            &resume
        ));
        assert!(!inline_supported(
            &json!({"contract_version":"1.30.2"}),
            &resume
        ));
        assert!(!inline_supported(&json!({}), &resume));
        assert!(inline_supported(&json!({}), &new), "run --account is old");
    }
    fn info(shell: u64, group: u64, pids: &[u64]) -> Value {
        json!({"process_info":{"shell_pid":shell,"foreground_process_group_id":group,
            "foreground_processes":pids.iter().map(|p| json!({"pid":p})).collect::<Vec<_>>()}})
    }
    #[test]
    fn typable_requires_the_panes_own_shell_alone_in_the_foreground() {
        assert!(typable("unknown", &info(10, 10, &[10])));
        assert!(!typable("unknown", &info(10, 20, &[20])), "program running");
        assert!(
            !typable("unknown", &info(10, 10, &[10, 11])),
            "extra process"
        );
        assert!(
            !typable("unknown", &info(10, 10, &[11])),
            "not the shell pid"
        );
        assert!(!typable("working", &info(10, 10, &[10])), "agent reported");
        assert!(!typable("idle", &info(10, 10, &[10])), "agent reported");
        assert!(!typable("unknown", &json!({})), "no data, no offer");
        assert!(!typable(
            "unknown",
            &json!({"process_info":{"shell_pid":10}})
        ));
    }
}
