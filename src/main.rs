#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Beads Deck — a lightweight native dashboard for the `bd` (beads) issue
//! tracker, with a Jira-like UI driven by the design tokens in `theme`.

mod bd;
mod markdown;
mod theme;

use bd::{HistoryEntry, Interaction, Issue};
use chrono::{DateTime, Utc};
use eframe::egui;
use egui_commonmark::CommonMarkCache;
use serde::{Deserialize, Serialize};
use egui::{Margin, RichText, Rounding};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use theme as t;

/// Beadbox-faithful bucketing. Precedence: Archived > Backlog > Active.
/// - Archived = carries the `archived` label.
/// - Backlog  = priority P4 (bd's "backlog" priority).
fn is_archived(i: &Issue) -> bool {
    i.labels.iter().any(|l| l.eq_ignore_ascii_case("archived"))
}
fn is_backlog(i: &Issue) -> bool {
    !is_archived(i) && i.priority == 4
}

const STATUS_ORDER: &[&str] = &[
    "open",
    "in_progress",
    "blocked",
    "ready_for_qa",
    "in_qa",
    "qa_passed",
    "ready_to_ship",
    "closed",
    "deferred",
];

fn status_rank(s: &str) -> usize {
    STATUS_ORDER.iter().position(|x| *x == s).unwrap_or(99)
}

/// mtime of the workspace event log — the cheap real-time change signal.
fn beads_event_mtime(ws: &str) -> Option<std::time::SystemTime> {
    std::fs::metadata(format!("{ws}/.beads/interactions.jsonl"))
        .and_then(|m| m.modified())
        .ok()
}

fn has_beads_dir(path: &str) -> bool {
    std::path::Path::new(path).join(".beads").is_dir()
}

fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Replace the home prefix with `~` for display.
fn short_path(path: &str) -> String {
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

#[derive(Clone, Serialize, Deserialize)]
struct WorkspaceEntry {
    name: String,
    path: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Registry {
    #[serde(default)]
    workspaces: Vec<WorkspaceEntry>,
    #[serde(default)]
    last: Option<String>,
}

fn registry_path() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(std::path::Path::new(&home).join(".beads-deck").join("registry.json"))
}

fn load_registry() -> Registry {
    if let Some(p) = registry_path() {
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(r) = serde_json::from_str::<Registry>(&s) {
                return r;
            }
        }
    }
    import_from_beadbox().unwrap_or_default()
}

fn save_registry(reg: &Registry) {
    if let Some(p) = registry_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(reg) {
            let _ = std::fs::write(p, s);
        }
    }
}

/// First-run seed: import local workspaces from Beadbox's registry if present.
fn import_from_beadbox() -> Option<Registry> {
    let home = std::env::var("HOME").ok()?;
    let p = std::path::Path::new(&home).join(".beadbox").join("registry.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()?;
    let mut workspaces = Vec::new();
    for w in v.get("workspaces")?.as_array()? {
        let name = w.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if let Some(path) = w.get("local").and_then(|l| l.get("path")).and_then(|x| x.as_str()) {
            // Beadbox stores the `.beads` dir; we want the project dir.
            let path = path.strip_suffix("/.beads").unwrap_or(path).to_string();
            workspaces.push(WorkspaceEntry { name, path });
        }
    }
    Some(Registry { workspaces, last: None })
}

#[derive(PartialEq, Clone, Copy)]
enum Sort {
    Priority,
    StatusClosedFirst,
    Updated,
    Created,
    Id,
}

impl Sort {
    fn label(self) -> &'static str {
        match self {
            Sort::Priority => "Priority",
            Sort::StatusClosedFirst => "Status (Closed first)",
            Sort::Updated => "Recently updated",
            Sort::Created => "Recently created",
            Sort::Id => "ID",
        }
    }
}

enum Msg {
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
enum View {
    Board,
    Tree,
    Activity,
}

#[derive(PartialEq, Clone, Copy)]
enum ThemeMode {
    Auto,
    Light,
    Dark,
}

#[derive(PartialEq, Clone, Copy)]
enum DetailTab {
    Details,
    Comments,
    History,
}

/// A pending write action collected from the detail panel.
enum BeadAction {
    Status(String),
    Priority(i64),
    Assignee(Option<String>),
    ArchiveToggle(bool),
    Backlog,
    Delete,
}

struct App {
    ctx: egui::Context,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,

    workspace: String,
    registry: Registry,
    in_workspace: bool,
    show_add: bool,
    add_path: String,
    add_error: Option<String>,
    issues: Vec<Issue>,
    events: Vec<Interaction>,
    comment_index: std::collections::HashMap<String, String>,
    board_col_rects: std::collections::HashMap<String, egui::Rect>,
    roles: Vec<String>,
    action_error: Option<String>,
    confirm_delete: Option<String>,
    confirm_delete_agent: Option<String>,
    show_add_agent: bool,
    add_agent_name: String,
    add_agent_custom: bool,
    show_add_bead: bool,
    nb_title: String,
    nb_desc: String,
    nb_type: String,
    nb_priority: i64,
    nb_assignee: Option<String>,
    nb_parent: String,
    list_error: Option<String>,
    loading_list: bool,

    search: String,
    filter_status: Option<String>,
    filter_priority: Option<i64>,
    filter_assignee: Option<String>,
    sort: Sort,
    view: View,
    theme_mode: ThemeMode,
    applied_dark: Option<bool>,
    live: bool,
    watch_mtime: Option<std::time::SystemTime>,

    selected: Option<String>,
    detail: Option<Issue>,
    detail_md: String,
    comments_md: Vec<String>,
    md_cache: CommonMarkCache,
    detail_error: Option<String>,
    history: Result<Vec<HistoryEntry>, String>,
    loading_detail: bool,
    detail_tab: DetailTab,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
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
            list_error: None,
            loading_list: false,
            search: String::new(),
            filter_status: None,
            filter_priority: None,
            filter_assignee: None,
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
            detail_tab: DetailTab::Comments,
        };
        if app.in_workspace {
            app.reload();
        }
        app
    }

    /// Open a workspace by project path: persist, reset state, load.
    fn open_workspace(&mut self, path: String) {
        self.workspace = path.clone();
        self.in_workspace = true;
        self.registry.last = Some(path.clone());
        save_registry(&self.registry);
        self.issues.clear();
        self.events.clear();
        self.selected = None;
        self.detail = None;
        self.list_error = None;
        self.watch_mtime = beads_event_mtime(&path);
        self.reload();
    }

    fn go_back(&mut self) {
        self.in_workspace = false;
        self.selected = None;
        self.detail = None;
    }

    fn add_workspace(&mut self, path: String) {
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

    fn remove_workspace(&mut self, path: &str) {
        self.registry.workspaces.retain(|w| w.path != path);
        if self.registry.last.as_deref() == Some(path) {
            self.registry.last = None;
        }
        save_registry(&self.registry);
    }

    // ---- Workspace selector screen ----
    fn selector_screen(&mut self, ctx: &egui::Context) {
        let p = t::pal();
        let mut open: Option<String> = None;
        let mut remove: Option<String> = None;
        let mut add = false;
        let entries = self.registry.workspaces.clone();

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.page).inner_margin(Margin::same(24.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(36.0);
                        ui.add(egui::Image::new(egui::include_image!("../assets/logo.svg")).fit_to_exact_size(egui::vec2(72.0, 72.0)));
                        ui.add_space(t::SP_SM);
                        ui.label(RichText::new("Select a workspace").size(28.0).strong().color(p.text));
                        ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).color(p.text_sub));
                        ui.add_space(24.0);
                    });
                    // Centered grid with a max width.
                    let max_w = 1040.0_f32.min(ui.available_width());
                    let pad = ((ui.available_width() - max_w) / 2.0).max(0.0);
                    ui.horizontal(|ui| {
                        ui.add_space(pad);
                        ui.allocate_ui(egui::vec2(max_w, 0.0), |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for w in &entries {
                                    if let Some(act) = workspace_card(ui, &w.name, &w.path) {
                                        match act {
                                            CardAction::Open => open = Some(w.path.clone()),
                                            CardAction::Remove => remove = Some(w.path.clone()),
                                        }
                                    }
                                }
                                if add_workspace_card(ui) {
                                    add = true;
                                }
                            });
                        });
                    });
                });
            });

        if let Some(path) = open {
            self.open_workspace(path);
        }
        if let Some(path) = remove {
            self.remove_workspace(&path);
        }
        if add {
            self.show_add = true;
            self.add_error = None;
        }
    }

    // ---- Add Workspace modal ----
    fn add_modal(&mut self, ctx: &egui::Context) {
        let p = t::pal();
        let mut do_add = false;
        let mut cancel = false;
        egui::Window::new(RichText::new("Add Workspace").strong())
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label(RichText::new("Point to a project folder that contains a .beads directory.").color(p.text_sub));
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.add_path)
                            .hint_text("~/Projects/my-project")
                            .desired_width(360.0),
                    );
                    if ui.button("\u{1F4C1}").on_hover_text("Browse…").clicked() {
                        let mut dialog = rfd::FileDialog::new();
                        if let Ok(home) = std::env::var("HOME") {
                            dialog = dialog.set_directory(home);
                        }
                        if let Some(path) = dialog.pick_folder() {
                            self.add_path = path.display().to_string();
                            self.add_error = None;
                        }
                    }
                });
                if let Some(e) = &self.add_error {
                    ui.add_space(t::SP_XS);
                    ui.colored_label(p.red_d, e);
                }
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Add").color(egui::Color32::WHITE)).fill(p.green))
                            .clicked()
                        {
                            do_add = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if cancel {
            self.show_add = false;
            self.add_error = None;
            self.add_path.clear();
        }
        if do_add {
            let path = self.add_path.clone();
            self.add_workspace(path);
        }
    }

    fn reconcile_theme(&mut self, ctx: &egui::Context) {
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

    fn reload(&mut self) {
        if self.workspace.is_empty() {
            return;
        }
        self.loading_list = true;
        self.list_error = None;
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        thread::spawn(move || {
            let issues = bd::list_all(&ws);
            let events = bd::read_interactions(&ws);
            let roles = bd::read_roles(&ws);
            let comment_index = bd::comment_index(&ws);
            let _ = tx.send(Msg::Loaded { issues, events, roles, comment_index });
            ctx.request_repaint();
        });
    }

    /// Run a mutation (bd/initech) in a background thread, then refresh.
    fn run_cmd(&self, program: &str, args: Vec<String>, reselect: Option<String>) {
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        let program = program.to_string();
        thread::spawn(move || {
            let error = bd::run_cmd(&ws, &program, &args).err();
            let _ = tx.send(Msg::Mutated { reselect, error, optimistic: false });
            ctx.request_repaint();
        });
    }

    fn bd_update(&self, id: &str, flag: &str, value: &str) {
        self.run_cmd(
            "bd",
            vec!["update".into(), id.into(), flag.into(), value.into()],
            Some(id.into()),
        );
    }

    /// Fire a mutation without reloading on success — caller already patched local state.
    fn run_cmd_optimistic(&self, program: &str, args: Vec<String>, reselect: Option<String>) {
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        let program = program.to_string();
        thread::spawn(move || {
            let error = bd::run_cmd(&ws, &program, &args).err();
            let _ = tx.send(Msg::Mutated { reselect, error, optimistic: true });
            ctx.request_repaint();
        });
    }

    /// Immediately patch the status of a bead in local state (optimistic update).
    fn optimistic_status(&mut self, id: &str, new_status: &str) {
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

    /// Roster of selectable agents: initech roles ∪ assignees, sorted.
    fn agent_roster(&self) -> Vec<String> {
        let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for r in &self.roles {
            set.insert(r.clone());
        }
        for a in self.assignees() {
            set.insert(a);
        }
        set.into_iter().collect()
    }

    fn select(&mut self, id: String) {
        self.selected = Some(id.clone());
        self.detail = None;
        self.detail_md = String::new();
        self.comments_md = Vec::new();
        self.detail_error = None;
        self.history = Ok(Vec::new());
        self.loading_detail = true;
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        thread::spawn(move || {
            let issue = bd::show(&ws, &id);
            let history = bd::history(&ws, &id);
            let _ = tx.send(Msg::Detail { id, issue, history });
            ctx.request_repaint();
        });
    }

    fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Loaded { issues, events, roles, comment_index } => {
                    self.loading_list = false;
                    self.events = events;
                    self.roles = roles;
                    self.comment_index = comment_index;
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
                Msg::Detail { id, issue, history } => {
                    if self.selected.as_deref() == Some(&id) {
                        self.loading_detail = false;
                        match issue {
                            Ok(i) => {
                                // Preprocess mermaid ONCE here (may spawn mmdc),
                                // not every frame in the renderer.
                                self.detail_md = markdown::preprocess(&i.description);
                                self.comments_md = i
                                    .comments
                                    .iter()
                                    .map(|c| markdown::preprocess(&c.text))
                                    .collect();
                                self.detail = Some(i);
                            }
                            Err(e) => self.detail_error = Some(e),
                        }
                        self.history = history;
                    }
                }
                Msg::Mutated { reselect, error, optimistic } => {
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

    fn passes_filter(&self, i: &Issue) -> bool {
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
    fn apply_sort(&self, mut v: Vec<usize>) -> Vec<usize> {
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
    fn statuses_present(&self) -> Vec<String> {
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

    /// Unique assignees present in the loaded beads (sorted).
    fn assignees(&self) -> Vec<String> {
        let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for i in &self.issues {
            if let Some(a) = i.assignee.as_deref().filter(|a| !a.is_empty()) {
                set.insert(a.to_string());
            }
        }
        set.into_iter().collect()
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.reconcile_theme(ctx);
        self.drain();

        // No active workspace → show the selector screen.
        if !self.in_workspace {
            self.selector_screen(ctx);
            if self.show_add {
                self.add_modal(ctx);
            }
            return;
        }

        // Real-time: poll the event log mtime; auto-reload on change.
        if self.live {
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
            if !self.loading_list {
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
                    ui.colored_label(p.red_d, format!("\u{26A0} {err}"));
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
                View::Activity => self.activity_view(ui),
            });

        self.confirm_delete_modal(ctx);
        self.confirm_delete_agent_modal(ctx);
        if self.show_add_agent {
            self.add_agent_modal(ctx);
        }
        if self.show_add_bead {
            self.add_bead_modal(ctx);
        }
    }
}

impl App {
    fn add_bead_modal(&mut self, ctx: &egui::Context) {
        const FIELD_W: f32 = 300.0;
        let p = t::pal();
        let roster = self.agent_roster();
        let mut epics: Vec<(String, String)> = self
            .issues
            .iter()
            .filter(|i| i.issue_type == "epic")
            .map(|i| (i.id.clone(), i.title.clone()))
            .collect();
        epics.sort_by(|a, b| a.0.cmp(&b.0));
        let (mut create, mut cancel) = (false, false);
        egui::Window::new(RichText::new("New Bead").strong())
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(460.0);
                let cap = |ui: &mut egui::Ui, s: &str| {
                    ui.label(RichText::new(s).size(t::FS_CAPTION).strong().color(p.text_sub));
                };
                cap(ui, "Title");
                ui.add(egui::TextEdit::singleline(&mut self.nb_title).hint_text("Short summary").desired_width(f32::INFINITY));
                ui.add_space(t::SP_SM);

                egui::Grid::new("nb_grid")
                    .num_columns(2)
                    .spacing([t::SP_MD, t::SP_SM])
                    .show(ui, |ui| {
                        cap(ui, "Type");
                        egui::ComboBox::from_id_salt("nb_type")
                            .width(FIELD_W)
                            .selected_text(t::title_case(&self.nb_type))
                            .show_ui(ui, |ui| {
                                for ty in ["epic", "feature", "task", "bug", "chore"] {
                                    ui.selectable_value(&mut self.nb_type, ty.to_string(), t::title_case(ty));
                                }
                            });
                        ui.end_row();

                        cap(ui, "Priority");
                        egui::ComboBox::from_id_salt("nb_prio")
                            .width(FIELD_W)
                            .selected_text(format!("P{}", self.nb_priority))
                            .show_ui(ui, |ui| {
                                for pr in 0..=4 {
                                    ui.selectable_value(&mut self.nb_priority, pr, format!("P{pr}"));
                                }
                            });
                        ui.end_row();

                        cap(ui, "Assignee");
                        egui::ComboBox::from_id_salt("nb_asg")
                            .width(FIELD_W)
                            .selected_text(self.nb_assignee.clone().unwrap_or_else(|| "Unassigned".into()))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.nb_assignee, None, "Unassigned");
                                for a in &roster {
                                    ui.selectable_value(&mut self.nb_assignee, Some(a.clone()), a.clone());
                                }
                            });
                        ui.end_row();

                        cap(ui, "Parent epic");
                        egui::ComboBox::from_id_salt("nb_parent")
                            .width(FIELD_W)
                            .selected_text(if self.nb_parent.is_empty() {
                                "None".to_string()
                            } else {
                                self.nb_parent.clone()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.nb_parent, String::new(), "None");
                                for (id, title) in &epics {
                                    let mut label = format!("{id}  {title}");
                                    if label.chars().count() > 48 {
                                        label = label.chars().take(48).collect::<String>() + "…";
                                    }
                                    ui.selectable_value(&mut self.nb_parent, id.clone(), label);
                                }
                            });
                        ui.end_row();
                    });

                ui.add_space(t::SP_SM);
                cap(ui, "Description (optional)");
                ui.add(egui::TextEdit::multiline(&mut self.nb_desc).desired_rows(4).desired_width(f32::INFINITY));
                if let Some(err) = self.action_error.clone() {
                    ui.add_space(t::SP_XS);
                    ui.colored_label(p.red_d, err);
                }
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let enabled = !self.nb_title.trim().is_empty();
                        if ui
                            .add_enabled(enabled, egui::Button::new(RichText::new("Create").color(egui::Color32::WHITE)).fill(p.green))
                            .clicked()
                        {
                            create = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });

        if cancel {
            self.show_add_bead = false;
            self.action_error = None;
        }
        if create {
            let mut args = vec![
                "create".to_string(),
                "--title".into(),
                self.nb_title.trim().to_string(),
                "--type".into(),
                self.nb_type.clone(),
                "--priority".into(),
                format!("P{}", self.nb_priority),
            ];
            if let Some(a) = &self.nb_assignee {
                args.push("--assignee".into());
                args.push(a.clone());
            }
            if !self.nb_parent.trim().is_empty() {
                args.push("--parent".into());
                args.push(self.nb_parent.trim().to_string());
            }
            if !self.nb_desc.trim().is_empty() {
                args.push("-d".into());
                args.push(self.nb_desc.trim().to_string());
            }
            self.run_cmd("bd", args, None);
            // reset form
            self.show_add_bead = false;
            self.nb_title.clear();
            self.nb_desc.clear();
            self.nb_parent.clear();
            self.nb_assignee = None;
            self.nb_type = "task".into();
            self.nb_priority = 2;
        }
    }
}

impl App {
    fn confirm_delete_modal(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete.clone() else { return };
        let p = t::pal();
        let (mut yes, mut no) = (false, false);
        egui::Window::new(RichText::new("Delete bead?").strong())
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new(format!("Permanently delete {id} and clean up references? This cannot be undone.")).color(p.text_sub));
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("Delete").color(egui::Color32::WHITE)).fill(p.red)).clicked() {
                            yes = true;
                        }
                        if ui.button("Cancel").clicked() {
                            no = true;
                        }
                    });
                });
            });
        if no {
            self.confirm_delete = None;
        }
        if yes {
            self.confirm_delete = None;
            self.selected = None;
            self.detail = None;
            self.run_cmd("bd", vec!["delete".into(), id], None);
        }
    }

    fn confirm_delete_agent_modal(&mut self, ctx: &egui::Context) {
        let Some(name) = self.confirm_delete_agent.clone() else { return };
        let p = t::pal();
        let (mut yes, mut no) = (false, false);
        egui::Window::new(RichText::new("Remove agent?").strong())
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new(format!("Remove agent '{name}' from initech.yaml? (workspace dir is preserved)")).color(p.text_sub));
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("Remove").color(egui::Color32::WHITE)).fill(p.red)).clicked() {
                            yes = true;
                        }
                        if ui.button("Cancel").clicked() {
                            no = true;
                        }
                    });
                });
            });
        if no {
            self.confirm_delete_agent = None;
        }
        if yes {
            self.confirm_delete_agent = None;
            self.run_cmd("initech", vec!["delete-agent".into(), name], None);
        }
    }

    fn add_agent_modal(&mut self, ctx: &egui::Context) {
        let p = t::pal();
        let (mut do_add, mut cancel) = (false, false);
        egui::Window::new(RichText::new("Add Agent").strong())
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new("Role name (e.g. eng3, qa3, ops) — scaffolds a workspace and registers it in initech.yaml.").color(p.text_sub));
                ui.add_space(t::SP_MD);
                ui.add(egui::TextEdit::singleline(&mut self.add_agent_name).hint_text("eng3").desired_width(340.0));
                ui.add_space(t::SP_XS);
                ui.checkbox(&mut self.add_agent_custom, "Custom role (skip catalog check)");
                if let Some(err) = self.action_error.clone() {
                    ui.add_space(t::SP_XS);
                    ui.colored_label(p.red_d, err);
                }
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("Add").color(egui::Color32::WHITE)).fill(p.green)).clicked() {
                            do_add = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if cancel {
            self.show_add_agent = false;
            self.add_agent_name.clear();
            self.action_error = None;
        }
        if do_add {
            let name = self.add_agent_name.trim().to_string();
            if !name.is_empty() {
                let mut args = vec!["add-agent".to_string(), name];
                if self.add_agent_custom {
                    args.push("--custom".into());
                }
                self.run_cmd("initech", args, None);
                self.show_add_agent = false;
                self.add_agent_name.clear();
            }
        }
    }
}

impl App {
    fn top_bar(&mut self, ctx: &egui::Context) {
        let p = t::pal();
        let statuses = self.statuses_present();
        let roster = self.agent_roster();
        egui::TopBottomPanel::top("top")
            .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin::symmetric(12.0, 8.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("\u{2190} Back").on_hover_text(&self.workspace).clicked() {
                        self.go_back();
                    }
                    ui.add_space(t::SP_SM);
                    ui.add(egui::Image::new(egui::include_image!("../assets/logo.svg")).fit_to_exact_size(egui::vec2(24.0, 24.0)));
                    ui.add_space(t::SP_XS);
                    ui.label(RichText::new(basename(&self.workspace)).size(t::FS_H1).strong().color(p.text));
                    ui.separator();
                    if ui
                        .add(egui::Button::new(RichText::new("+ New bead").color(egui::Color32::WHITE)).fill(p.green))
                        .clicked()
                    {
                        self.show_add_bead = true;
                        self.action_error = None;
                    }
                    if ui.button("\u{27F3} Reload").clicked() {
                        self.reload();
                    }
                    let live_color = if self.live { p.green } else { p.text_sub };
                    if ui
                        .selectable_label(self.live, RichText::new("\u{25CF} Live").color(live_color))
                        .clicked()
                    {
                        self.live = !self.live;
                    }
                    if self.loading_list {
                        ui.spinner();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Theme selector
                        egui::ComboBox::from_id_salt("theme")
                            .selected_text(match self.theme_mode {
                                ThemeMode::Auto => "\u{1F5A5} Auto",
                                ThemeMode::Light => "\u{2600} Light",
                                ThemeMode::Dark => "\u{1F319} Dark",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.theme_mode, ThemeMode::Auto, "\u{1F5A5} Auto");
                                ui.selectable_value(&mut self.theme_mode, ThemeMode::Light, "\u{2600} Light");
                                ui.selectable_value(&mut self.theme_mode, ThemeMode::Dark, "\u{1F319} Dark");
                            });
                        ui.separator();
                        ui.selectable_value(&mut self.view, View::Activity, "\u{1F4C8} Activity");
                        ui.selectable_value(&mut self.view, View::Tree, "\u{1F333} Tree");
                        ui.selectable_value(&mut self.view, View::Board, "\u{25A6} Board");
                        ui.separator();
                        ui.label(RichText::new(format!("{} beads", self.issues.len())).color(p.text_sub));
                    });
                });
                if self.view != View::Activity {
                ui.add_space(t::SP_XS);
                ui.horizontal(|ui| {
                    // Search input (icon inside a rounded frame).
                    egui::Frame::none()
                        .fill(p.surface_alt)
                        .stroke(egui::Stroke::new(1.0, p.border))
                        .rounding(Rounding::same(t::R_MD))
                        .inner_margin(Margin::symmetric(8.0, 4.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("\u{1F50E}").color(p.text_sub));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.search)
                                        .hint_text("Search by title or ID…")
                                        .desired_width(220.0)
                                        .frame(false),
                                );
                            });
                        });

                    egui::ComboBox::from_id_salt("st")
                        .width(130.0)
                        .selected_text(
                            self.filter_status
                                .as_ref()
                                .map(|s| t::status_style(s).label)
                                .unwrap_or_else(|| "All Status".into()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.filter_status, None, "All Status");
                            for s in &statuses {
                                ui.selectable_value(&mut self.filter_status, Some(s.clone()), t::status_style(s).label);
                            }
                        });

                    egui::ComboBox::from_id_salt("pr")
                        .width(120.0)
                        .selected_text(
                            self.filter_priority
                                .map(|p| format!("P{p}"))
                                .unwrap_or_else(|| "All Priority".into()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.filter_priority, None, "All Priority");
                            for p in 0..=4 {
                                ui.selectable_value(&mut self.filter_priority, Some(p), format!("P{p}"));
                            }
                        });

                    egui::ComboBox::from_id_salt("asg")
                        .width(140.0)
                        .selected_text(
                            self.filter_assignee
                                .clone()
                                .unwrap_or_else(|| "All Assignees".into()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.filter_assignee, None, "All Assignees");
                            for a in &roster {
                                ui.selectable_value(&mut self.filter_assignee, Some(a.clone()), a.clone());
                            }
                        });

                    ui.add_space(t::SP_SM);
                    ui.label(RichText::new("\u{21C5}").color(p.text_sub));
                    egui::ComboBox::from_id_salt("sort")
                        .width(170.0)
                        .selected_text(self.sort.label())
                        .show_ui(ui, |ui| {
                            for s in [
                                Sort::Priority,
                                Sort::StatusClosedFirst,
                                Sort::Updated,
                                Sort::Created,
                                Sort::Id,
                            ] {
                                ui.selectable_value(&mut self.sort, s, s.label());
                            }
                        });

                    if self.filter_status.is_some()
                        || self.filter_priority.is_some()
                        || self.filter_assignee.is_some()
                        || !self.search.is_empty()
                    {
                        if ui.button("Clear").clicked() {
                            self.filter_status = None;
                            self.filter_priority = None;
                            self.filter_assignee = None;
                            self.search.clear();
                        }
                    }
                });
                }
            });
    }

    // ---- Board ----
    fn board_view(&mut self, ui: &mut egui::Ui) {
        let mut clicked: Option<String> = None;
        let col_h = ui.available_height();
        let cols = self.statuses_present();

        // Check if a drag was just released over a column.
        let drag_released = ui.input(|i| i.pointer.any_released());
        let pointer_pos = ui.input(|i| i.pointer.hover_pos());
        let is_dragging = egui::DragAndDrop::has_any_payload(ui.ctx());

        // Collect (status, col_rect) during rendering so we can hit-test on release.
        let mut col_rects: Vec<(String, egui::Rect)> = Vec::new();

        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for status in &cols {
                    let s = t::status_style(status);
                    let mut item_idx: Vec<usize> = self
                        .issues
                        .iter()
                        .enumerate()
                        .filter(|(_, i)| {
                            &i.status == status
                                && !is_archived(i)
                                && !is_backlog(i)
                                && self.passes_filter(i)
                        })
                        .map(|(idx, _)| idx)
                        .collect();
                    if item_idx.is_empty() {
                        continue;
                    }
                    item_idx = self.apply_sort(item_idx);
                    let items: Vec<Issue> = item_idx.iter().map(|&i| self.issues[i].clone()).collect();

                    // Is the cursor hovering over this column while dragging?
                    // col_rect will be NOTHING on the first frame; that's fine.
                    let col_rect = self.board_col_rects.get(status).copied().unwrap_or(egui::Rect::NOTHING);
                    let is_drop_target = is_dragging
                        && pointer_pos.map(|p| col_rect.contains(p)).unwrap_or(false);

                    let (stroke_color, stroke_w) = if is_drop_target {
                        (s.fg, 2.0)
                    } else {
                        (egui::Color32::TRANSPARENT, 0.0)
                    };

                    let resp = ui.allocate_ui_with_layout(
                        egui::vec2(t::COL_W, col_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(t::COL_W);
                            ui.set_max_width(t::COL_W);
                            egui::Frame::none()
                                .fill(s.bg)
                                .rounding(Rounding::same(t::R_MD))
                                .stroke(egui::Stroke::new(stroke_w, stroke_color))
                                .inner_margin(Margin::symmetric(10.0, 6.0))
                                .show(ui, |ui| {
                                    ui.set_width(t::COL_W - 4.0);
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(s.label.to_uppercase()).color(s.fg).size(t::FS_CAPTION).strong());
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(format!("{}", items.len())).color(s.fg).strong());
                                        });
                                    });
                                });
                            ui.add_space(t::SP_SM);
                            egui::ScrollArea::vertical()
                                .id_salt(status)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_width(t::COL_W - 4.0);
                                    for i in &items {
                                        if self.draggable_card(ui, i) {
                                            clicked = Some(i.id.clone());
                                        }
                                        ui.add_space(t::SP_SM);
                                    }
                                });
                        },
                    );
                    col_rects.push((status.clone(), resp.response.rect));
                    self.board_col_rects.insert(status.clone(), resp.response.rect);
                    ui.add_space(t::SP_SM);
                }
            });
        });

        // Resolve a completed drag-drop.
        if drag_released {
            if let Some(bead_id) = egui::DragAndDrop::take_payload::<String>(ui.ctx()) {
                if let Some(pos) = pointer_pos {
                    if let Some((target_status, _)) = col_rects.iter().find(|(_, r)| r.contains(pos)) {
                        let current = self.issues.iter().find(|i| i.id == *bead_id)
                            .map(|i| i.status.clone()).unwrap_or_default();
                        if *target_status != current {
                            // Optimistic: move card in UI immediately, sync in background.
                            self.optimistic_status(&bead_id, target_status);
                            self.run_cmd_optimistic(
                                "bd",
                                vec!["update".into(), (*bead_id).clone(), "--status".into(), target_status.clone()],
                                Some((*bead_id).clone()),
                            );
                        }
                    }
                }
            }
        }

        if let Some(id) = clicked {
            self.select(id);
        }
    }

    fn draggable_card(&self, ui: &mut egui::Ui, i: &Issue) -> bool {
        let p = t::pal();
        let selected = self.selected.as_deref() == Some(&i.id);
        let is_being_dragged = egui::DragAndDrop::payload::<String>(ui.ctx())
            .map(|pay| *pay == i.id).unwrap_or(false);

        // Render the card content, faded if it's the one being dragged.
        let opacity = if is_being_dragged { 0.35 } else { 1.0 };
        let card_resp = ui.add_enabled_ui(true, |ui| {
            ui.set_opacity(opacity);
            t::card_frame(selected).show(ui, |ui| {
                ui.set_width(t::CARD_W);
                ui.style_mut().interaction.selectable_labels = false;
                ui.label(RichText::new(&i.title).size(t::FS_BODY).color(p.text));
                ui.add_space(t::SP_SM);
                ui.horizontal(|ui| {
                    let (glyph, tc) = t::type_glyph(&i.issue_type);
                    ui.label(RichText::new(glyph).color(tc));
                    t::copyable_id(ui, &i.id, t::FS_CAPTION);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(a) = i.assignee.as_deref().filter(|a| !a.is_empty()) {
                            t::avatar(ui, a, 18.0);
                        }
                        t::priority_lozenge(ui, i.priority);
                        if i.comment_count > 0 {
                            ui.label(RichText::new(format!("\u{1F4AC}{}", i.comment_count)).size(t::FS_CAPTION).color(p.text_sub));
                        }
                        if i.dependency_count > 0 {
                            ui.label(RichText::new(format!("\u{26D4}{}", i.dependency_count)).size(t::FS_CAPTION).color(p.text_sub));
                        }
                    });
                });
            }).response
        }).inner;

        // Overlay the full card rect with a drag+click sense so it wins over
        // child label events — this is the standard egui D&D pattern.
        let drag_resp = ui.interact(
            card_resp.rect,
            egui::Id::new(("card_drag", &i.id)),
            egui::Sense::click_and_drag(),
        );

        if drag_resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), i.id.clone());
        }

        // Floating ghost follows the cursor while dragging.
        if is_being_dragged {
            if let Some(pos) = ui.input(|inp| inp.pointer.hover_pos()) {
                egui::show_tooltip_at(
                    ui.ctx(),
                    ui.layer_id(),
                    egui::Id::new("dnd_ghost"),
                    pos + egui::vec2(12.0, 12.0),
                    |ui| {
                        egui::Frame::none()
                            .fill(p.surface)
                            .rounding(Rounding::same(t::R_SM))
                            .stroke(egui::Stroke::new(1.5, p.border))
                            .inner_margin(Margin::same(8.0))
                            .show(ui, |ui| {
                                ui.set_max_width(180.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&i.title).size(t::FS_SMALL).color(p.text),
                                    )
                                    .truncate(),
                                );
                                ui.label(
                                    RichText::new(&i.id)
                                        .monospace()
                                        .size(t::FS_CAPTION)
                                        .color(p.text_sub),
                                );
                            });
                    },
                );
            }
        }

        drag_resp.clicked()
    }

    // ---- Activity ----
    fn activity_view(&mut self, ui: &mut egui::Ui) {
        let p = t::pal();
        let title_of: HashMap<&str, &Issue> =
            self.issues.iter().map(|i| (i.id.as_str(), i)).collect();

        // ---- AGENTS: union of assignees + event actors, latest activity each.
        let mut agent_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        // initech roster (present agents) + assignees + event actors.
        for r in &self.roles {
            agent_names.insert(r.clone());
        }
        for i in &self.issues {
            if let Some(a) = i.assignee.as_deref().filter(|a| !a.is_empty()) {
                agent_names.insert(a.to_string());
            }
        }
        for e in &self.events {
            if !e.actor.is_empty() {
                agent_names.insert(e.actor.clone());
            }
        }
        let roles: std::collections::BTreeSet<String> = self.roles.iter().cloned().collect();
        let mut agents: Vec<AgentCard> = Vec::new();
        for name in &agent_names {
            let mut best: Option<(DateTime<Utc>, String, String)> = None;
            for e in &self.events {
                if &e.actor != name {
                    continue;
                }
                if let Some(ts) = parse_ts(&e.created_at) {
                    if best.as_ref().map(|b| ts > b.0).unwrap_or(true) {
                        best = Some((ts, e.issue_id.clone(), event_action(e)));
                    }
                }
            }
            for i in &self.issues {
                if i.assignee.as_deref() == Some(name.as_str()) {
                    if let Some(ts) = i.updated_at.as_deref().and_then(parse_ts) {
                        if best.as_ref().map(|b| ts > b.0).unwrap_or(true) {
                            best = Some((ts, i.id.clone(), "updated bead".into()));
                        }
                    }
                }
            }
            let (ts, bead, action) = match best {
                Some(b) => (Some(b.0), b.1, b.2),
                None => (None, String::new(), "—".into()),
            };
            let title = title_of.get(bead.as_str()).map(|i| i.title.clone()).unwrap_or_default();
            agents.push(AgentCard { name: name.clone(), ts, bead, title, action });
        }
        agents.sort_by(|a, b| b.ts.cmp(&a.ts));
        let now = Utc::now();
        let active_now = agents
            .iter()
            .filter(|c| c.ts.map(|t| (now - t).num_seconds() < 900).unwrap_or(false))
            .count();

        // ---- PIPELINE: Backlog (P4) + each status, excluding archived.
        let mut pipe: Vec<PipeCard> = Vec::new();
        let backlog: Vec<&Issue> = self
            .issues
            .iter()
            .filter(|i| is_backlog(i))
            .collect();
        pipe.push(PipeCard::build("Backlog".into(), p.text_sub, p.neutral_t, &backlog));
        // Dynamic: statuses present in this project, ordered.
        let statuses = self.statuses_present();
        for s in &statuses {
            let items: Vec<&Issue> = self
                .issues
                .iter()
                .filter(|i| &i.status == s && !is_archived(i) && !is_backlog(i))
                .collect();
            if items.is_empty() {
                continue;
            }
            let st = t::status_style(s);
            pipe.push(PipeCard::build(st.label, st.fg, st.bg, &items));
        }

        // ---- FEED: newest first, grouped by day.
        let mut feed: Vec<&Interaction> = self.events.iter().collect();
        feed.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        feed.truncate(200);
        let feed_items: Vec<FeedItem> = feed
            .iter()
            .filter_map(|e| {
                let ts = parse_ts(&e.created_at)?;
                Some(FeedItem {
                    id: e.issue_id.clone(),
                    title: title_of.get(e.issue_id.as_str()).map(|i| i.title.clone()).unwrap_or_else(|| e.issue_id.clone()),
                    meta: format!("{} {}", e.actor, event_action(e)),
                    ago: ago(ts),
                    day: day_label(ts),
                })
            })
            .collect();
        let total_events = self.events.len();

        let mut clicked: Option<String> = None;
        let mut add_agent = false;
        let mut remove_agent: Option<String> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            ui.horizontal(|ui| {
                ui.label(RichText::new("AGENTS").size(t::FS_CAPTION).strong().color(p.text_sub));
                if ui.small_button("+ Add").on_hover_text("Add agent (initech)").clicked() {
                    add_agent = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{active_now} active now")).size(t::FS_CAPTION).color(p.text_sub));
                });
            });
            egui::ScrollArea::horizontal().id_salt("agents").show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    for c in &agents {
                        let removable = roles.contains(&c.name);
                        match agent_card(ui, c, now, removable) {
                            AgentAction::Open => clicked = Some(c.bead.clone()),
                            AgentAction::Remove => remove_agent = Some(c.name.clone()),
                            AgentAction::None => {}
                        }
                    }
                });
            });

            ui.add_space(t::SP_MD);
            activity_caption(ui, "Pipeline", "");
            egui::ScrollArea::horizontal().id_salt("pipe").show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    for c in &pipe {
                        if pipeline_card(ui, c) {
                            // clicking a sample id navigates
                            clicked = c.first_sample.clone();
                        }
                    }
                });
            });

            ui.add_space(t::SP_MD);
            activity_caption(ui, "Activity", &format!("{total_events} events"));
            ui.add_space(t::SP_XS);
            let mut last_day = String::new();
            for f in &feed_items {
                if f.day != last_day {
                    ui.add_space(t::SP_SM);
                    ui.label(RichText::new(f.day.to_uppercase()).size(t::FS_CAPTION).strong().color(p.text_sub));
                    last_day = f.day.clone();
                }
                if feed_row(ui, f) {
                    clicked = Some(f.id.clone());
                }
            }
        });
        if let Some(id) = clicked {
            self.select(id);
        }
        if add_agent {
            self.show_add_agent = true;
            self.action_error = None;
        }
        if let Some(name) = remove_agent {
            self.confirm_delete_agent = Some(name);
        }
    }

    // ---- Tree ----
    fn tree_view(&mut self, ui: &mut egui::Ui) {
        let p = t::pal();
        // Partition by bucket. Precedence: Archived (label) > Backlog (P4) > Active.
        let mut active: Vec<usize> = Vec::new();
        let mut backlog: Vec<usize> = Vec::new();
        let mut archived: Vec<usize> = Vec::new();
        for (idx, i) in self.issues.iter().enumerate() {
            if is_archived(i) {
                archived.push(idx);
            } else if is_backlog(i) {
                backlog.push(idx);
            } else {
                active.push(idx);
            }
        }
        // Epic tree over the ACTIVE set. Active roots split into Epics vs Loose.
        let active_ids: std::collections::HashSet<&str> =
            active.iter().map(|&i| self.issues[i].id.as_str()).collect();
        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        let mut epic_roots: Vec<usize> = Vec::new();
        let mut loose_roots: Vec<usize> = Vec::new();
        for &idx in &active {
            let i = &self.issues[idx];
            match &i.parent {
                Some(par) if active_ids.contains(par.as_str()) => {
                    children.entry(par.clone()).or_default().push(idx)
                }
                _ if i.issue_type == "epic" => epic_roots.push(idx),
                _ => loose_roots.push(idx),
            }
        }
        let epic_roots = self.apply_sort(epic_roots);
        let loose_roots = self.apply_sort(loose_roots);
        let backlog = self.apply_sort(backlog);
        let archived = self.apply_sort(archived);

        // Visible counts (respect filters).
        let epics_n = epic_roots.iter().filter(|&&r| self.node_visible(r, &children)).count();
        let loose_n = loose_roots.iter().filter(|&&r| self.node_visible(r, &children)).count();
        let backlog_n = backlog.iter().filter(|&&i| self.passes_filter(&self.issues[i])).count();
        let archived_n = archived.iter().filter(|&&i| self.passes_filter(&self.issues[i])).count();

        let mut clicked: Option<String> = None;
        egui::Frame::none()
            .fill(p.surface)
            .rounding(Rounding::same(t::R_LG))
            .stroke(egui::Stroke::new(1.0, p.border))
            .inner_margin(Margin::same(t::SP_SM))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());

                        tree_group(ui, "\u{1F5C2}", "Epics", epics_n, true, |ui| {
                            for &r in &epic_roots {
                                self.tree_node(ui, r, &children, &mut clicked);
                            }
                        });
                        tree_group(ui, "\u{2B1A}", "Loose Beads", loose_n, false, |ui| {
                            for &r in &loose_roots {
                                self.tree_node(ui, r, &children, &mut clicked);
                            }
                        });
                        tree_group(ui, "\u{1F4E5}", "Backlog", backlog_n, false, |ui| {
                            for &i in &backlog {
                                if self.passes_filter(&self.issues[i]) && self.tree_row(ui, &self.issues[i]) {
                                    clicked = Some(self.issues[i].id.clone());
                                }
                            }
                        });
                        tree_group(ui, "\u{1F5C4}", "Archived", archived_n, false, |ui| {
                            for &i in &archived {
                                if self.passes_filter(&self.issues[i]) && self.tree_row(ui, &self.issues[i]) {
                                    clicked = Some(self.issues[i].id.clone());
                                }
                            }
                        });
                    });
            });
        if let Some(id) = clicked {
            self.select(id);
        }
    }

    /// Whether a tree root should render given the active filters.
    fn node_visible(&self, idx: usize, children: &HashMap<String, Vec<usize>>) -> bool {
        if self.passes_filter(&self.issues[idx]) {
            return true;
        }
        if let Some(k) = children.get(&self.issues[idx].id) {
            return k.iter().any(|&c| self.subtree_has_match(c, children));
        }
        false
    }

    fn tree_node(
        &self,
        ui: &mut egui::Ui,
        idx: usize,
        children: &HashMap<String, Vec<usize>>,
        clicked: &mut Option<String>,
    ) {
        let i = &self.issues[idx];
        let kids = children.get(&i.id);
        let visible = self.passes_filter(i)
            || kids
                .map(|k| k.iter().any(|&c| self.subtree_has_match(c, children)))
                .unwrap_or(false);
        if !visible {
            return;
        }
        if let Some(kids) = kids {
            let (glyph, tc) = t::type_glyph(&i.issue_type);
            egui::CollapsingHeader::new(
                RichText::new(format!("{}  {}   {}", glyph, i.id, i.title)).color(tc).strong(),
            )
            .id_salt(&i.id)
            .default_open(true)
            .show(ui, |ui| {
                if self.passes_filter(i) && self.tree_row(ui, i) {
                    *clicked = Some(i.id.clone());
                }
                for &c in kids {
                    self.tree_node(ui, c, children, clicked);
                }
            });
        } else if self.passes_filter(i) && self.tree_row(ui, i) {
            *clicked = Some(i.id.clone());
        }
    }

    fn tree_row(&self, ui: &mut egui::Ui, i: &Issue) -> bool {
        let p = t::pal();
        let selected = self.selected.as_deref() == Some(&i.id);
        let resp = egui::Frame::none()
            .fill(if selected { p.blue_t } else { egui::Color32::TRANSPARENT })
            .rounding(Rounding::same(t::R_SM))
            .inner_margin(Margin::symmetric(6.0, 3.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (glyph, tc) = t::type_glyph(&i.issue_type);
                    ui.label(RichText::new(glyph).color(tc));
                    t::priority_lozenge(ui, i.priority);
                    t::status_lozenge(ui, &i.status);
                    t::copyable_id(ui, &i.id, t::FS_CAPTION);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(a) = i.assignee.as_deref().filter(|a| !a.is_empty()) {
                            t::avatar(ui, a, 20.0);
                        }
                        if i.comment_count > 0 {
                            ui.label(RichText::new(format!("\u{1F4AC}{}", i.comment_count)).size(t::FS_CAPTION).color(p.text_sub));
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(RichText::new(&i.title).color(p.text)).truncate());
                        });
                    });
                });
            })
            .response;
        resp.interact(egui::Sense::click()).clicked()
    }

    fn subtree_has_match(&self, idx: usize, children: &HashMap<String, Vec<usize>>) -> bool {
        let i = &self.issues[idx];
        if self.passes_filter(i) {
            return true;
        }
        if let Some(kids) = children.get(&i.id) {
            return kids.iter().any(|&c| self.subtree_has_match(c, children));
        }
        false
    }

    // ---- Detail ----
    fn detail_panel(&mut self, ui: &mut egui::Ui) {
        let p = t::pal();
        // Always claim the full panel width so narrow states (spinner / empty)
        // never let egui shrink the resizable panel back to default.
        ui.set_min_width(ui.available_width());

        if self.selected.is_none() {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(RichText::new("Select a bead to see details").color(p.text_sub).size(14.0));
            });
            return;
        }
        if self.loading_detail {
            ui.add_space(t::SP_LG);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Loading detail (bd show can be slow)…").color(p.text_sub));
            });
            return;
        }
        if let Some(err) = self.detail_error.clone() {
            ui.colored_label(p.red_d, format!("\u{26A0} {err}"));
            return;
        }
        let Some(i) = self.detail.clone() else { return };
        let mut nav: Option<String> = None;
        let mut action: Option<BeadAction> = None;

        // Options (computed before the UI closures borrow nothing of self).
        let mut status_opts: Vec<String> = STATUS_ORDER.iter().map(|s| s.to_string()).collect();
        for s in self.statuses_present() {
            if !status_opts.contains(&s) {
                status_opts.push(s);
            }
        }
        let roster = self.agent_roster();
        let archived_now = is_archived(&i);
        let backlog_now = is_backlog(&i);

        ui.horizontal(|ui| {
            let (glyph, tc) = t::type_glyph(&i.issue_type);
            ui.label(RichText::new(glyph).size(t::FS_H1).color(tc));
            t::copyable_id(ui, &i.id, t::FS_BODY);
            if let Some(par) = &i.parent {
                ui.label(RichText::new("\u{2191}").size(t::FS_SMALL).color(p.text_sub));
                if t::bead_link(ui, par) {
                    nav = Some(par.clone());
                }
            }
            // Action buttons pinned right.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("\u{1F5D1}").on_hover_text("Delete bead").clicked() {
                    action = Some(BeadAction::Delete);
                }
                let arch_label = if archived_now { "\u{21A9}" } else { "\u{1F5C4}" };
                let arch_hint = if archived_now { "Unarchive" } else { "Archive bead" };
                if ui.button(arch_label).on_hover_text(arch_hint).clicked() {
                    action = Some(BeadAction::ArchiveToggle(archived_now));
                }
                if !backlog_now
                    && ui.button("\u{1F4E5}").on_hover_text("Move to backlog (P4)").clicked()
                {
                    action = Some(BeadAction::Backlog);
                }
            });
        });
        ui.add_space(2.0);
        ui.label(RichText::new(&i.title).size(t::FS_H1).strong().color(p.text));
        ui.add_space(t::SP_SM);
        // Interactive status / priority / assignee.
        ui.horizontal_wrapped(|ui| {
            let cur = t::status_style(&i.status);
            egui::ComboBox::from_id_salt("d_status")
                .selected_text(RichText::new(cur.label).color(cur.fg).strong())
                .show_ui(ui, |ui| {
                    for s in &status_opts {
                        if ui.selectable_label(s == &i.status, t::status_style(s).label).clicked()
                            && s != &i.status
                        {
                            action = Some(BeadAction::Status(s.clone()));
                        }
                    }
                });
            egui::ComboBox::from_id_salt("d_prio")
                .selected_text(format!("P{}", i.priority))
                .show_ui(ui, |ui| {
                    for pr in 0..=4 {
                        if ui.selectable_label(pr == i.priority, format!("P{pr}")).clicked()
                            && pr != i.priority
                        {
                            action = Some(BeadAction::Priority(pr));
                        }
                    }
                });
            let cur_a = i.assignee.clone().unwrap_or_default();
            egui::ComboBox::from_id_salt("d_asg")
                .selected_text(if cur_a.is_empty() { "Unassigned".to_string() } else { cur_a.clone() })
                .show_ui(ui, |ui| {
                    if ui.selectable_label(cur_a.is_empty(), "None").clicked() && !cur_a.is_empty() {
                        action = Some(BeadAction::Assignee(None));
                    }
                    for a in &roster {
                        if ui.selectable_label(&cur_a == a, a.as_str()).clicked() && &cur_a != a {
                            action = Some(BeadAction::Assignee(Some(a.clone())));
                        }
                    }
                });
            if archived_now {
                t::lozenge(ui, "Archived", p.amber_d, p.yellow_t);
            }
        });
        if let Some(err) = self.action_error.clone() {
            ui.colored_label(p.red_d, format!("\u{26A0} {err}"));
        }
        ui.add_space(t::SP_SM);
        ui.separator();

        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.detail_tab, DetailTab::Details, "Details");
            ui.selectable_value(&mut self.detail_tab, DetailTab::Comments, format!("Comments ({})", i.comments.len()));
            ui.selectable_value(&mut self.detail_tab, DetailTab::History, "History");
        });
        ui.add_space(t::SP_SM);

        let tab = self.detail_tab;
        let md = self.detail_md.clone();
        let comments_md = self.comments_md.clone();
        let history = self.history.clone();
        let cache = &mut self.md_cache;
        let inner = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match tab {
                DetailTab::Details => detail_tab(ui, &i, &md, cache),
                DetailTab::Comments => {
                    comments_tab(ui, &i, &comments_md, cache);
                    None
                }
                DetailTab::History => {
                    history_tab(ui, &history);
                    None
                }
            })
            .inner;

        if let Some(id) = nav.or(inner) {
            self.select(id);
        }

        if let Some(act) = action {
            let id = i.id.clone();
            self.action_error = None;
            match act {
                BeadAction::Status(s) => self.bd_update(&id, "--status", &s),
                BeadAction::Priority(pr) => self.bd_update(&id, "--priority", &format!("P{pr}")),
                BeadAction::Assignee(Some(a)) => self.bd_update(&id, "--assignee", &a),
                BeadAction::Assignee(None) => self.bd_update(&id, "--assignee", ""),
                BeadAction::Backlog => self.bd_update(&id, "--priority", "4"),
                BeadAction::ArchiveToggle(now) => {
                    let op = if now { "remove" } else { "add" };
                    self.run_cmd(
                        "bd",
                        vec!["label".into(), op.into(), id.clone(), "archived".into()],
                        Some(id.clone()),
                    );
                }
                BeadAction::Delete => self.confirm_delete = Some(id),
            }
        }
    }
}

/// Returns the id of a bead to navigate to, if a bead link was clicked.
enum CardAction {
    Open,
    Remove,
}

fn workspace_card(ui: &mut egui::Ui, name: &str, path: &str) -> Option<CardAction> {
    const W: f32 = 320.0;
    let p = t::pal();
    let mut action = None;
    ui.allocate_ui_with_layout(
        egui::vec2(W, 112.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(W);
            ui.set_max_width(W);
            t::card_frame(false).show(ui, |ui| {
                ui.set_width(W - 24.0);
                ui.set_min_height(84.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("\u{25CF}").size(10.0).color(p.text_sub));
                    ui.label(RichText::new(name).strong().size(16.0).color(p.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Label::new(RichText::new("\u{00D7}").color(p.text_sub)).sense(egui::Sense::click()))
                            .on_hover_text("Remove")
                            .clicked()
                        {
                            action = Some(CardAction::Remove);
                        }
                    });
                });
                ui.add(egui::Label::new(RichText::new(short_path(path)).size(t::FS_CAPTION).color(p.text_sub)).truncate());
                ui.add_space(t::SP_SM);
                if ui
                    .add(egui::Button::new(RichText::new("Open").color(egui::Color32::WHITE)).fill(p.green))
                    .clicked()
                {
                    action = Some(CardAction::Open);
                }
            });
        },
    );
    ui.add_space(t::SP_SM);
    action
}

fn add_workspace_card(ui: &mut egui::Ui) -> bool {
    const W: f32 = 320.0;
    let p = t::pal();
    let mut clicked = false;
    ui.allocate_ui_with_layout(
        egui::vec2(W, 112.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(W);
            ui.set_max_width(W);
            let resp = egui::Frame::none()
                .fill(p.surface_alt)
                .stroke(egui::Stroke::new(1.0, p.border))
                .rounding(Rounding::same(t::R_MD))
                .inner_margin(Margin::same(10.0))
                .show(ui, |ui| {
                    ui.set_width(W - 24.0);
                    ui.set_min_height(84.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("+").size(22.0).color(p.text_sub));
                        ui.add_space(t::SP_SM);
                        ui.vertical(|ui| {
                            ui.add_space(t::SP_SM);
                            ui.label(RichText::new("Add workspace").strong().color(p.text));
                            ui.label(RichText::new("Browse for a beads project").size(t::FS_CAPTION).color(p.text_sub));
                        });
                    });
                })
                .response;
            if resp.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clicked = true;
            }
        },
    );
    ui.add_space(t::SP_SM);
    clicked
}

/// A top-level collapsible section header (Epics / Loose Beads / Backlog /
/// Archived) with icon, uppercase label and muted count.
fn tree_group(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    count: usize,
    default_open: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    let p = t::pal();
    ui.add_space(2.0);
    egui::CollapsingHeader::new(
        RichText::new(format!("{}   {}   {}", icon, label.to_uppercase(), count))
            .strong()
            .size(t::FS_SMALL)
            .color(p.text_sub),
    )
    .id_salt(label)
    .default_open(default_open)
    .show(ui, body);
}

// ---------------------------------------------------------------------------
// Activity tab data + widgets
// ---------------------------------------------------------------------------

struct AgentCard {
    name: String,
    ts: Option<DateTime<Utc>>,
    bead: String,
    title: String,
    action: String,
}

struct PipeCard {
    label: String,
    fg: egui::Color32,
    bg: egui::Color32,
    count: usize,
    breakdown: String,
    samples: Vec<String>,
    more: usize,
    first_sample: Option<String>,
}

impl PipeCard {
    fn build(label: String, fg: egui::Color32, bg: egui::Color32, items: &[&Issue]) -> Self {
        let count = items.len();
        let mut by: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for i in items {
            *by.entry(i.issue_type.as_str()).or_default() += 1;
        }
        let breakdown = ["bug", "task", "feature", "epic", "chore"]
            .iter()
            .filter_map(|t| by.get(*t).map(|n| format!("{n} {t}{}", if *n == 1 { "" } else { "s" })))
            .collect::<Vec<_>>()
            .join("  ·  ");
        let mut ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
        ids.sort();
        let samples: Vec<String> = ids.iter().take(4).cloned().collect();
        let more = count.saturating_sub(samples.len());
        let first_sample = ids.first().cloned();
        PipeCard { label, fg, bg, count, breakdown, samples, more, first_sample }
    }
}

struct FeedItem {
    id: String,
    title: String,
    meta: String,
    ago: String,
    day: String,
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

fn ago(ts: DateTime<Utc>) -> String {
    let secs = (Utc::now() - ts).num_seconds().max(0);
    if secs < 60 {
        "now".into()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

fn day_label(ts: DateTime<Utc>) -> String {
    let d = ts.date_naive();
    let today = Utc::now().date_naive();
    if d == today {
        "Today".into()
    } else if Some(d) == today.pred_opt() {
        "Yesterday".into()
    } else {
        ts.format("%a, %b %d").to_string()
    }
}

fn event_action(e: &Interaction) -> String {
    match e.field() {
        "status" => format!("\u{2192} {}", t::status_style(&e.new_value()).label),
        "assignee" => {
            let v = e.new_value();
            if v.is_empty() {
                "unassigned".into()
            } else {
                format!("assigned {v}")
            }
        }
        "priority" => format!("set P{}", e.new_value()),
        _ => e.kind.replace('_', " "),
    }
}

fn activity_caption(ui: &mut egui::Ui, label: &str, right: &str) {
    let p = t::pal();
    ui.horizontal(|ui| {
        ui.label(RichText::new(label.to_uppercase()).size(t::FS_CAPTION).strong().color(p.text_sub));
        if !right.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(right).size(t::FS_CAPTION).color(p.text_sub));
            });
        }
    });
}

enum AgentAction {
    Open,
    Remove,
    None,
}

fn agent_card(ui: &mut egui::Ui, c: &AgentCard, now: DateTime<Utc>, removable: bool) -> AgentAction {
    const W: f32 = 210.0;
    let p = t::pal();
    let active = c.ts.map(|t| (now - t).num_seconds() < 900).unwrap_or(false);
    let mut act = AgentAction::None;
    ui.allocate_ui_with_layout(
        egui::vec2(W, 96.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(W);
            ui.set_max_width(W);
            let resp = t::card_frame(false)
                .show(ui, |ui| {
                    ui.set_width(W - 22.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("\u{25CF}").size(9.0).color(if active { p.green } else { p.text_sub }));
                        ui.label(RichText::new(&c.name).strong().color(p.text));
                        if removable {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .add(egui::Label::new(RichText::new("\u{00D7}").color(p.text_sub)).sense(egui::Sense::click()))
                                    .on_hover_text("Remove agent")
                                    .clicked()
                                {
                                    act = AgentAction::Remove;
                                }
                            });
                        }
                    });
                    if !c.bead.is_empty() {
                        t::copyable_id(ui, &c.bead, t::FS_CAPTION);
                        ui.add(egui::Label::new(RichText::new(&c.title).size(t::FS_SMALL).color(p.text)).truncate());
                    }
                    ui.label(RichText::new(&c.action).size(t::FS_CAPTION).color(p.text_sub));
                    if let Some(ts) = c.ts {
                        ui.label(RichText::new(ago(ts)).size(t::FS_CAPTION).color(p.text_sub));
                    }
                })
                .response;
            if matches!(act, AgentAction::None)
                && !c.bead.is_empty()
                && resp.interact(egui::Sense::click()).clicked()
            {
                act = AgentAction::Open;
            }
        },
    );
    ui.add_space(t::SP_SM);
    act
}

fn pipeline_card(ui: &mut egui::Ui, c: &PipeCard) -> bool {
    const W: f32 = 168.0;
    let p = t::pal();
    let mut clicked = false;
    ui.allocate_ui_with_layout(
        egui::vec2(W, 132.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(W);
            ui.set_max_width(W);
            egui::Frame::none()
                .fill(c.bg)
                .rounding(Rounding::same(t::R_MD))
                .stroke(egui::Stroke::new(1.0, p.border))
                .inner_margin(Margin::same(10.0))
                .show(ui, |ui| {
                    ui.set_width(W - 22.0);
                    ui.label(RichText::new(c.label.to_uppercase()).size(t::FS_CAPTION).strong().color(c.fg));
                    ui.label(RichText::new(format!("{}", c.count)).size(22.0).strong().color(c.fg));
                    if !c.breakdown.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&c.breakdown).size(10.0).color(p.text_sub)).truncate());
                    }
                    ui.add_space(2.0);
                    for s in &c.samples {
                        t::copyable_id(ui, s, t::FS_CAPTION);
                    }
                    if c.more > 0 {
                        ui.label(RichText::new(format!("+{} more", c.more)).size(t::FS_CAPTION).color(p.text_sub));
                    }
                });
        },
    );
    ui.add_space(t::SP_SM);
    clicked
}

fn feed_row(ui: &mut egui::Ui, f: &FeedItem) -> bool {
    let p = t::pal();
    let mut clicked = false;
    egui::Frame::none()
        .inner_margin(Margin::symmetric(2.0, 3.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("\u{2192}").color(p.text_sub));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&f.ago).size(t::FS_CAPTION).color(p.text_sub));
                    ui.label(RichText::new(format!("({})", f.meta)).size(t::FS_CAPTION).color(p.text_sub));
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let r = ui.add(
                            egui::Label::new(RichText::new(&f.title).color(p.primary))
                                .truncate()
                                .sense(egui::Sense::click()),
                        );
                        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            clicked = true;
                        }
                    });
                });
            });
        });
    clicked
}

fn detail_tab(ui: &mut egui::Ui, i: &Issue, md: &str, cache: &mut CommonMarkCache) -> Option<String> {
    let p = t::pal();
    let mut nav: Option<String> = None;
    if !i.description.is_empty() {
        t::section(ui, "Description");
        markdown::show(ui, cache, md);
    }
    t::section(ui, "Dependencies");
    if i.dependencies.is_empty() {
        ui.label(RichText::new("None").color(p.text_sub));
    } else {
        for d in &i.dependencies {
            ui.horizontal(|ui| {
                if !d.status.is_empty() {
                    t::status_lozenge(ui, &d.status);
                }
                if t::bead_link(ui, &d.id) {
                    nav = Some(d.id.clone());
                }
                ui.add(egui::Label::new(RichText::new(&d.title).color(p.text)).truncate());
            });
        }
    }
    t::section(ui, "Relations");
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("\u{26D4} Blocked by: {}", i.dependency_count)).color(p.text_sub));
        ui.label(RichText::new(format!("\u{2192} Blocks: {}", i.dependent_count)).color(p.text_sub));
    });
    if let (Some(c), Some(reason)) = (&i.closed_at, &i.close_reason) {
        t::section(ui, "Resolution");
        ui.label(RichText::new(format!("Closed {}", t::short_date(c))).color(p.text_sub));
        ui.label(RichText::new(reason).color(p.text));
    }
    nav
}

fn comments_tab(ui: &mut egui::Ui, i: &Issue, bodies: &[String], cache: &mut CommonMarkCache) {
    let p = t::pal();
    if i.comments.is_empty() {
        ui.add_space(t::SP_SM);
        ui.label(RichText::new("No comments").color(p.text_sub));
        return;
    }
    for (idx, c) in i.comments.iter().enumerate() {
        ui.add_space(t::SP_SM);
        egui::Frame::none()
            .fill(p.surface_alt)
            .rounding(Rounding::same(t::R_MD))
            .stroke(egui::Stroke::new(1.0, p.border))
            .inner_margin(Margin::same(t::SP_SM))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    t::avatar(ui, &c.author, 24.0);
                    ui.label(RichText::new(&c.author).strong().color(p.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(t::short_date(&c.created_at)).color(p.text_sub).size(t::FS_CAPTION));
                    });
                });
                ui.add_space(2.0);
                // Render the comment body as markdown (preprocessed for mermaid).
                let body = bodies.get(idx).map(String::as_str).unwrap_or(&c.text);
                markdown::show(ui, cache, body);
            });
    }
}

fn history_tab(ui: &mut egui::Ui, history: &Result<Vec<HistoryEntry>, String>) {
    let p = t::pal();
    match history {
        Err(e) => {
            ui.add_space(t::SP_SM);
            ui.colored_label(p.amber_d, "History unavailable (known bd 1.0.x bug):");
            ui.label(RichText::new(e).color(p.text_sub).size(t::FS_CAPTION));
        }
        Ok(entries) if entries.is_empty() => {
            ui.add_space(t::SP_SM);
            ui.label(RichText::new("No history").color(p.text_sub));
        }
        Ok(entries) => {
            let mut last_status = String::new();
            for e in entries {
                let st = e.issue.as_ref().map(|s| s.status.clone()).unwrap_or_default();
                let changed = st != last_status && !last_status.is_empty();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t::short_date(&e.commit_date)).color(p.text_sub).size(t::FS_CAPTION));
                    if !st.is_empty() {
                        t::status_lozenge(ui, &st);
                    }
                    if changed {
                        ui.label(RichText::new("transition").size(10.0).italics().color(p.text_sub));
                    }
                    ui.label(RichText::new(&e.committer).size(t::FS_CAPTION).color(p.text_sub));
                });
                last_status = st;
            }
        }
    }
}

/// Rasterize the bundled SVG logo to an RGBA window icon.
fn load_icon() -> Option<egui::IconData> {
    let svg = include_bytes!("../assets/logo.svg");
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).ok()?;
    let size = 256u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let ts = tree.size();
    let scale = size as f32 / ts.width().max(ts.height());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(egui::IconData {
        rgba: pixmap.data().to_vec(),
        width: size,
        height: size,
    })
}

/// GUI apps launched from Finder/Spotlight inherit a minimal PATH that omits
/// Homebrew and friends, so `bd`/`initech`/`dolt`/`mmdc` aren't found. Prepend
/// the usual CLI locations so child processes resolve them.
fn ensure_cli_path() {
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
        format!("{home}/.cargo/bin"),
        format!("{home}/.local/bin"),
    ];
    let current = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<String> = Vec::new();
    for c in candidates {
        if std::path::Path::new(&c).is_dir()
            && !current.split(':').any(|p| p == c)
            && !parts.contains(&c)
        {
            parts.push(c);
        }
    }
    if !parts.is_empty() {
        let new = if current.is_empty() {
            parts.join(":")
        } else {
            format!("{}:{current}", parts.join(":"))
        };
        std::env::set_var("PATH", new);
    }
}

fn main() -> eframe::Result {
    ensure_cli_path();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1320.0, 860.0])
        .with_min_inner_size([900.0, 600.0])
        .with_title("Beads Deck");
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Beads Deck",
        native_options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
