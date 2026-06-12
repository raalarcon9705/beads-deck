//! UI state enums and the background-thread message type.

use crate::bd::{HistoryEntry, Interaction, Issue, StatusDef};

/// What a clickable card/row did this frame.
pub(crate) enum RowAction {
    /// Open the bead in the detail panel.
    Open,
    /// Toggle the bead's membership in the bulk selection.
    Toggle,
}

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
        statuses: Vec<StatusDef>,
    },
    Detail {
        id: String,
        issue: Result<Issue, String>,
    },
    /// Lazy `bd history`, fetched only when the History tab is opened.
    History {
        id: String,
        history: Result<Vec<HistoryEntry>, String>,
    },
    /// Lazy comment-body search index (`bd export`), built only when searching.
    CommentIndex {
        map: std::collections::HashMap<String, String>,
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
