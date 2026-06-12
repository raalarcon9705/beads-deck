//! Read-side helpers on `App`: filtering, sorting and derived lists
//! (statuses, releases, assignees) plus the release→epic conversion.

use crate::app::App;
use crate::bd::{self, Issue};
use crate::state::{Msg, Sort};
use crate::util::*;
use std::thread;

/// Category sort rank for board columns (pipeline order, left→right).
fn status_category_rank(cat: &str) -> usize {
    match cat {
        "active" => 0,
        "wip" => 1,
        "done" => 2,
        "frozen" => 3,
        _ => 4,
    }
}

impl App {
    pub(crate) fn passes_filter(&self, i: &Issue) -> bool {
        if let Some(s) = &self.filter_status {
            if &i.status != s {
                return false;
            }
        }
        if let Some(pr) = self.filter_priority {
            if i.priority != pr {
                return false;
            }
        }
        if let Some(a) = &self.filter_assignee {
            if i.assignee.as_deref().unwrap_or("") != a {
                return false;
            }
        }
        if !self.search.is_empty() {
            let q = self.search.to_lowercase();
            let in_meta = i.id.to_lowercase().contains(&q)
                || i.title.to_lowercase().contains(&q)
                || i.description.to_lowercase().contains(&q);
            let in_comments = self
                .comment_index
                .get(&i.id)
                .map(|t| t.contains(&q))
                .unwrap_or(false);
            if !in_meta && !in_comments {
                return false;
            }
        }
        true
    }

    /// Sort a list of issue indices according to the active sort mode.
    pub(crate) fn apply_sort(&self, mut v: Vec<usize>) -> Vec<usize> {
        let iss = &self.issues;
        match self.sort {
            Sort::Priority => v.sort_by(|&a, &b| {
                iss[a]
                    .priority
                    .cmp(&iss[b].priority)
                    .then_with(|| iss[a].id.cmp(&iss[b].id))
            }),
            Sort::StatusClosedFirst => v.sort_by_key(|&i| {
                let closed_first = if iss[i].status == "closed" { 0 } else { 1 };
                (closed_first, status_rank(&iss[i].status), iss[i].id.clone())
            }),
            Sort::Updated => v.sort_by(|&a, &b| iss[b].updated_at.cmp(&iss[a].updated_at)),
            Sort::Created => v.sort_by(|&a, &b| iss[b].created_at.cmp(&iss[a].created_at)),
            Sort::Id => v.sort_by(|&a, &b| iss[a].id.cmp(&iss[b].id)),
        }
        v
    }

    /// Distinct statuses present in the loaded beads, ordered by the canonical
    /// preference (`STATUS_ORDER`) with any project-specific extras appended.
    /// Fully dynamic per workspace.
    pub(crate) fn statuses_present(&self) -> Vec<String> {
        let mut set: Vec<String> = Vec::new();
        for i in &self.issues {
            if !set.iter().any(|s| s == &i.status) {
                set.push(i.status.clone());
            }
        }
        set.sort_by(|a, b| {
            status_rank(a)
                .cmp(&status_rank(b))
                .then_with(|| a.cmp(b))
        });
        set
    }

    /// All statuses selectable in dropdowns: the configured workflow statuses
    /// (built-in + custom from `bd statuses`) plus any status present on issues
    /// but not configured. Falls back to present statuses if `bd statuses` is
    /// unavailable.
    pub(crate) fn selectable_statuses(&self) -> Vec<String> {
        let mut v: Vec<String> = self.workflow_statuses.iter().map(|s| s.name.clone()).collect();
        for s in self.statuses_present() {
            if !v.contains(&s) {
                v.push(s);
            }
        }
        v
    }

    /// Statuses to render as board columns: those present on issues plus any
    /// custom-configured status (so newly-added custom states appear as empty
    /// columns), ordered as a left→right pipeline by category then config order.
    pub(crate) fn board_columns(&self) -> Vec<String> {
        let present = self.statuses_present();
        let mut cols: Vec<String> = Vec::new();
        for s in &self.workflow_statuses {
            let shown = present.iter().any(|p| p == &s.name) || s.custom;
            if shown && !cols.contains(&s.name) {
                cols.push(s.name.clone());
            }
        }
        cols.sort_by_key(|name| {
            self.workflow_statuses
                .iter()
                .position(|s| &s.name == name)
                .map(|i| (status_category_rank(&self.workflow_statuses[i].category), i))
                .unwrap_or((9, 0))
        });
        for s in &present {
            if !cols.contains(s) {
                cols.push(s.clone());
            }
        }
        if cols.is_empty() {
            return present;
        }
        cols
    }

    /// Distinct release names present across all loaded beads (sorted).
    pub(crate) fn releases(&self) -> Vec<String> {
        let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for i in &self.issues {
            if let Some(r) = release_of(i) {
                set.insert(r.to_string());
            }
        }
        set.into_iter().collect()
    }

    /// Convert a release into an epic: create the epic, then reparent every bead
    /// carrying `release:<name>` under it. The release label is kept, so the
    /// release grouping and the epic coexist. Runs in a background thread.
    pub(crate) fn convert_release(&mut self, release: String) {
        let ids: Vec<String> = self
            .issues
            .iter()
            .filter(|i| release_of(i) == Some(release.as_str()))
            .map(|i| i.id.clone())
            .collect();
        self.pending_mutations += 1;
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        thread::spawn(move || {
            let error = (|| {
                let epic = bd::create_epic(&ws, &release, &[format!("{RELEASE_PREFIX}{release}")])?;
                if !ids.is_empty() {
                    let mut args: Vec<String> = vec!["update".into()];
                    args.extend(ids);
                    args.push("--parent".into());
                    args.push(epic.clone());
                    bd::run_cmd(&ws, "bd", &args)?;
                }
                Ok::<_, String>(epic)
            })();
            let (reselect, error) = match error {
                Ok(epic) => (Some(epic), None),
                Err(e) => (None, Some(e)),
            };
            let _ = tx.send(Msg::Mutated { reselect, error, optimistic: false });
            ctx.request_repaint();
        });
    }

    /// Unique assignees present in the loaded beads (sorted).
    pub(crate) fn assignees(&self) -> Vec<String> {
        let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for i in &self.issues {
            if let Some(a) = i.assignee.as_deref().filter(|a| !a.is_empty()) {
                set.insert(a.to_string());
            }
        }
        set.into_iter().collect()
    }
}
