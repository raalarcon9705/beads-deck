//! Thin layer over the `bd` (beads) CLI. Everything is fetched as JSON and
//! parsed with serde. All calls shell out to `bd` with the workspace directory
//! as the process CWD, so bd resolves the right embedded/Dolt backend itself.

use serde::Deserialize;
use std::process::Command;

/// One bead, as returned by `bd list --json` / `bd show --json`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Issue {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub issue_type: String,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub closed_at: Option<String>,
    #[serde(default)]
    pub close_reason: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    /// Present in `bd list`/`show` JSON only when the bead has labels.
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub comment_count: i64,
    #[serde(default)]
    pub dependency_count: i64,
    #[serde(default)]
    pub dependent_count: i64,
    /// Populated by `bd show` (list of dependency issue objects). Empty in list.
    #[serde(default)]
    pub dependencies: Vec<Dep>,
    /// Populated by `bd show --include-comments`.
    #[serde(default)]
    pub comments: Vec<Comment>,
}

/// Dependency object. `bd show` returns `{id, title, status}`; `bd list`
/// returns a different relation shape (`{issue_id, depends_on_id, type}`).
/// All fields default so either shape deserializes; the UI only uses the
/// `show` form.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Dep {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default, rename = "type")]
    pub dep_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Comment {
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub created_at: String,
}

/// One Dolt commit touching an issue (`bd history --json`). Go-style keys.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct HistoryEntry {
    #[serde(default, rename = "CommitHash")]
    pub commit_hash: String,
    #[serde(default, rename = "Committer")]
    pub committer: String,
    #[serde(default, rename = "CommitDate")]
    pub commit_date: String,
    #[serde(default, rename = "Issue")]
    pub issue: Option<HistorySnap>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct HistorySnap {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub assignee: Option<String>,
}

/// One entry from `.beads/interactions.jsonl` — the bd audit/event log.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Interaction {
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub issue_id: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub extra: serde_json::Value,
}

impl Interaction {
    pub fn field(&self) -> &str {
        self.extra.get("field").and_then(|v| v.as_str()).unwrap_or("")
    }
    pub fn new_value(&self) -> String {
        match self.extra.get("new_value") {
            Some(v) => v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()),
            None => String::new(),
        }
    }
}

/// Build an id → lowercased-comment-text index via `bd export` (one call,
/// includes all comment bodies). Used to extend search into comments, which
/// `bd search` itself does not cover.
pub fn comment_index(workspace: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let out = match Command::new("bd").arg("export").current_dir(workspace).output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return map,
    };
    for line in String::from_utf8_lossy(&out).lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let Some(id) = v.get("id").and_then(|x| x.as_str()) else { continue };
        let mut text = String::new();
        if let Some(arr) = v.get("comments").and_then(|c| c.as_array()) {
            for c in arr {
                if let Some(t) = c.get("text").and_then(|x| x.as_str()) {
                    text.push_str(t);
                    text.push('\n');
                }
            }
        }
        if !text.is_empty() {
            map.insert(id.to_string(), text.to_lowercase());
        }
    }
    map
}

/// Agent roster from initech (`initech config get roles`). Empty if initech or
/// the config is unavailable.
pub fn read_roles(workspace: &str) -> Vec<String> {
    match Command::new("initech")
        .args(["config", "get", "roles"])
        .current_dir(workspace)
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// Run an arbitrary mutation command (bd / initech) in the workspace.
pub fn run_cmd(workspace: &str, program: &str, args: &[String]) -> Result<(), String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(workspace)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Read the workspace event log (newest entries last). Returns empty when the
/// file is absent.
pub fn read_interactions(workspace: &str) -> Vec<Interaction> {
    let path = format!("{workspace}/.beads/interactions.jsonl");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Interaction>(l).ok())
        .collect()
}

fn run(workspace: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("bd")
        .args(args)
        .current_dir(workspace)
        .output()
        .map_err(|e| format!("no se pudo ejecutar `bd`: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("bd {:?} falló: {}", args, err.trim()));
    }
    Ok(out.stdout)
}

/// All beads in the workspace (every status), flat.
pub fn list_all(workspace: &str) -> Result<Vec<Issue>, String> {
    let bytes = run(workspace, &["list", "--status", "all", "--limit", "0", "--json"])?;
    serde_json::from_slice::<Vec<Issue>>(&bytes).map_err(|e| format!("JSON list inválido: {e}"))
}

/// Create an epic via `bd q` (quick capture, prints only the new ID) and return
/// that ID. Optional comma-joinable labels are attached with `-l`. `bd q` has no
/// description flag, so only the title/type/labels are set here.
pub fn create_epic(workspace: &str, title: &str, labels: &[String]) -> Result<String, String> {
    let mut args = vec!["q", title, "--type", "epic"];
    let joined = labels.join(",");
    if !joined.is_empty() {
        args.push("-l");
        args.push(&joined);
    }
    let bytes = run(workspace, &args)?;
    let id = String::from_utf8_lossy(&bytes).trim().to_string();
    if id.is_empty() {
        return Err("bd q no devolvió un ID".to_string());
    }
    Ok(id)
}

/// Full detail for one bead, including comments and dependency objects.
pub fn show(workspace: &str, id: &str) -> Result<Issue, String> {
    let bytes = run(
        workspace,
        &["show", id, "--json", "--include-comments", "--include-dependents"],
    )?;
    let v: Vec<Issue> =
        serde_json::from_slice(&bytes).map_err(|e| format!("JSON show inválido: {e}"))?;
    v.into_iter()
        .next()
        .ok_or_else(|| format!("{id}: sin resultado"))
}

/// Dolt commit history for a bead. Returns Err with the bd message when the
/// known bd 1.0.x NULL-description bug trips (handled/displayed by the UI).
pub fn history(workspace: &str, id: &str) -> Result<Vec<HistoryEntry>, String> {
    let bytes = run(workspace, &["history", id, "--json"])?;
    serde_json::from_slice::<Vec<HistoryEntry>>(&bytes)
        .map_err(|e| format!("JSON history inválido: {e}"))
}
