//! The `App` struct, its lifecycle (eframe), workspace management and the
//! `bd` command runners. View rendering lives under `crate::views`; data
//! queries live in `crate::query`.

use crate::bd::{self, HistoryEntry, Interaction, Issue};
use crate::registry::{load_registry, save_registry, Registry, WorkspaceEntry};
use crate::state::*;
use crate::util::*;
use crate::{markdown, theme as t};
use eframe::egui;
use egui::Margin;
use egui_commonmark::CommonMarkCache;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

/// A previously-fetched bead detail, kept so reopening it is instant while a
/// fresh `bd show` runs in the background (stale-while-revalidate). Stores the
/// preprocessed markdown too, so we don't re-run mermaid/mmdc on cache hits.
#[derive(Clone)]
pub(crate) struct CachedDetail {
    pub(crate) issue: Issue,
    pub(crate) md: String,
    pub(crate) comments_md: Vec<String>,
}

pub(crate) struct App {
    pub(crate) ctx: egui::Context,
    pub(crate) tx: Sender<Msg>,
    pub(crate) rx: Receiver<Msg>,

    pub(crate) workspace: String,
    pub(crate) registry: Registry,
    pub(crate) in_workspace: bool,
    pub(crate) show_add: bool,
    pub(crate) add_path: String,
    pub(crate) add_error: Option<String>,
    pub(crate) issues: Vec<Issue>,
    pub(crate) events: Vec<Interaction>,
    pub(crate) comment_index: std::collections::HashMap<String, String>,
    pub(crate) board_col_rects: std::collections::HashMap<String, egui::Rect>,
    pub(crate) roles: Vec<String>,
    /// Valid workflow statuses (built-in + custom) from `bd statuses`.
    pub(crate) workflow_statuses: Vec<crate::bd::StatusDef>,
    /// Workflow schema (labels/colors/order/transitions/roles/hierarchy) from
    /// `.beads/deck-workflow.json`. Empty → fallback behavior. Published to the
    /// `crate::schema` thread-local each frame so the free helpers can read it.
    pub(crate) workflow_schema: std::rc::Rc<crate::schema::WorkflowSchema>,
    /// Bulk-selection mode: cards/rows show checkboxes and a floating action bar.
    pub(crate) select_mode: bool,
    /// Beads currently selected for a bulk action.
    pub(crate) selected_ids: std::collections::HashSet<String>,
    /// Anchor for OS-style shift range-select (last bead toggled without shift).
    pub(crate) select_anchor: Option<String>,
    /// Flat top→bottom visual order of selectable beads in the active view,
    /// rebuilt each frame; used to resolve shift range-selection.
    pub(crate) visible_order: Vec<String>,
    /// Pending confirmation for a bulk delete.
    pub(crate) confirm_bulk_delete: bool,
    pub(crate) action_error: Option<String>,
    pub(crate) confirm_delete: Option<String>,
    pub(crate) confirm_delete_agent: Option<String>,
    pub(crate) show_add_agent: bool,
    pub(crate) add_agent_name: String,
    pub(crate) add_agent_custom: bool,
    pub(crate) show_add_bead: bool,
    pub(crate) nb_title: String,
    pub(crate) nb_desc: String,
    pub(crate) nb_type: String,
    pub(crate) nb_priority: i64,
    pub(crate) nb_assignee: Option<String>,
    pub(crate) nb_parent: String,
    pub(crate) nb_release: String,
    /// Free-text buffer for creating a new release from the detail panel.
    pub(crate) release_buf: String,
    /// Whether the inline "new release" entry is showing in the detail panel.
    pub(crate) adding_release: bool,
    /// One-shot: request focus on the new-release field on the next frame.
    pub(crate) focus_release: bool,
    /// Buffer / state for the inline external-ref (Jira key) editor in detail.
    pub(crate) jira_buf: String,
    pub(crate) editing_jira: bool,
    pub(crate) focus_jira: bool,
    /// Workflow editor modal: open flag + the working copy being edited.
    pub(crate) show_workflow_editor: bool,
    pub(crate) editing_schema: crate::schema::WorkflowSchema,
    /// Release name pending "convert to epic" confirmation.
    pub(crate) confirm_convert: Option<String>,
    pub(crate) list_error: Option<String>,
    pub(crate) loading_list: bool,

    pub(crate) search: String,
    pub(crate) filter_status: Option<String>,
    pub(crate) filter_priority: Option<i64>,
    pub(crate) filter_assignee: Option<String>,
    /// Filter by release name (`release:<name>` label). None = all.
    pub(crate) filter_release: Option<String>,
    /// Filter by external-tracker key presence: Some(true)=has, Some(false)=none.
    pub(crate) filter_jira: Option<bool>,
    pub(crate) sort: Sort,
    pub(crate) view: View,
    pub(crate) theme_mode: ThemeMode,
    pub(crate) applied_dark: Option<bool>,
    pub(crate) live: bool,
    pub(crate) watch_mtime: Option<std::time::SystemTime>,

    pub(crate) selected: Option<String>,
    pub(crate) detail: Option<Issue>,
    pub(crate) detail_md: String,
    pub(crate) comments_md: Vec<String>,
    pub(crate) md_cache: CommonMarkCache,
    pub(crate) detail_error: Option<String>,
    pub(crate) history: Result<Vec<HistoryEntry>, String>,
    pub(crate) loading_detail: bool,
    /// Per-workspace LRU cache of fetched bead details (stale-while-revalidate).
    pub(crate) detail_cache: crate::lru::Lru<String, CachedDetail>,
    /// `bd history` is fetched lazily when the History tab is first opened.
    pub(crate) loading_history: bool,
    pub(crate) history_loaded: bool,
    pub(crate) detail_tab: DetailTab,
    /// The comment-search index (`bd export`) is built lazily on first search.
    pub(crate) comment_index_loaded: bool,
    pub(crate) loading_comment_index: bool,
    /// Background mutations in flight; while > 0 the live watcher skips reloads
    /// so an optimistic change isn't clobbered mid-write.
    pub(crate) pending_mutations: usize,
}

impl App {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        t::install_fonts(&cc.egui_ctx);
        t::apply(&cc.egui_ctx, false);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let (tx, rx) = channel();
        let registry = load_registry();
        // Resume the last workspace if it still exists; otherwise show selector.
        // A CLI arg overrides.
        let arg = std::env::args().nth(1);
        let resume = arg
            .clone()
            .filter(|p| has_beads_dir(p))
            .or_else(|| registry.last.clone().filter(|p| has_beads_dir(p)));
        let workspace = resume.clone().unwrap_or_default();
        let in_workspace = resume.is_some();
        let watch_mtime = beads_event_mtime(&workspace);
        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
            tx,
            rx,
            workspace,
            registry,
            in_workspace,
            show_add: false,
            add_path: String::new(),
            add_error: None,
            issues: Vec::new(),
            events: Vec::new(),
            comment_index: std::collections::HashMap::new(),
            board_col_rects: std::collections::HashMap::new(),
            roles: Vec::new(),
            workflow_statuses: Vec::new(),
            workflow_schema: std::rc::Rc::new(crate::schema::WorkflowSchema::default()),
            select_mode: false,
            selected_ids: std::collections::HashSet::new(),
            select_anchor: None,
            visible_order: Vec::new(),
            confirm_bulk_delete: false,
            action_error: None,
            confirm_delete: None,
            confirm_delete_agent: None,
            show_add_agent: false,
            add_agent_name: String::new(),
            add_agent_custom: false,
            show_add_bead: false,
            nb_title: String::new(),
            nb_desc: String::new(),
            nb_type: "task".into(),
            nb_priority: 2,
            nb_assignee: None,
            nb_parent: String::new(),
            nb_release: String::new(),
            release_buf: String::new(),
            adding_release: false,
            focus_release: false,
            jira_buf: String::new(),
            editing_jira: false,
            focus_jira: false,
            show_workflow_editor: false,
            editing_schema: crate::schema::WorkflowSchema::default(),
            confirm_convert: None,
            list_error: None,
            loading_list: false,
            search: String::new(),
            filter_status: None,
            filter_priority: None,
            filter_assignee: None,
            filter_release: None,
            filter_jira: None,
            sort: Sort::Priority,
            view: View::Board,
            theme_mode: ThemeMode::Auto,
            applied_dark: None,
            live: true,
            watch_mtime,
            selected: None,
            detail: None,
            detail_md: String::new(),
            comments_md: Vec::new(),
            md_cache: CommonMarkCache::default(),
            detail_error: None,
            history: Ok(Vec::new()),
            loading_detail: false,
            detail_cache: crate::lru::Lru::new(64),
            loading_history: false,
            history_loaded: false,
            detail_tab: DetailTab::Comments,
            comment_index_loaded: false,
            loading_comment_index: false,
            pending_mutations: 0,
        };
        if app.in_workspace {
            app.reload();
        }
        app
    }

    /// Open a workspace by project path: persist, reset state, load.
    pub(crate) fn open_workspace(&mut self, path: String) {
        self.workspace = path.clone();
        self.in_workspace = true;
        self.registry.last = Some(path.clone());
        save_registry(&self.registry);
        self.issues.clear();
        self.events.clear();
        self.selected = None;
        self.detail = None;
        self.detail_cache.clear();
        self.list_error = None;
        // A selection is workspace-scoped — never carry it across workspaces.
        self.select_mode = false;
        self.selected_ids.clear();
        self.watch_mtime = beads_event_mtime(&path);
        self.reload();
    }

    pub(crate) fn go_back(&mut self) {
        self.in_workspace = false;
        self.selected = None;
        self.detail = None;
        self.detail_cache.clear();
        self.select_mode = false;
        self.selected_ids.clear();
    }

    pub(crate) fn add_workspace(&mut self, path: String) {
        let path = path.trim().trim_end_matches('/').to_string();
        if path.is_empty() {
            self.add_error = Some("Enter a path".into());
            return;
        }
        if !has_beads_dir(&path) {
            self.add_error = Some(format!("No .beads directory found in {path}"));
            return;
        }
        if !self.registry.workspaces.iter().any(|w| w.path == path) {
            self.registry.workspaces.push(WorkspaceEntry { name: basename(&path), path: path.clone() });
        }
        save_registry(&self.registry);
        self.show_add = false;
        self.add_path.clear();
        self.add_error = None;
        self.open_workspace(path);
    }

    pub(crate) fn remove_workspace(&mut self, path: &str) {
        self.registry.workspaces.retain(|w| w.path != path);
        if self.registry.last.as_deref() == Some(path) {
            self.registry.last = None;
        }
        save_registry(&self.registry);
    }

    // ---- Workspace selector screen ----
    pub(crate) fn reconcile_theme(&mut self, ctx: &egui::Context) {
        let want_dark = match self.theme_mode {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::Auto => matches!(ctx.system_theme(), Some(egui::Theme::Dark)),
        };
        if self.applied_dark != Some(want_dark) {
            t::apply(ctx, want_dark);
            self.applied_dark = Some(want_dark);
        }
    }

    pub(crate) fn reload(&mut self) {
        if self.workspace.is_empty() {
            return;
        }
        self.loading_list = true;
        self.list_error = None;
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        thread::spawn(move || {
            // Run the independent `bd` subprocesses concurrently so wall time is
            // the slowest single call, not their sum. (`bd export` for comment
            // search is no longer here — it's built lazily on first search.)
            let h_issues = {
                let ws = ws.clone();
                thread::spawn(move || bd::list_all(&ws))
            };
            let h_roles = {
                let ws = ws.clone();
                thread::spawn(move || bd::read_roles(&ws))
            };
            let h_statuses = {
                let ws = ws.clone();
                thread::spawn(move || bd::workflow_statuses(&ws))
            };
            let h_schema = {
                let ws = ws.clone();
                thread::spawn(move || crate::schema::read_workflow_schema(&ws))
            };
            let events = bd::read_interactions(&ws);
            let issues = h_issues.join().unwrap_or_else(|_| Ok(Vec::new()));
            let roles = h_roles.join().unwrap_or_default();
            let statuses = h_statuses.join().unwrap_or_default();
            let schema = h_schema.join().unwrap_or_default();
            let _ = tx.send(Msg::Loaded { issues, events, roles, statuses, schema });
            ctx.request_repaint();
        });
    }

    /// Run a mutation (bd/initech) in a background thread, then refresh.
    pub(crate) fn run_cmd(&mut self, program: &str, args: Vec<String>, reselect: Option<String>) {
        self.pending_mutations += 1;
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        let program = program.to_string();
        thread::spawn(move || {
            let error = bd::run_cmd(&ws, &program, &args).err();
            let _ = tx.send(Msg::Mutated { reselect, error, optimistic: false });
            ctx.request_repaint();
        });
    }

    /// Reassign a bead's release label: drop the old `release:` label (if any)
    /// then add the new one. Both run in one background thread, then refresh.
    pub(crate) fn set_release(&mut self, id: &str, current: Option<String>, new: Option<String>) {
        self.pending_mutations += 1;
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        let id = id.to_string();
        thread::spawn(move || {
            let error = (|| {
                if let Some(cur) = &current {
                    bd::run_cmd(&ws, "bd", &["label".into(), "remove".into(), id.clone(), format!("{RELEASE_PREFIX}{cur}")])?;
                }
                if let Some(n) = &new {
                    bd::run_cmd(&ws, "bd", &["label".into(), "add".into(), id.clone(), format!("{RELEASE_PREFIX}{n}")])?;
                }
                Ok::<_, String>(())
            })()
            .err();
            let _ = tx.send(Msg::Mutated { reselect: Some(id), error, optimistic: false });
            ctx.request_repaint();
        });
    }

    pub(crate) fn bd_update(&mut self, id: &str, flag: &str, value: &str) {
        self.run_cmd(
            "bd",
            vec!["update".into(), id.into(), flag.into(), value.into()],
            Some(id.into()),
        );
    }

    /// Fire a mutation without reloading on success — caller already patched local state.
    pub(crate) fn run_cmd_optimistic(&mut self, program: &str, args: Vec<String>, reselect: Option<String>) {
        self.pending_mutations += 1;
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        let program = program.to_string();
        thread::spawn(move || {
            let error = bd::run_cmd(&ws, &program, &args).err();
            let _ = tx.send(Msg::Mutated { reselect, error, optimistic: true });
            ctx.request_repaint();
        });
    }

    /// Immediately patch the status of a bead in local state (optimistic update).
    pub(crate) fn optimistic_status(&mut self, id: &str, new_status: &str) {
        if let Some(issue) = self.issues.iter_mut().find(|i| i.id == id) {
            issue.status = new_status.to_string();
        }
        // Also patch the open detail panel if it's the same bead.
        if let Some(detail) = self.detail.as_mut() {
            if detail.id == id {
                detail.status = new_status.to_string();
            }
        }
    }

    /// Archive (or unarchive) the given beads AND all their descendants, so
    /// archiving an epic cascades to its children. Optimistic: patches labels
    /// locally and only reloads on error.
    pub(crate) fn set_archived(&mut self, roots: &[String], archive: bool) {
        if roots.is_empty() {
            return;
        }
        // Expand to every descendant via the parent chain (handles sub-epics).
        let mut targets: std::collections::HashSet<String> = roots.iter().cloned().collect();
        loop {
            let before = targets.len();
            for i in &self.issues {
                if let Some(par) = &i.parent {
                    if targets.contains(par) {
                        targets.insert(i.id.clone());
                    }
                }
            }
            if targets.len() == before {
                break;
            }
        }
        let patch = |labels: &mut Vec<String>| {
            let has = labels.iter().any(|l| l.eq_ignore_ascii_case("archived"));
            if archive && !has {
                labels.push("archived".to_string());
            } else if !archive {
                labels.retain(|l| !l.eq_ignore_ascii_case("archived"));
            }
        };
        for issue in self.issues.iter_mut().filter(|i| targets.contains(&i.id)) {
            patch(&mut issue.labels);
        }
        if let Some(d) = self.detail.as_mut() {
            if targets.contains(&d.id) {
                patch(&mut d.labels);
            }
        }
        let op = if archive { "add" } else { "remove" };
        let mut ids: Vec<String> = targets.into_iter().collect();
        ids.sort();
        let mut args = vec!["label".to_string(), op.to_string()];
        args.extend(ids);
        args.push("archived".to_string());
        self.run_cmd_optimistic("bd", args, None);
    }

    /// Open the workflow editor, seeding the working copy from the loaded schema
    /// — or synthesizing one from `bd statuses` when no schema file exists yet,
    /// so the user always starts from the workspace's real states.
    pub(crate) fn open_workflow_editor(&mut self) {
        let mut s = (*self.workflow_schema).clone();
        if s.states.is_empty() {
            s.states = self
                .workflow_statuses
                .iter()
                .map(|sd| crate::schema::StateDef {
                    name: sd.name.clone(),
                    label: None,
                    color: None,
                    category: (!sd.category.is_empty() && sd.category != "unspecified")
                        .then(|| sd.category.clone()),
                    side: false,
                    owner: None,
                })
                .collect();
        }
        self.editing_schema = s;
        self.show_workflow_editor = true;
        self.action_error = None;
    }

    /// Serialize the edited schema to `.beads/deck-workflow.json` and reload so
    /// the change takes effect immediately.
    pub(crate) fn save_workflow_schema(&mut self) {
        self.editing_schema.states.retain(|s| !s.name.trim().is_empty());
        let path = format!("{}/.beads/deck-workflow.json", self.workspace);
        match serde_json::to_string_pretty(&self.editing_schema) {
            Ok(json) => match std::fs::write(&path, json) {
                Ok(_) => {
                    self.show_workflow_editor = false;
                    self.reload();
                }
                Err(e) => self.action_error = Some(format!("write deck-workflow.json: {e}")),
            },
            Err(e) => self.action_error = Some(format!("serialize schema: {e}")),
        }
    }

    /// Roster of selectable agents: initech roles ∪ assignees, sorted.
    pub(crate) fn agent_roster(&self) -> Vec<String> {
        let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for r in &self.roles {
            set.insert(r.clone());
        }
        for a in self.assignees() {
            set.insert(a);
        }
        set.into_iter().collect()
    }

    pub(crate) fn select(&mut self, id: String) {
        self.selected = Some(id.clone());
        self.detail_error = None;
        self.history = Ok(Vec::new());
        self.history_loaded = false;
        self.loading_history = false;
        // Show the cached detail instantly (if any); always refresh in background.
        if let Some(c) = self.detail_cache.get(&id).cloned() {
            self.detail = Some(c.issue);
            self.detail_md = c.md;
            self.comments_md = c.comments_md;
            self.loading_detail = false;
        } else {
            self.detail = None;
            self.detail_md = String::new();
            self.comments_md = Vec::new();
            self.loading_detail = true;
        }
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        thread::spawn(move || {
            let issue = bd::show(&ws, &id);
            let _ = tx.send(Msg::Detail { id, issue });
            ctx.request_repaint();
        });
    }

    /// Fetch `bd history` for the selected bead — called lazily the first time
    /// the History tab is shown (it's the slowest detail call, ~2s).
    pub(crate) fn ensure_history(&mut self) {
        if self.history_loaded || self.loading_history {
            return;
        }
        let Some(id) = self.selected.clone() else { return };
        self.loading_history = true;
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        thread::spawn(move || {
            let history = bd::history(&ws, &id);
            let _ = tx.send(Msg::History { id, history });
            ctx.request_repaint();
        });
    }

    /// Build the comment-body search index (`bd export`) in the background, only
    /// when a search is active and it isn't already loaded — keeps it off the
    /// hot reload path.
    pub(crate) fn ensure_comment_index(&mut self) {
        if self.search.is_empty() || self.comment_index_loaded || self.loading_comment_index {
            return;
        }
        self.loading_comment_index = true;
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        thread::spawn(move || {
            let map = bd::comment_index(&ws);
            let _ = tx.send(Msg::CommentIndex { map });
            ctx.request_repaint();
        });
    }

    /// Add/remove a bead from the bulk selection.
    pub(crate) fn toggle_select(&mut self, id: String) {
        if !self.selected_ids.remove(&id) {
            self.selected_ids.insert(id);
        }
    }

    /// Resolve a selection click in select-mode, OS-style:
    /// - shift + a prior anchor → select the inclusive range between the anchor
    ///   and the clicked bead in the current visual order (anchor preserved);
    /// - otherwise → toggle the bead and set it as the new anchor.
    pub(crate) fn apply_select(&mut self, id: String, shift: bool) {
        if shift {
            if let Some(anchor) = self.select_anchor.clone() {
                let a = self.visible_order.iter().position(|x| *x == anchor);
                let b = self.visible_order.iter().position(|x| *x == id);
                if let (Some(a), Some(b)) = (a, b) {
                    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                    let range: Vec<String> = self.visible_order[lo..=hi].to_vec();
                    for x in range {
                        self.selected_ids.insert(x);
                    }
                    return; // anchor stays put for further range extension
                }
            }
        }
        self.toggle_select(id.clone());
        self.select_anchor = self.selected_ids.contains(&id).then_some(id);
    }

    pub(crate) fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Loaded { issues, events, roles, statuses, schema } => {
                    self.loading_list = false;
                    self.events = events;
                    self.roles = roles;
                    self.workflow_statuses = statuses;
                    self.workflow_schema = std::rc::Rc::new(schema);
                    crate::schema::set_wf(self.workflow_schema.clone());
                    // Comment index is now stale relative to the new data; it is
                    // rebuilt lazily the next time a search is active.
                    self.comment_index_loaded = false;
                    self.watch_mtime = beads_event_mtime(&self.workspace);
                    match issues {
                        Ok(v) => {
                            eprintln!(
                                "[beads-deck] loaded {} beads, {} events",
                                v.len(),
                                self.events.len()
                            );
                            self.issues = v;
                        }
                        Err(e) => {
                            eprintln!("[beads-deck] list error: {e}");
                            self.list_error = Some(e);
                        }
                    }
                }
                Msg::Detail { id, issue } => {
                    let is_current = self.selected.as_deref() == Some(&id);
                    match issue {
                        Ok(i) => {
                            // Preprocess mermaid ONCE here (may spawn mmdc), not
                            // every frame — and cache it for instant reopen.
                            let md = markdown::preprocess(&i.description);
                            let comments_md: Vec<String> =
                                i.comments.iter().map(|c| markdown::preprocess(&c.text)).collect();
                            self.detail_cache.insert(
                                id.clone(),
                                CachedDetail { issue: i.clone(), md: md.clone(), comments_md: comments_md.clone() },
                            );
                            if is_current {
                                self.loading_detail = false;
                                self.detail_md = md;
                                self.comments_md = comments_md;
                                self.detail = Some(i);
                            }
                        }
                        Err(e) => {
                            if is_current {
                                self.loading_detail = false;
                                self.detail_error = Some(e);
                            }
                        }
                    }
                }
                Msg::History { id, history } => {
                    if self.selected.as_deref() == Some(&id) {
                        self.history = history;
                        self.loading_history = false;
                        self.history_loaded = true;
                    }
                }
                Msg::CommentIndex { map } => {
                    self.comment_index = map;
                    self.comment_index_loaded = true;
                    self.loading_comment_index = false;
                }
                Msg::Mutated { reselect, error, optimistic } => {
                    self.pending_mutations = self.pending_mutations.saturating_sub(1);
                    self.action_error = error.clone();
                    if !optimistic || error.is_some() {
                        // Full reload: either non-optimistic mutation, or need to revert.
                        self.reload();
                    }
                    if let Some(id) = reselect {
                        self.select(id);
                    }
                }
            }
        }
    }

}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.reconcile_theme(ctx);
        self.drain();
        self.ensure_comment_index();

        // No active workspace → show the selector screen.
        if !self.in_workspace {
            self.selector_screen(ctx);
            if self.show_add {
                self.add_modal(ctx);
            }
            return;
        }

        // Real-time: poll the event log mtime; auto-reload on change. Skip while a
        // mutation is in flight so its (slow) background write doesn't trigger a
        // reload that clobbers the optimistic UI mid-operation.
        if self.live {
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
            if !self.loading_list && self.pending_mutations == 0 {
                let m = beads_event_mtime(&self.workspace);
                if m.is_some() && m != self.watch_mtime {
                    self.reload();
                }
            }
        }

        self.top_bar(ctx);

        let p = t::pal();
        if let Some(err) = self.list_error.clone() {
            egui::TopBottomPanel::top("err")
                .frame(egui::Frame::none().fill(p.red_t).inner_margin(Margin::symmetric(12.0, 6.0)))
                .show(ctx, |ui| {
                    ui.colored_label(p.red_d, format!("{} {err}", t::ic::WARNING));
                });
        }

        egui::SidePanel::right("detail")
            .resizable(true)
            .default_width(480.0)
            .width_range(340.0..=1000.0)
            .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin::same(t::SP_LG)))
            .show(ctx, |ui| self.detail_panel(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.page).inner_margin(Margin::same(t::SP_MD)))
            .show(ctx, |ui| match self.view {
                View::Tree => self.tree_view(ui),
                View::Board => self.board_view(ui),
                View::Releases => self.releases_view(ui),
                View::Activity => self.activity_view(ui),
            });

        self.bulk_action_bar(ctx);
        self.confirm_delete_modal(ctx);
        self.confirm_convert_modal(ctx);
        self.confirm_bulk_delete_modal(ctx);
        self.confirm_delete_agent_modal(ctx);
        if self.show_add_agent {
            self.add_agent_modal(ctx);
        }
        if self.show_add_bead {
            self.add_bead_modal(ctx);
        }
        if self.show_workflow_editor {
            self.workflow_editor_modal(ctx);
        }
    }
}
