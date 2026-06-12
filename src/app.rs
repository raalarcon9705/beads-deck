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
    /// Release name pending "convert to epic" confirmation.
    pub(crate) confirm_convert: Option<String>,
    pub(crate) list_error: Option<String>,
    pub(crate) loading_list: bool,

    pub(crate) search: String,
    pub(crate) filter_status: Option<String>,
    pub(crate) filter_priority: Option<i64>,
    pub(crate) filter_assignee: Option<String>,
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
    pub(crate) detail_tab: DetailTab,
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
            confirm_convert: None,
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
    pub(crate) fn open_workspace(&mut self, path: String) {
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

    pub(crate) fn go_back(&mut self) {
        self.in_workspace = false;
        self.selected = None;
        self.detail = None;
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
            let issues = bd::list_all(&ws);
            let events = bd::read_interactions(&ws);
            let roles = bd::read_roles(&ws);
            let comment_index = bd::comment_index(&ws);
            let _ = tx.send(Msg::Loaded { issues, events, roles, comment_index });
            ctx.request_repaint();
        });
    }

    /// Run a mutation (bd/initech) in a background thread, then refresh.
    pub(crate) fn run_cmd(&self, program: &str, args: Vec<String>, reselect: Option<String>) {
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
    pub(crate) fn set_release(&self, id: &str, current: Option<String>, new: Option<String>) {
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

    pub(crate) fn bd_update(&self, id: &str, flag: &str, value: &str) {
        self.run_cmd(
            "bd",
            vec!["update".into(), id.into(), flag.into(), value.into()],
            Some(id.into()),
        );
    }

    /// Fire a mutation without reloading on success — caller already patched local state.
    pub(crate) fn run_cmd_optimistic(&self, program: &str, args: Vec<String>, reselect: Option<String>) {
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

    pub(crate) fn drain(&mut self) {
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

        self.confirm_delete_modal(ctx);
        self.confirm_convert_modal(ctx);
        self.confirm_delete_agent_modal(ctx);
        if self.show_add_agent {
            self.add_agent_modal(ctx);
        }
        if self.show_add_bead {
            self.add_bead_modal(ctx);
        }
    }
}
