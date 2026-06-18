//! Workflow schema — what makes the deck workflow-agnostic.
//!
//! The state set's labels/colors/order, the legal transitions, role-ownership,
//! the hierarchy levels and the external-tracker label all come from an optional
//! `.beads/deck-workflow.json` in the workspace, layered over the `bd statuses`
//! baseline. With NO file the deck degrades gracefully to bd's statuses + a small
//! map for bd's universal built-ins + hash colors — i.e. its prior behavior — so
//! it stays generic for any project, not wired to one workflow.
//!
//! Read once per reload and published to a thread-local (mirrors `theme::pal()`),
//! so the free `theme::status_style(name)` and the query/board helpers can read it
//! without threading it through every call site.

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;

/// Color tokens selectable in the workflow editor (map to the palette in theme).
pub const COLOR_TOKENS: &[&str] =
    &["neutral", "blue", "green", "red", "amber", "yellow", "purple", "teal", "muted"];

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct WorkflowSchema {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<StateDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<TransitionDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<HierarchyDef>,
    /// Label for the external-tracker key shown on cards/detail (e.g. "Jira").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_ref_label: Option<String>,
    /// URL template for the external-tracker key, with `{key}` substituted by the
    /// display key (e.g. `https://acme.atlassian.net/browse/{key}`). When set, the
    /// ref on cards/detail becomes a clickable link that opens in the browser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_ref_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct StateDef {
    pub name: String,
    /// Display label; falls back to title-cased name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Named color token: blue|green|red|amber|yellow|purple|teal|muted|neutral.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// bd-style category (active|wip|done|frozen|…); informational.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Side state — off the linear pipeline (e.g. blocked/deferred). Rendered
    /// in a separate lane rather than inline as a pipeline column.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub side: bool,
    /// Role that owns this state (display only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TransitionDef {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct HierarchyDef {
    /// Conceptual levels top→bottom (e.g. ["epic","user_story","task","subtask"]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<String>,
    /// Which level is the unit that ships (display only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ships_at: Option<String>,
}

impl WorkflowSchema {
    pub fn is_empty(&self) -> bool {
        self.states.is_empty() && self.transitions.is_empty()
    }
    pub fn state(&self, name: &str) -> Option<&StateDef> {
        self.states.iter().find(|s| s.name == name)
    }
    /// Pipeline order index for a state name; unknown states sort last.
    pub fn order(&self, name: &str) -> usize {
        self.states.iter().position(|s| s.name == name).unwrap_or(usize::MAX)
    }
    pub fn is_side(&self, name: &str) -> bool {
        self.state(name).map(|s| s.side).unwrap_or(false)
    }
    pub fn owner(&self, name: &str) -> Option<&str> {
        self.state(name).and_then(|s| s.owner.as_deref())
    }
    /// Are explicit transitions declared? If not, the board is unrestricted.
    pub fn has_transitions(&self) -> bool {
        !self.transitions.is_empty()
    }
    /// Whether moving `from → to` is allowed. Same-state and the
    /// no-transitions-declared case are always allowed (unconfigured =
    /// unrestricted, so the deck never blocks a workflow it doesn't know).
    pub fn can_transition(&self, from: &str, to: &str) -> bool {
        if from == to || self.transitions.is_empty() {
            return true;
        }
        self.transitions.iter().any(|tr| tr.from == from && tr.to == to)
    }
    /// Role required for `from → to`, if the transition is declared with one.
    pub fn transition_role(&self, from: &str, to: &str) -> Option<&str> {
        self.transitions
            .iter()
            .find(|tr| tr.from == from && tr.to == to)
            .and_then(|tr| tr.role.as_deref())
    }
    /// Label for the external-tracker key (e.g. "Jira"); defaults to "Ref".
    pub fn ref_label(&self) -> &str {
        self.external_ref_label.as_deref().unwrap_or("Ref")
    }
    /// Browser URL for a display key, from the `external_ref_url` template with
    /// `{key}` substituted. None when no template is configured.
    pub fn ref_url(&self, key: &str) -> Option<String> {
        let tmpl = self.external_ref_url.as_deref().map(str::trim).filter(|s| !s.is_empty())?;
        Some(tmpl.replace("{key}", key))
    }
}

thread_local! {
    static ACTIVE: RefCell<Rc<WorkflowSchema>> =
        RefCell::new(Rc::new(WorkflowSchema::default()));
}

/// The active workflow schema (cheap Rc clone).
pub fn wf() -> Rc<WorkflowSchema> {
    ACTIVE.with(|a| a.borrow().clone())
}

/// Publish the active schema (called on each load + each frame from `App`).
pub fn set_wf(s: Rc<WorkflowSchema>) {
    ACTIVE.with(|a| *a.borrow_mut() = s);
}

/// Read `.beads/deck-workflow.json` from the workspace. Returns the empty schema
/// (→ fallback behavior) when the file is absent or invalid, so a missing/broken
/// config never wedges the UI.
pub fn read_workflow_schema(workspace: &str) -> WorkflowSchema {
    let path = format!("{workspace}/.beads/deck-workflow.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return WorkflowSchema::default();
    };
    match serde_json::from_str::<WorkflowSchema>(&content) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[beads-deck] invalid deck-workflow.json: {e}");
            WorkflowSchema::default()
        }
    }
}
