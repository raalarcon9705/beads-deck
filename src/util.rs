//! Small domain + filesystem helpers shared across the app.

use crate::bd::Issue;

/// Beadbox-faithful bucketing. Precedence: Archived > Backlog > Active.
/// - Archived = carries the `archived` label.
/// - Backlog  = priority P4 (bd's "backlog" priority).
pub(crate) fn is_archived(i: &Issue) -> bool {
    i.labels.iter().any(|l| l.eq_ignore_ascii_case("archived"))
}
pub(crate) fn is_backlog(i: &Issue) -> bool {
    !is_archived(i) && i.priority == 4
}

/// Label prefix that marks a bead as belonging to a release/milestone, e.g.
/// `release:v0.3.0`. Releases are modelled as labels (orthogonal to the
/// single-parent epic hierarchy) so a bead can be in a release AND an epic.
pub(crate) const RELEASE_PREFIX: &str = "release:";

/// The release a bead belongs to (the text after `release:`), if any.
pub(crate) fn release_of(i: &Issue) -> Option<&str> {
    i.labels.iter().find_map(|l| l.strip_prefix(RELEASE_PREFIX)).filter(|s| !s.is_empty())
}

/// A bead counts as shipped when bd considers it closed.
pub(crate) fn is_closed(i: &Issue) -> bool {
    i.status == "closed" || i.closed_at.is_some()
}

/// The external-tracker key for display, with a redundant `<label>-` prefix
/// stripped (e.g. `jira-ST-1236` → `ST-1236` when the label is "Jira"). None
/// when the bead has no external_ref.
pub(crate) fn external_key(i: &Issue) -> Option<String> {
    let raw = i.external_ref.as_deref().map(str::trim).filter(|s| !s.is_empty())?;
    let prefix = format!("{}-", crate::schema::wf().ref_label().to_lowercase());
    let key = raw.strip_prefix(prefix.as_str()).unwrap_or(raw);
    Some(key.to_string())
}

/// Fallback pipeline order used ONLY when no workflow schema is configured.
/// Generic bd built-ins, no workflow-specific states.
pub(crate) const STATUS_ORDER: &[&str] = &[
    "open",
    "in_progress",
    "blocked",
    "closed",
    "deferred",
];

/// Pipeline rank for a status: from the workflow schema's order when one is
/// configured (unknown states sort last), otherwise the generic fallback.
pub(crate) fn status_rank(s: &str) -> usize {
    let schema = crate::schema::wf();
    if !schema.is_empty() {
        return schema.order(s);
    }
    STATUS_ORDER.iter().position(|x| *x == s).unwrap_or(99)
}

/// mtime of the workspace event log — the cheap real-time change signal.
///
/// We watch only `interactions.jsonl` on purpose: `bd list` (run on every reload)
/// rewrites the Dolt store under `…/.dolt/noms/`, so watching those files would
/// cause an infinite reload loop. The event log is only appended on real status /
/// assignment activity, so it's a safe signal. (Create/delete and `bd config`
/// changes land only in the Dolt store and aren't auto-detected — use Reload.)
pub(crate) fn beads_event_mtime(ws: &str) -> Option<std::time::SystemTime> {
    std::fs::metadata(format!("{ws}/.beads/interactions.jsonl"))
        .and_then(|m| m.modified())
        .ok()
}

pub(crate) fn has_beads_dir(path: &str) -> bool {
    std::path::Path::new(path).join(".beads").is_dir()
}

pub(crate) fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Replace the home prefix with `~` for display.
pub(crate) fn short_path(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME") {
        if let Some(rest) = path.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    path.to_string()
}

// ---------------------------------------------------------------------------
// Workspace registry — persisted at ~/.beads-deck/registry.json
// ---------------------------------------------------------------------------
