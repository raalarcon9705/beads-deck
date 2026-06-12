//! UI state enums and the background-thread message type.

use crate::bd::{HistoryEntry, Interaction, Issue};

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum Sort {
    Priority,
    StatusClosedFirst,
    Updated,
    Created,
    Id,
}

impl Sort {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Sort::Priority => "Priority",
            Sort::StatusClosedFirst => "Status (Closed first)",
            Sort::Updated => "Recently updated",
            Sort::Created => "Recently created",
            Sort::Id => "ID",
        }
    }
}

pub(crate) enum Msg {
    Loaded {
        issues: Result<Vec<Issue>, String>,
        events: Vec<Interaction>,
        roles: Vec<String>,
        comment_index: std::collections::HashMap<String, String>,
    },
    Detail {
        id: String,
        issue: Result<Issue, String>,
        history: Result<Vec<HistoryEntry>, String>,
    },
    Mutated {
        reselect: Option<String>,
        error: Option<String>,
        /// true = caller already patched local state; only reload on error.
        optimistic: bool,
    },
}

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum View {
    Board,
    Tree,
    Releases,
    Activity,
}

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum ThemeMode {
    Auto,
    Light,
    Dark,
}

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum DetailTab {
    Details,
    Comments,
    History,
}

/// A pending write action collected from the detail panel.
pub(crate) enum BeadAction {
    Status(String),
    Priority(i64),
    Assignee(Option<String>),
    ArchiveToggle(bool),
    Backlog,
    /// Move the bead to a release (Some) or clear its release (None).
    SetRelease(Option<String>),
    Delete,
}
